//! The B2F message exchange: the turn-based back-and-forth that actually moves
//! messages once the handshake is done.
//!
//! A session alternates turns. On our turn ([`send_turn`]) we offer our pending
//! messages as proposals, read the other side's accept/reject/defer answers, and
//! send the bodies of the accepted ones — or, with nothing to send, we signal
//! "no more" (`FF`) or "quit" (`FQ`). On the other side's turn
//! ([`receive_turn`]) we read their proposals, verify the batch checksum, answer
//! each one, and pull down the bodies we accepted.
//!
//! These functions work over any reader/writer, so they are exercised with
//! scripted in-memory transports — no network, no transmission. Mirrors
//! `wl2k-go/fbb/b2f.go` (`handleOutbound` / `handleInbound`); no Go ships.

pub mod cms_health;

use std::io::{BufRead, Write};

use cms_health::{CmsAttemptOutcome, CMS_HEALTH};

use super::message::{self, Message};
use super::proposal::{self, Answer, Proposal};
use super::{handshake, lzhuf, secure, transfer, wire};

/// At most this many proposals are offered in a single batch.
const MAX_BATCH: usize = 5;

/// A safety cap on the number of turns in one exchange, so a misbehaving server
/// cannot drive an unbounded loop. A real session is a handful of turns; this is
/// generous headroom for a large mailbox sent in many batches.
const MAX_TURNS: u32 = 1000;

/// A message prepared for sending: its proposal line, its title (the subject,
/// which travels in the framed block header), and its compressed body.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub proposal: Proposal,
    pub title: String,
    pub compressed: Vec<u8>,
}

/// What happened to the messages we offered this turn.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SendOutcome {
    /// MIDs whose bodies we sent (the other side accepted them).
    pub sent: Vec<String>,
    /// MIDs the other side already had (rejected).
    pub rejected: Vec<String>,
    /// MIDs the other side deferred to a later turn.
    pub deferred: Vec<String>,
    /// True if we sent the quit signal (nothing to send and the other side was
    /// also done).
    pub quit_sent: bool,
}

/// What we got from the other side this turn.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReceiveOutcome {
    /// Messages received and parsed.
    pub messages: Vec<Message>,
    /// True if the other side sent the quit signal.
    pub remote_quit: bool,
    /// True if the other side had no more messages to offer.
    pub remote_no_messages: bool,
}

/// What the caller must supply to run a full exchange.
#[derive(Clone)]
pub struct ExchangeConfig {
    /// Our call sign.
    pub mycall: String,
    /// The station we are connecting to (a CMS gateway call, or `SERVICE`).
    pub targetcall: String,
    /// Our grid locator, e.g. `CN87`.
    pub locator: String,
    /// The station password, used only if the server sends a challenge. Supplied
    /// by the caller (from the OS keyring); never stored here.
    pub password: Option<String>,
    /// Which message pool this session belongs to. Determines the routing
    /// flag tag the local mailbox applies to messages received over this
    /// session, and gates outbound delivery (a `RadioOnly` session refuses
    /// to send a `Cms`-tagged message and vice versa, per WLE
    /// `B2Protocol.cs:860-900`). Defaults to [`SessionIntent::Cms`] —
    /// every existing caller predates §2.13 and behaves as a CMS dial.
    pub intent: SessionIntent,
}

/// Manual `Debug` impl for `ExchangeConfig`.
///
/// Redacts `password` per spec §5.3 / alpha-logging tuxlink-qjgx: the
/// password field must never appear in `tracing::debug!(?config, ...)` output.
/// All other fields render with their normal `Debug` output so consumers that
/// grep on debug strings keep working.
impl std::fmt::Debug for ExchangeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangeConfig")
            .field("mycall", &self.mycall)
            .field("targetcall", &self.targetcall)
            .field("locator", &self.locator)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("intent", &self.intent)
            .finish()
    }
}

/// Which message pool a B2F session belongs to.
///
/// Mirrors WLE's `B2SessionType` enum (`B2Protocol.cs:51-60` in the
/// `RMS_Express_v11.0.0.0` decompile, surfaced via the dial-time
/// session-type dropdown in `Main.cs:5820-6040`). Per the deep-dive at
/// [`dev/scratch/winlink-re/findings/client-of-rms-relay.md`] §3.1,
/// each intent maps 1:1 to a single-character [`RoutingFlag`] that the
/// local mailbox uses to gate cross-pool message delivery — e.g., a
/// message stored under flag `R` cannot leave over a `Cms` session and
/// vice versa.
///
/// The intent is **operator-typed** at the dial-target picker; see also
/// `src/connections/sessionTypes.ts` for the user-facing labels the
/// sidebar surfaces.
///
/// ## Diverges from WLE
///
/// - Tuxlink does NOT replicate WLE's `Automatic` / `RMSRelay` runtime-
///   transition variants. The operator's typed intent is the
///   authoritative source of pool membership for outbound messages;
///   post-connect banner parsing (see [`super::relay_banner`])
///   refines the operator's *view* of the remote but does NOT
///   silently re-pool already-composed messages mid-session. The
///   banner parser surfaces what the remote IS so the UI can show a
///   persistent state strip; it does not mutate routing flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionIntent {
    /// Default — talking to the global Winlink CMS, either directly
    /// (Telnet/TLS to `cms.winlink.org:8773`) or via a transparent
    /// relay-to-CMS proxy (deep-dive path 1A's `Use RMS Relay`
    /// checkbox).
    #[default]
    Cms,
    /// R pool — RF-only Hybrid network. Messages never traverse the
    /// internet. Deep-dive path 1B (`TelnetSessionRadioOnly` and the
    /// cross-transport Pactor / Packet / VARA / ARDOP Radio-only
    /// variants).
    RadioOnly,
    /// L pool — store-and-forward at a local RMS Relay "post office".
    /// Operator dials a LAN-local relay endpoint instead of the global
    /// CMS, and messages stay in the local pool until the operator
    /// later forwards them.
    PostOffice,
    /// MESH — Network Post Office. Deep-dive path 1C
    /// (`TelnetMESHSession` with `B2PeerToPeer=false`). Telnet to a
    /// locally-run RMS Relay instance, or via AREDN mesh. Carries no
    /// routing flag at the message layer (the relay tags inbound by
    /// its own configuration).
    Mesh,
    /// Peer-to-peer — direct station, no CMS, no creds, no routing
    /// flag. The local mailbox stores P2P messages unpooled.
    P2p,
}

/// Single-character routing flag tagged on every message that crosses
/// a B2F session, per WLE's `B2Protocol.cs:860-900` (`B2CheckSendMessage`)
/// + `L1125-1155` (inbound `RoutingFlag` tagging on receive).
///
/// `None` means "no flag" — applies to [`SessionIntent::P2p`] and
/// [`SessionIntent::Mesh`] sessions per the WLE behavior; the local
/// mailbox treats unflagged messages as belonging to no pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingFlag {
    /// `C` — CMS-routed message.
    Cms,
    /// `R` — Radio-only / Hybrid-network message.
    RadioOnly,
    /// `L` — Local-RMS-Relay / Post-Office message.
    PostOffice,
}

impl RoutingFlag {
    /// Single ASCII character used to tag the message in the mailbox.
    pub fn as_char(self) -> char {
        match self {
            Self::Cms => 'C',
            Self::RadioOnly => 'R',
            Self::PostOffice => 'L',
        }
    }

    /// Parse the single-character tag from a stored mailbox header.
    /// Case-sensitive — WLE's `B2Protocol.cs:1144-1149` compares against
    /// uppercase literals only.
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'C' => Some(Self::Cms),
            'R' => Some(Self::RadioOnly),
            'L' => Some(Self::PostOffice),
            _ => None,
        }
    }
}

impl SessionIntent {
    /// The routing flag a message takes when it crosses this session.
    /// Returns `None` for [`SessionIntent::P2p`] and [`SessionIntent::Mesh`]
    /// — the local mailbox stores unflagged messages for these intents.
    pub fn routing_flag(self) -> Option<RoutingFlag> {
        match self {
            Self::Cms => Some(RoutingFlag::Cms),
            Self::RadioOnly => Some(RoutingFlag::RadioOnly),
            Self::PostOffice => Some(RoutingFlag::PostOffice),
            Self::Mesh | Self::P2p => None,
        }
    }

    /// True for intents that auto-arm a listener at Open Session (per spec §2 + §3).
    /// Driven by whether the intent has an inbound side: P2p (any peer) and RadioOnly
    /// (R-pool peer) yes; Cms (CMS gateway is outbound-only from the client's view),
    /// PostOffice and Mesh (out of alpha scope) no.
    pub fn auto_arms_listener(self) -> bool {
        matches!(self, SessionIntent::P2p | SessionIntent::RadioOnly)
    }
}

/// The result of a whole exchange.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExchangeResult {
    pub received: Vec<Message>,
    pub sent: Vec<String>,
    pub rejected: Vec<String>,
    pub deferred: Vec<String>,
}

/// Which side of the FBB master/slave split this exchange plays.
///
/// `Dial` (slave/dialer): the remote speaks first (sends its handshake +
/// optional `;PQ` challenge); we read it, answer, then take the first message
/// turn. This is the gateway-dial and peer-dial case.
///
/// `Answer` (master/answerer): WE speak first (send our handshake; clients never
/// challenge), the remote reads it and replies, then the *remote* (slave) takes
/// the first message turn. This is the P2P-listen case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeRole {
    Dial,
    Answer,
}

/// Back-compat entry point: a slave-role (`Dial`) exchange. Existing callers
/// (telnet) and tests use this; new packet callers use [`run_exchange_with_role`].
pub fn run_exchange<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
    wire_log: Option<&dyn Fn(&str)>,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    run_exchange_with_role(reader, writer, ExchangeRole::Dial, config, outbound, decide, wire_log)
}

/// Run a full exchange in the given [`ExchangeRole`]. See the enum docs for the
/// role split. The turn loop after the handshake is identical for both roles.
pub fn run_exchange_with_role<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    role: ExchangeRole,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
    wire_log: Option<&dyn Fn(&str)>,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let span = tracing::info_span!(
        "b2f_exchange",
        target = "tuxlink::winlink::session",
        mycall = %config.mycall,
        targetcall = %config.targetcall,
        role = ?role,
    );
    let _guard = span.enter();

    tracing::info!(
        target: "tuxlink::winlink::session",
        mycall = %config.mycall,
        targetcall = %config.targetcall,
        role = ?role,
        outbound_count = outbound.len(),
        "exchange started",
    );

    let my_turn = match role {
        ExchangeRole::Dial => {
            // Slave: the remote speaks first; answer its challenge if present.
            let remote =
                handshake::read_remote_handshake(reader).map_err(ExchangeError::Handshake)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                remote_sid = %remote.sid,
                has_challenge = remote.challenge.is_some(),
                forwarder_count = remote.forwarders.len(),
                "remote handshake received",
            );
            let token = match (&remote.challenge, &config.password) {
                (Some(challenge), Some(password)) => {
                    tracing::debug!(
                        target: "tuxlink::winlink::session",
                        challenge_len = challenge.len(),
                        "secure-login challenge received; computing response",
                    );
                    Some(secure::secure_login_response(challenge, password))
                }
                (Some(_), None) => {
                    CMS_HEALTH.record_failure(CmsAttemptOutcome::Other("password_required".into()));
                    return Err(ExchangeError::PasswordRequired);
                }
                (None, _) => None,
            };
            let our_handshake = handshake::build_handshake(
                &config.mycall,
                &config.targetcall,
                &config.locator,
                token.as_deref(),
            );
            write_bytes(writer, &our_handshake)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                "dial handshake sent; taking first message turn",
            );
            true // the dialer/slave takes the first message turn
        }
        ExchangeRole::Answer => {
            // Master: WE speak first, sending the master handshake (SID + the FBB
            // `>` prompt that signals the slave our handshake is complete). Clients
            // never challenge, so no `;PQ`; we never answer one, so no `;PR`.
            let our_handshake = handshake::build_master_handshake(
                &config.mycall,
                &config.targetcall,
                &config.locator,
            );
            write_bytes(writer, &our_handshake)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                "master handshake sent; waiting for slave handshake",
            );
            // Read the remote (slave) handshake. A slave sends no `>` prompt, so the
            // master detects its end by the start of the slave's message turn
            // (an `F`-prefixed line); `read_slave_handshake` handles that.
            let remote =
                handshake::read_slave_handshake(reader).map_err(ExchangeError::Handshake)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                remote_sid = %remote.sid,
                forwarder_count = remote.forwarders.len(),
                "slave handshake received; remote takes first turn",
            );
            false // the remote/slave takes the first message turn
        }
    };

    let mut result = ExchangeResult::default();
    let mut remaining = outbound;
    let mut remote_no_messages = false;
    let mut my_turn = my_turn;
    let mut turns = 0u32;

    loop {
        turns += 1;
        if turns > MAX_TURNS {
            tracing::warn!(
                target: "tuxlink::winlink::session",
                turns,
                "exchange exceeded turn cap",
            );
            CMS_HEALTH.record_failure(CmsAttemptOutcome::Other("too_many_turns".into()));
            return Err(ExchangeError::TooManyTurns);
        }
        if my_turn {
            let outcome = send_turn(reader, writer, &remaining, remote_no_messages, wire_log)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                turn = turns,
                sent_count = outcome.sent.len(),
                rejected_count = outcome.rejected.len(),
                deferred_count = outcome.deferred.len(),
                quit_sent = outcome.quit_sent,
                "send turn completed",
            );
            result.sent.extend(outcome.sent);
            result.rejected.extend(outcome.rejected);
            result.deferred.extend(outcome.deferred);
            remaining.clear(); // each message is offered once
            if outcome.quit_sent {
                break;
            }
        } else {
            let outcome = receive_turn(reader, writer, &decide)?;
            tracing::debug!(
                target: "tuxlink::winlink::session",
                turn = turns,
                received_count = outcome.messages.len(),
                remote_no_messages = outcome.remote_no_messages,
                remote_quit = outcome.remote_quit,
                "receive turn completed",
            );
            result.received.extend(outcome.messages);
            remote_no_messages = outcome.remote_no_messages;
            if outcome.remote_quit {
                break;
            }
        }
        my_turn = !my_turn;
    }

    tracing::info!(
        target: "tuxlink::winlink::session",
        mycall = %config.mycall,
        targetcall = %config.targetcall,
        received_count = result.received.len(),
        sent_count = result.sent.len(),
        turns,
        "exchange completed successfully",
    );
    CMS_HEALTH.record_success();
    Ok(result)
}

/// Additive entry point for the smart auth-failure diagnostics (spec §6.3).
///
/// Takes an optional [`B2fEventSink`] alongside the existing `wire_log`
/// closure. Existing callers (telnet, P2P, ARDOP, VARA, packet backends)
/// continue to use [`run_exchange`] / [`run_exchange_with_role`] unchanged —
/// this entry point is ADDITIVE, not a replacement (R1 #2 + R3 #8 finding).
///
/// Per §6.3 + §6.4: emits structured events at each handshake phase, and
/// emits [`PostAuthExchangeStarted`][crate::winlink::b2f_events::B2fEvent::PostAuthExchangeStarted]
/// when the first non-`***` `F`-prefixed protocol byte arrives from the server
/// (the Mode 5 discriminator). Without that event, a `;PR`-rejected drop would
/// mis-classify as Mode 5 ("credentials are fine").
///
/// ## Auth-only contract
///
/// This entry point runs the handshake, classifies the auth result, then quits
/// cleanly (FF + FQ). It does NOT run a full message exchange — no inbound
/// proposal reading, no outbound message sending. The `_outbound`, `_decide`,
/// and `_wire_log` parameters are present for signature consistency with
/// [`run_exchange_with_role`]; they are unused in this auth-only path.
// Auth-only path mirrors run_exchange_with_role's signature shape for
// consistency, which puts it 1 over the clippy::too_many_arguments default
// threshold of 7. Restructuring would diverge the two from each other
// without making either clearer; allowing the lint locally is the right call.
#[allow(unused_variables, clippy::too_many_arguments)]
pub fn run_exchange_with_events<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    config: &ExchangeConfig,
    _outbound: Vec<OutboundMessage>,
    _decide: F,
    _wire_log: Option<&dyn Fn(&str)>,
    events: Option<&dyn super::b2f_events::B2fEventSink>,
    attempt_id: super::b2f_events::AttemptId,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    use super::b2f_events::{B2fEvent, ConnectionPhase};

    // Slave/Dial role: server speaks first.
    let remote = match handshake::read_remote_handshake(reader) {
        Ok(r) => r,
        Err(e) => {
            if let Some(s) = events {
                if let handshake::HandshakeError::RemoteError(raw) = &e {
                    s.push(B2fEvent::RemoteErrorReceived {
                        raw: raw.clone(),
                        attempt_id,
                    });
                }
                s.push(B2fEvent::ConnectionClosed {
                    phase: ConnectionPhase::DuringHandshake,
                    transport_kind: None,
                    attempt_id,
                });
            }
            return Err(ExchangeError::Handshake(e));
        }
    };
    if let Some(s) = events {
        s.push(B2fEvent::RemoteSidReceived {
            sid: remote.sid.clone(),
            attempt_id,
        });
        if remote.challenge.is_some() {
            s.push(B2fEvent::SecureChallengeReceived { attempt_id });
        }
    }

    let token = match (&remote.challenge, &config.password) {
        (Some(challenge), Some(password)) => {
            Some(secure::secure_login_response(challenge, password))
        }
        (Some(_), None) => return Err(ExchangeError::PasswordRequired),
        (None, _) => None,
    };
    let our_handshake = handshake::build_handshake(
        &config.mycall,
        &config.targetcall,
        &config.locator,
        token.as_deref(),
    );
    writer
        .write_all(&our_handshake)
        .map_err(|_| ExchangeError::ConnectionClosed)?;
    if let Some(s) = events {
        if token.is_some() {
            s.push(B2fEvent::SecureResponseSent { attempt_id });
        }
    }

    // Read the first protocol line from the server post-handshake.
    //   `***` prefix  → CMS rejected (Mode 2/3/4/6): emit RemoteErrorReceived,
    //                    do NOT emit PostAuthExchangeStarted.
    //   `F` prefix    → CMS accepted (Mode 5 discriminator): emit
    //                    PostAuthExchangeStarted, then quit cleanly (FF + FQ).
    let first_line = match wire::read_line(reader) {
        Ok(line) => line,
        Err(_) => {
            if let Some(s) = events {
                s.push(B2fEvent::ConnectionClosed {
                    phase: ConnectionPhase::PostHandshake,
                    transport_kind: None,
                    attempt_id,
                });
            }
            return Err(ExchangeError::ConnectionClosed);
        }
    };

    if let Some(rest) = first_line.strip_prefix("***") {
        let raw = rest.trim().to_string();
        let scrubbed = super::redaction::redact_freeform(&raw).into_owned();
        if let Some(s) = events {
            s.push(B2fEvent::RemoteErrorReceived {
                raw: scrubbed.clone(),
                attempt_id,
            });
            s.push(B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id,
            });
        }
        return Err(ExchangeError::RemoteError(scrubbed));
    }

    // Validate the first post-auth line is a recognized B2F command.
    // `starts_with('F')` is too permissive — a malformed server could send
    // `FLOL\r` and trigger a false Mode 5 classification (Codex MAJOR #3).
    let is_valid_b2f_command = first_line == "FF"
        || first_line == "FQ"
        || first_line.starts_with("FA ")
        || first_line.starts_with("FB ")
        || first_line.starts_with("FC ")
        || first_line.starts_with("FD ")
        || first_line.starts_with("F>");
    if is_valid_b2f_command {
        if let Some(s) = events {
            // SPEC §6.4 invariant: PostAuthExchangeStarted fires ONLY here —
            // when the first non-`***` F-prefixed byte proves CMS accepted.
            s.push(B2fEvent::PostAuthExchangeStarted { attempt_id });
        }
        // Auth-only contract: no message exchange. Send FF (nothing to offer)
        // then FQ (quit). The server's first F line is already consumed.
        let _ = writer.write_all(b"FF\r");
        let _ = writer.write_all(b"FQ\r");
        if let Some(s) = events {
            s.push(B2fEvent::ConnectionClosed {
                phase: ConnectionPhase::PostHandshake,
                transport_kind: None,
                attempt_id,
            });
        }
        return Ok(ExchangeResult::default());
    }

    Err(ExchangeError::UnexpectedResponse(first_line))
}

/// Our turn: offer the pending messages, read the answers, send the accepted
/// bodies. With nothing to send, signal "no more" (or "quit" if the other side
/// was also done).
pub fn send_turn<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    outbound: &[OutboundMessage],
    remote_no_messages: bool,
    wire_log: Option<&dyn Fn(&str)>,
) -> Result<SendOutcome, ExchangeError> {
    let mut outcome = SendOutcome::default();

    if outbound.is_empty() {
        if remote_no_messages {
            write_bytes(writer, b"FQ\r")?;
            outcome.quit_sent = true;
        } else {
            write_bytes(writer, b"FF\r")?;
        }
        return Ok(outcome);
    }

    let batch = &outbound[..outbound.len().min(MAX_BATCH)];
    let proposals: Vec<Proposal> = batch.iter().map(|m| m.proposal.clone()).collect();
    for proposal in &proposals {
        let line = proposal.line();
        if let Some(log) = wire_log {
            log(&line);
        }
        write_bytes(writer, line.as_bytes())?;
        write_bytes(writer, b"\r")?;
    }
    write_bytes(writer, proposal::batch_checksum_line(&proposals).as_bytes())?;
    write_bytes(writer, b"\r")?;

    // Read the answer line, skipping comment / pending-message lines.
    let answers = loop {
        let line = read_line(reader)?;
        if let Some(message) = remote_error(&line) {
            return Err(ExchangeError::RemoteError(message));
        }
        if line.starts_with("FS ") {
            if let Some(log) = wire_log {
                log(&line);
            }
            break proposal::parse_answers(&line).map_err(ExchangeError::BadAnswer)?;
        } else if line.starts_with(';') {
            continue;
        } else {
            return Err(ExchangeError::UnexpectedResponse(line));
        }
    };
    if answers.len() != batch.len() {
        return Err(ExchangeError::AnswerCountMismatch);
    }

    for (msg, answer) in batch.iter().zip(answers) {
        let mid = msg.proposal.mid.clone();
        match answer {
            Answer::Accept { resume_offset } => {
                let data = msg.compressed.get(resume_offset..).unwrap_or(&[]);
                write_bytes(writer, &transfer::frame_block(&msg.title, resume_offset, data))?;
                outcome.sent.push(mid);
            }
            Answer::Reject => outcome.rejected.push(mid),
            Answer::Defer => outcome.deferred.push(mid),
        }
    }
    Ok(outcome)
}

/// The other side's turn: read its proposals, verify the batch checksum, answer
/// each (via `decide`), and pull down the bodies we accept.
pub fn receive_turn<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    decide: F,
) -> Result<ReceiveOutcome, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let mut outcome = ReceiveOutcome::default();
    let mut proposals: Vec<Proposal> = Vec::new();
    let mut checksum: u32 = 0;
    let answers: Vec<Answer>;

    loop {
        let line = read_line(reader)?;
        if let Some(message) = remote_error(&line) {
            return Err(ExchangeError::RemoteError(message));
        }
        if line.is_empty() || line.starts_with(';') {
            continue; // comment, pending-message info, or blank
        }
        if line.len() < 2 || !line.starts_with('F') {
            return Err(ExchangeError::UnknownCommand(line));
        }

        match &line[..2] {
            "FA" | "FB" | "FC" | "FD" => {
                for b in line.bytes() {
                    checksum = checksum.wrapping_add(u32::from(b));
                }
                checksum = checksum.wrapping_add(u32::from(b'\r'));
                proposals.push(Proposal::parse(&line).map_err(ExchangeError::BadProposal)?);
            }
            "FF" => {
                outcome.remote_no_messages = true;
                return Ok(outcome);
            }
            "FQ" => {
                outcome.remote_quit = true;
                return Ok(outcome);
            }
            "F>" => {
                let theirs = u8::from_str_radix(line[2..].trim(), 16)
                    .map_err(|_| ExchangeError::ChecksumMismatch)?;
                let ours = (checksum.wrapping_neg() & 0xff) as u8;
                if theirs != ours {
                    return Err(ExchangeError::ChecksumMismatch);
                }
                if proposals.is_empty() {
                    outcome.remote_no_messages = true;
                    return Ok(outcome);
                }
                answers = decide(&proposals);
                if answers.len() != proposals.len() {
                    return Err(ExchangeError::AnswerCountMismatch);
                }
                write_bytes(writer, answer_line(&answers).as_bytes())?;
                break;
            }
            _ => return Err(ExchangeError::UnknownCommand(line)),
        }
    }

    // Read the bodies of the proposals we accepted, in order. Each carries its
    // own headers (Mid, Subject, ...), so the proposal is not needed here.
    for answer in &answers {
        if !matches!(answer, Answer::Accept { .. }) {
            continue;
        }
        let block = transfer::read_block(reader).map_err(ExchangeError::Transfer)?;
        let raw = lzhuf::decompress(&block.data).map_err(ExchangeError::Decompress)?;
        let message = Message::from_bytes(&raw).map_err(ExchangeError::Parse)?;
        outcome.messages.push(message);
    }
    Ok(outcome)
}

/// Build the `FS <answers>\r` line we send back: one symbol per proposal.
fn answer_line(answers: &[Answer]) -> String {
    let mut line = String::from("FS ");
    for answer in answers {
        line.push(match answer {
            Answer::Accept { .. } => '+',
            Answer::Reject => '-',
            Answer::Defer => '=',
        });
    }
    line.push('\r');
    line
}

/// If `line` is a remote error line (`*** message`), return the message. The
/// CMS reports failures this way (e.g. authentication or client-type rejection).
fn remote_error(line: &str) -> Option<String> {
    line.strip_prefix("***").map(|rest| rest.trim().to_string())
}

fn write_bytes<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<(), ExchangeError> {
    writer
        .write_all(bytes)
        .map_err(|_| ExchangeError::ConnectionClosed)
}

fn read_line<R: BufRead>(reader: &mut R) -> Result<String, ExchangeError> {
    wire::read_line(reader).map_err(|_| ExchangeError::ConnectionClosed)
}

/// Why a turn could not be completed.
#[derive(Debug, PartialEq, Eq)]
pub enum ExchangeError {
    /// The connection closed mid-turn.
    ConnectionClosed,
    /// We expected an answer line but got something else.
    UnexpectedResponse(String),
    /// A protocol line we did not recognise.
    UnknownCommand(String),
    /// The proposal batch checksum did not match.
    ChecksumMismatch,
    /// The number of answers did not match the number of proposals.
    AnswerCountMismatch,
    /// A proposal line could not be parsed.
    BadProposal(proposal::ProposalParseError),
    /// An answer line could not be parsed.
    BadAnswer(proposal::AnswerParseError),
    /// A framed block could not be read.
    Transfer(transfer::TransferError),
    /// A message body could not be decompressed.
    Decompress(lzhuf::LzhufError),
    /// A decompressed message could not be parsed.
    Parse(message::ParseError),
    /// The handshake with the server failed.
    Handshake(handshake::HandshakeError),
    /// The server asked for a password but none was provided.
    PasswordRequired,
    /// The remote sent an error line (`*** ...`), e.g. a rejected login or an
    /// unsupported client type.
    RemoteError(String),
    /// The exchange exceeded its turn cap (a misbehaving or looping server).
    TooManyTurns,
}

impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExchangeError::ConnectionClosed => write!(f, "connection closed mid-exchange"),
            ExchangeError::UnexpectedResponse(s) => write!(f, "unexpected response: {s}"),
            ExchangeError::UnknownCommand(s) => write!(f, "unknown command: {s}"),
            ExchangeError::ChecksumMismatch => write!(f, "proposal batch checksum mismatch"),
            ExchangeError::AnswerCountMismatch => write!(f, "answer count did not match proposal count"),
            ExchangeError::BadProposal(e) => write!(f, "bad proposal: {e:?}"),
            ExchangeError::BadAnswer(e) => write!(f, "bad answer: {e:?}"),
            ExchangeError::Transfer(e) => write!(f, "transfer error: {e:?}"),
            ExchangeError::Decompress(e) => write!(f, "decompression error: {e:?}"),
            ExchangeError::Parse(e) => write!(f, "message parse error: {e:?}"),
            ExchangeError::Handshake(e) => write!(f, "handshake error: {e:?}"),
            ExchangeError::PasswordRequired => write!(f, "server required a password but none was configured"),
            ExchangeError::RemoteError(s) => write!(f, "remote error: {s}"),
            ExchangeError::TooManyTurns => write!(f, "exchange exceeded turn cap"),
        }
    }
}

impl std::error::Error for ExchangeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::message::Message;
    use crate::winlink::proposal::{batch_checksum_line, Answer};
    use crate::winlink::transfer;
    use std::io::Cursor;

    // ============================================================================
    // SessionIntent + RoutingFlag (tuxlink-kld3 — RMS-Relay client foundation)
    // ============================================================================

    // ============================================================================
    // ExchangeConfig::Debug redaction (alpha-logging §5.3 / tuxlink-qjgx Task 2)
    // ============================================================================

    #[test]
    fn exchange_config_debug_redacts_password() {
        let cfg = ExchangeConfig {
            mycall: "K0ABC".into(),
            targetcall: "K6XXX-10".into(),
            locator: "CN87".into(),
            password: Some("hunter2hunter2".into()),
            intent: SessionIntent::Cms,
        };
        let dbg = format!("{cfg:?}");
        assert!(
            !dbg.contains("hunter2hunter2"),
            "Debug must not contain the real password; got: {dbg}"
        );
        assert!(
            dbg.contains("<redacted>") || dbg.contains("Some(\"<redacted>\")"),
            "Debug must show redacted marker; got: {dbg}"
        );
        assert!(dbg.contains("K0ABC"), "Debug should still show callsign; got: {dbg}");
    }

    #[test]
    fn session_intent_default_is_cms() {
        // Every pre-§2.13 caller built ExchangeConfig without an intent
        // field. The Default impl preserves their CMS-dial semantics.
        assert_eq!(SessionIntent::default(), SessionIntent::Cms);
    }

    #[test]
    fn cms_intent_carries_cms_routing_flag() {
        assert_eq!(
            SessionIntent::Cms.routing_flag(),
            Some(RoutingFlag::Cms),
        );
        assert_eq!(RoutingFlag::Cms.as_char(), 'C');
    }

    #[test]
    fn radio_only_intent_carries_r_routing_flag() {
        // R-pool message tagging — WLE B2Protocol.cs:1144 enforces that
        // a `Cms` session refuses to send an `R`-tagged message and
        // vice versa.
        assert_eq!(
            SessionIntent::RadioOnly.routing_flag(),
            Some(RoutingFlag::RadioOnly),
        );
        assert_eq!(RoutingFlag::RadioOnly.as_char(), 'R');
    }

    #[test]
    fn post_office_intent_carries_l_routing_flag() {
        assert_eq!(
            SessionIntent::PostOffice.routing_flag(),
            Some(RoutingFlag::PostOffice),
        );
        assert_eq!(RoutingFlag::PostOffice.as_char(), 'L');
    }

    #[test]
    fn p2p_intent_carries_no_routing_flag() {
        // P2P messages live unpooled in the local mailbox — no `C`/`R`/`L`
        // tag, the mailbox stores them as "direct peer" instead.
        assert_eq!(SessionIntent::P2p.routing_flag(), None);
    }

    #[test]
    fn mesh_intent_carries_no_routing_flag() {
        // Network Post Office / MESH sessions don't carry a flag at the
        // message layer either — the relay's own configuration tags
        // inbound messages downstream.
        assert_eq!(SessionIntent::Mesh.routing_flag(), None);
    }

    #[test]
    fn routing_flag_round_trips_via_char() {
        for f in [RoutingFlag::Cms, RoutingFlag::RadioOnly, RoutingFlag::PostOffice] {
            assert_eq!(RoutingFlag::from_char(f.as_char()), Some(f));
        }
    }

    #[test]
    fn routing_flag_from_char_rejects_unknown_and_lowercase() {
        // WLE's parser is case-sensitive (`B2Protocol.cs:1144-1149`).
        // Lowercase MUST NOT round-trip to a known flag.
        assert_eq!(RoutingFlag::from_char('c'), None);
        assert_eq!(RoutingFlag::from_char('r'), None);
        assert_eq!(RoutingFlag::from_char('l'), None);
        assert_eq!(RoutingFlag::from_char('X'), None);
        assert_eq!(RoutingFlag::from_char(' '), None);
    }

    // ============================================================================
    // SessionIntent serde + auto_arms_listener (tuxlink-0ye6 — Phase 3, Task 3.1)
    // ============================================================================

    #[test]
    fn session_intent_serializes_kebab_case() {
        use serde_json;
        assert_eq!(serde_json::to_string(&SessionIntent::Cms).unwrap(),         "\"cms\"");
        assert_eq!(serde_json::to_string(&SessionIntent::P2p).unwrap(),         "\"p2p\"");
        assert_eq!(serde_json::to_string(&SessionIntent::RadioOnly).unwrap(),   "\"radio-only\"");
        assert_eq!(serde_json::to_string(&SessionIntent::PostOffice).unwrap(),  "\"post-office\"");
        assert_eq!(serde_json::to_string(&SessionIntent::Mesh).unwrap(),        "\"mesh\"");
    }

    #[test]
    fn session_intent_deserializes_kebab_case() {
        use serde_json;
        let cms: SessionIntent = serde_json::from_str("\"cms\"").unwrap();
        let p2p: SessionIntent = serde_json::from_str("\"p2p\"").unwrap();
        let ro:  SessionIntent = serde_json::from_str("\"radio-only\"").unwrap();
        let po:  SessionIntent = serde_json::from_str("\"post-office\"").unwrap();
        let me:  SessionIntent = serde_json::from_str("\"mesh\"").unwrap();
        assert_eq!(cms, SessionIntent::Cms);
        assert_eq!(p2p, SessionIntent::P2p);
        assert_eq!(ro,  SessionIntent::RadioOnly);
        assert_eq!(po,  SessionIntent::PostOffice);
        assert_eq!(me,  SessionIntent::Mesh);
    }

    #[test]
    fn auto_arms_listener_matches_spec_matrix() {
        // Per spec §3 capability matrix — only intents that accept inbound auto-arm.
        assert!(!SessionIntent::Cms.auto_arms_listener());
        assert!( SessionIntent::P2p.auto_arms_listener());
        assert!( SessionIntent::RadioOnly.auto_arms_listener());
        // PostOffice + Mesh are out of alpha scope; their auto-arm behavior is
        // defined-but-unused. Codify the current intent so a future change is
        // a deliberate decision, not a silent flip.
        assert!(!SessionIntent::PostOffice.auto_arms_listener());
        assert!(!SessionIntent::Mesh.auto_arms_listener());
    }

    fn outbound_message(mid: &str, subject: &str, body: &[u8]) -> (OutboundMessage, Vec<u8>) {
        let mut msg = Message::new();
        msg.set_header("Mid", mid);
        msg.set_header("Subject", subject);
        msg.set_body(body.to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();
        (
            OutboundMessage {
                proposal,
                title: subject.to_string(),
                compressed: compressed.clone(),
            },
            compressed,
        )
    }

    #[test]
    fn with_nothing_to_send_we_signal_no_more_messages() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        let mut writer = Vec::<u8>::new();
        let outcome = send_turn(&mut reader, &mut writer, &[], false, None).unwrap();
        assert_eq!(writer, b"FF\r");
        assert!(!outcome.quit_sent);
    }

    #[test]
    fn with_nothing_to_send_and_the_other_side_done_we_quit() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        let mut writer = Vec::<u8>::new();
        let outcome = send_turn(&mut reader, &mut writer, &[], true, None).unwrap();
        assert_eq!(writer, b"FQ\r");
        assert!(outcome.quit_sent);
    }

    #[test]
    fn an_accepted_proposal_is_offered_then_its_body_is_sent() {
        let (out, compressed) = outbound_message("OUTBOUND0001", "Test", b"hello");
        let proposal = out.proposal.clone();

        let mut reader = Cursor::new(b"FS Y\r".to_vec());
        let mut writer = Vec::new();
        let outcome = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false, None).unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(proposal.line().as_bytes());
        expected.push(b'\r');
        expected.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        expected.push(b'\r');
        expected.extend_from_slice(&transfer::frame_block("Test", 0, &compressed));

        assert_eq!(writer, expected);
        assert_eq!(outcome.sent, vec!["OUTBOUND0001".to_string()]);
        assert!(outcome.rejected.is_empty() && outcome.deferred.is_empty());
    }

    #[test]
    fn a_rejected_proposal_sends_no_body() {
        let (out, _compressed) = outbound_message("OUTBOUND0002", "Test", b"hello");
        let proposal = out.proposal.clone();

        let mut reader = Cursor::new(b"FS R\r".to_vec());
        let mut writer = Vec::new();
        let outcome = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false, None).unwrap();

        // Only the proposal line and the checksum line — no framed block.
        let mut expected = Vec::new();
        expected.extend_from_slice(proposal.line().as_bytes());
        expected.push(b'\r');
        expected.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        expected.push(b'\r');

        assert_eq!(writer, expected);
        assert_eq!(outcome.rejected, vec!["OUTBOUND0002".to_string()]);
        assert!(outcome.sent.is_empty());
    }

    #[test]
    fn an_offered_message_we_accept_is_received_and_parsed() {
        let mut msg = Message::new();
        msg.set_header("Mid", "INBOUND00001");
        msg.set_header("Subject", "Field report");
        msg.set_header("From", "N7XYZ");
        msg.set_body(b"Net is active.\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();

        let mut script = Vec::new();
        script.extend_from_slice(proposal.line().as_bytes());
        script.push(b'\r');
        script.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        script.push(b'\r');
        script.extend_from_slice(&transfer::frame_block("Field report", 0, &compressed));

        let mut reader = Cursor::new(script);
        let mut writer = Vec::new();
        let outcome =
            receive_turn(&mut reader, &mut writer, |_| vec![Answer::Accept { resume_offset: 0 }])
                .unwrap();

        assert_eq!(writer, b"FS +\r");
        assert_eq!(outcome.messages.len(), 1);
        assert_eq!(outcome.messages[0].header("Mid"), Some("INBOUND00001"));
        assert_eq!(outcome.messages[0].body(), b"Net is active.\r\n");
        assert!(!outcome.remote_quit && !outcome.remote_no_messages);
    }

    #[test]
    fn the_other_side_having_no_messages_ends_the_turn() {
        let mut reader = Cursor::new(b"FF\r".to_vec());
        let mut writer = Vec::new();
        let outcome = receive_turn(&mut reader, &mut writer, |_| vec![]).unwrap();
        assert!(outcome.remote_no_messages);
        assert!(outcome.messages.is_empty());
        assert!(writer.is_empty());
    }

    #[test]
    fn the_other_side_quitting_is_reported() {
        let mut reader = Cursor::new(b"FQ\r".to_vec());
        let mut writer = Vec::new();
        let outcome = receive_turn(&mut reader, &mut writer, |_| vec![]).unwrap();
        assert!(outcome.remote_quit);
    }

    #[test]
    fn an_empty_proposal_batch_means_the_other_side_has_no_messages() {
        // No proposals, just the end-of-batch line; its checksum is "00".
        let mut reader = Cursor::new(b"F> 00\r".to_vec());
        let mut writer = Vec::new();
        let outcome = receive_turn(&mut reader, &mut writer, |_| vec![]).unwrap();
        assert!(outcome.remote_no_messages);
        assert!(outcome.messages.is_empty());
        assert!(writer.is_empty());
    }

    #[test]
    fn dial_role_preserves_server_speaks_first_behaviour() {
        // Identical to a_session_with_no_traffic_handshakes_then_quits, but via the
        // role-parameterized entry point. Dial = today's slave behaviour.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let mut reader = Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("MYPASS".into()),
            intent: SessionIntent::Cms,
        };
        let result = run_exchange_with_role(
            &mut reader,
            &mut writer,
            ExchangeRole::Dial,
            &config,
            vec![],
            |_| vec![],
            None,
        )
        .unwrap();
        assert!(result.received.is_empty() && result.sent.is_empty());

        let token = crate::winlink::secure::secure_login_response("12345678", "MYPASS");
        let mut expected =
            crate::winlink::handshake::build_handshake("N7CPZ", "SERVICE", "CN87", Some(&token));
        expected.extend_from_slice(b"FF\r");
        expected.extend_from_slice(b"FQ\r");
        assert_eq!(writer, expected);
    }

    #[test]
    fn a_session_with_no_traffic_handshakes_then_quits() {
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
        server.extend_from_slice(b"FF\r"); // the server's one turn: no messages
        let mut reader = Cursor::new(server);
        let mut writer = Vec::new();

        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("MYPASS".into()),
            intent: SessionIntent::Cms,
        };
        let result = run_exchange(&mut reader, &mut writer, &config, vec![], |_| vec![], None).unwrap();

        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());

        // We answer the challenge, then signal no-more (FF), then quit (FQ).
        let token = crate::winlink::secure::secure_login_response("12345678", "MYPASS");
        let mut expected =
            crate::winlink::handshake::build_handshake("N7CPZ", "SERVICE", "CN87", Some(&token));
        expected.extend_from_slice(b"FF\r");
        expected.extend_from_slice(b"FQ\r");
        assert_eq!(writer, expected);
    }

    #[test]
    fn a_session_receives_an_offered_message() {
        let mut msg = Message::new();
        msg.set_header("Mid", "SRVMSG000001");
        msg.set_header("Subject", "Weather");
        msg.set_body(b"Wind calm.\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();

        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r"); // no challenge
        server.extend_from_slice(proposal.line().as_bytes());
        server.push(b'\r');
        server.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        server.push(b'\r');
        server.extend_from_slice(&transfer::frame_block("Weather", 0, &compressed));
        server.extend_from_slice(b"FF\r"); // the server's next turn: no more

        let mut reader = Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::Cms,
        };
        let result = run_exchange(&mut reader, &mut writer, &config, vec![], |_| {
            vec![Answer::Accept { resume_offset: 0 }]
        }, None)
        .unwrap();

        assert_eq!(result.received.len(), 1);
        assert_eq!(result.received[0].header("Mid"), Some("SRVMSG000001"));
        assert_eq!(result.received[0].body(), b"Wind calm.\r\n");
    }

    #[test]
    fn a_challenge_with_no_password_is_an_error() {
        let mut reader = Cursor::new(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r".to_vec());
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::Cms,
        };
        assert_eq!(
            run_exchange(&mut reader, &mut writer, &config, vec![], |_| vec![], None),
            Err(ExchangeError::PasswordRequired)
        );
    }

    #[test]
    fn a_remote_error_line_is_surfaced_while_receiving() {
        // The CMS rejects with a "*** ..." line (seen live: unknown client type).
        let mut reader = Cursor::new(
            b"*** Unknown client types are not allowed on production servers - Disconnecting\r"
                .to_vec(),
        );
        let mut writer = Vec::new();
        let result = receive_turn(&mut reader, &mut writer, |_| vec![]);
        assert!(matches!(result, Err(ExchangeError::RemoteError(_))));
    }

    #[test]
    fn a_remote_error_line_is_surfaced_while_sending() {
        let (out, _) = outbound_message("ERR000000001", "Test", b"hi");
        let mut reader = Cursor::new(b"*** Secure login failed\r".to_vec());
        let mut writer = Vec::new();
        let result = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false, None);
        assert!(matches!(result, Err(ExchangeError::RemoteError(_))));
    }

    #[test]
    fn a_corrupt_proposal_batch_is_caught_by_the_checksum() {
        let (out, _) = outbound_message("CHECKSUM0001", "Test", b"hello");
        let proposal = out.proposal.clone();
        let mut script = Vec::new();
        script.extend_from_slice(proposal.line().as_bytes());
        script.push(b'\r');
        script.extend_from_slice(b"F> 00\r"); // wrong checksum for a non-empty batch

        let mut reader = Cursor::new(script);
        let mut writer = Vec::new();
        assert_eq!(
            receive_turn(&mut reader, &mut writer, |_| vec![Answer::Accept { resume_offset: 0 }]),
            Err(ExchangeError::ChecksumMismatch)
        );
    }

    #[test]
    fn answer_role_sends_handshake_first_then_remote_takes_first_turn() {
        // We are master. The scripted peer is slave: WE speak the handshake first; it
        // replies with its own handshake which — like a real dialing station — carries
        // NO `>` prompt and ends with its `DE` line. The master detects the end of the
        // slave handshake by the start of its message turn (the `FC` proposal line),
        // exactly as wl2k-go does (tuxlink-3wh).
        let mut peer = Vec::new();
        // The peer's (slave) handshake reply: forwarding line, identifier, DE line — no prompt.
        peer.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\r; N7CPZ DE W7AUX (CN87)\r");
        // The peer (slave) takes the first message turn: one offered message.
        let mut msg = Message::new();
        msg.set_header("Mid", "PEERMSG00001");
        msg.set_header("Subject", "Hi");
        msg.set_body(b"Direct peer message.\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();
        peer.extend_from_slice(proposal.line().as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(&transfer::frame_block("Hi", 0, &compressed));
        // After our accept + our (empty) turn, the peer is done.
        peer.extend_from_slice(b"FQ\r");

        let mut reader = Cursor::new(peer);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(), // base call — NO ssid in the B2F identity
            targetcall: "W7AUX".into(),
            locator: "CN87".into(),
            password: None, // peers never challenge; no secret in P2P
            intent: SessionIntent::P2p,
        };
        let result = run_exchange_with_role(
            &mut reader,
            &mut writer,
            ExchangeRole::Answer,
            &config,
            vec![],
            |_| vec![Answer::Accept { resume_offset: 0 }],
            None,
        )
        .unwrap();

        // We received the peer's message.
        assert_eq!(result.received.len(), 1);
        assert_eq!(result.received[0].header("Mid"), Some("PEERMSG00001"));
        assert_eq!(result.received[0].body(), b"Direct peer message.\r\n");

        // We spoke the MASTER handshake FIRST (SID + `>` prompt; no `;PQ`/`;PR` in P2P),
        // then accepted (`FS +`), then on our turn signalled no-more (FF) → quit (FQ).
        let our_handshake =
            crate::winlink::handshake::build_master_handshake("N7CPZ", "W7AUX", "CN87");
        assert!(
            writer.starts_with(&our_handshake),
            "master must send its master handshake (with `>` prompt) before anything else; wrote {:?}",
            String::from_utf8_lossy(&writer)
        );
        // After the handshake, we accept the peer's batch (`FS +\r`), then on our
        // turn we have nothing to send and the remote is not yet signalled done →
        // `FF\r`. The peer then sends `FQ\r` (inbound), which breaks the loop before
        // we write anything more — so our writes after the handshake are just
        // `FS +\r` then `FF\r`.
        let tail = &writer[our_handshake.len()..];
        assert_eq!(tail, b"FS +\rFF\r");
    }

    // ============================================================================
    // run_exchange_with_events — auth-only contract + Mode 5 discriminator
    // (tuxlink-7do4 smart auth diagnostics §6.3 / §6.4)
    // ============================================================================

    #[test]
    fn run_exchange_with_events_emits_handshake_events_to_sink() {
        use super::super::b2f_events::{AttemptId, B2fEvent, VecEventSink};
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 23753528\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("FOOBAR".into()),
            intent: SessionIntent::Cms,
        };
        let sink = VecEventSink::new();
        let result = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink),
            AttemptId::fresh(),
        ).unwrap();
        assert!(result.received.is_empty());
        let events = sink.snapshot();
        let kinds: Vec<&str> = events.iter().map(|e| match e {
            B2fEvent::RemoteSidReceived { .. } => "remote_sid_received",
            B2fEvent::SecureChallengeReceived { .. } => "secure_challenge_received",
            B2fEvent::SecureResponseSent { .. } => "secure_response_sent",
            B2fEvent::PostAuthExchangeStarted { .. } => "post_auth_exchange_started",
            B2fEvent::ConnectionClosed { .. } => "connection_closed",
            _ => "other",
        }).collect();
        assert!(kinds.contains(&"remote_sid_received"), "events: {kinds:?}");
        assert!(kinds.contains(&"secure_challenge_received"), "events: {kinds:?}");
        assert!(kinds.contains(&"secure_response_sent"), "events: {kinds:?}");
        assert!(kinds.contains(&"post_auth_exchange_started"),
            "Mode 5 discriminator must fire on FF receipt — events: {kinds:?}");
    }

    #[test]
    fn run_exchange_with_events_mode3_emits_remote_error_no_post_auth() {
        use super::super::b2f_events::{AttemptId, B2fEvent, VecEventSink};
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 23753528\rCMS>\r");
        // After we send our ;PR, server rejects with *** then closes.
        server.extend_from_slice(b"*** [1] Secure login failed - account password does not match\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("WRONGPW".into()),
            intent: SessionIntent::Cms,
        };
        let sink = VecEventSink::new();
        let _ = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink),
            AttemptId::fresh(),
        );
        let events = sink.snapshot();
        // RemoteErrorReceived must fire; PostAuthExchangeStarted MUST NOT fire.
        assert!(events.iter().any(|e| matches!(e, B2fEvent::RemoteErrorReceived { .. })),
            "events: {events:?}");
        assert!(!events.iter().any(|e| matches!(e, B2fEvent::PostAuthExchangeStarted { .. })),
            "Mode 3 wrongly emitted Mode 5 discriminator — events: {events:?}");
    }

    /// cms-z happy-path integration smoke — spec §8.4.
    ///
    /// Asserts the three invariants required by the tuxlink-7do4 smart auth
    /// diagnostics spec §8.4 on a successful (Mode 5) connect:
    ///
    /// 1. `PostAuthExchangeStarted` IS emitted — the Mode 5 discriminator
    ///    fired when the first non-`***` `F`-prefixed byte arrived.
    /// 2. `RemoteErrorReceived` is NOT emitted — no `***` line was received
    ///    (the server accepted our credentials without a rejection line).
    /// 3. `AuthClassified` is NOT emitted — `AuthClassified` is emitted at
    ///    the command layer (`ui_commands.rs`), not by the inner session
    ///    function. `run_exchange_with_events` must be clean of it on the
    ///    happy path; its absence here proves the event-layer boundary
    ///    between session and command is intact.
    ///
    /// The scripted server emits the real cms-z.winlink.org happy-path
    /// sequence (SID + `;PQ` challenge + `CMS>` prompt, then `FF` on the
    /// first message turn), so this test exercises the full code path from
    /// handshake through to the auth-only `FF + FQ` quit.
    #[test]
    fn cms_z_happy_path_smoke_post_auth_started_no_error_no_classified() {
        use super::super::b2f_events::{AttemptId, B2fEvent, VecEventSink};

        // Scripted in-memory CMS: SID with B2FHM, ;PQ challenge, CMS> prompt,
        // then FF (no inbound messages — "no traffic" happy path).
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 87654321\rCMS>\r");
        server.extend_from_slice(b"FF\r");

        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("GOODPASS".into()),
            intent: SessionIntent::Cms,
        };

        let sink = VecEventSink::new();
        let result = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink),
            AttemptId::fresh(),
        );
        assert!(result.is_ok(), "happy-path exchange must not error: {result:?}");

        let events = sink.snapshot();

        // Invariant 1 (spec §8.4): PostAuthExchangeStarted MUST fire — proves
        // the Mode 5 discriminator activated on the `FF` byte.
        assert!(
            events.iter().any(|e| matches!(e, B2fEvent::PostAuthExchangeStarted { .. })),
            "spec §8.4: PostAuthExchangeStarted must fire on happy-path connect — events: {events:?}",
        );

        // Invariant 2 (spec §8.4): RemoteErrorReceived MUST NOT fire — no
        // `***` line from the server means the credentials were accepted.
        assert!(
            !events.iter().any(|e| matches!(e, B2fEvent::RemoteErrorReceived { .. })),
            "spec §8.4: RemoteErrorReceived must NOT fire on happy path — events: {events:?}",
        );

        // Invariant 3 (spec §8.4): AuthClassified MUST NOT fire — that event
        // is emitted at the command layer (ui_commands.rs), never inside
        // run_exchange_with_events. Its presence here would indicate a layering
        // violation.
        assert!(
            !events.iter().any(|e| matches!(e, B2fEvent::AuthClassified { .. })),
            "spec §8.4: AuthClassified must NOT fire inside run_exchange_with_events — \
             it belongs to the command layer only — events: {events:?}",
        );
    }

    // ---- Codex MAJOR #2: caller-supplied AttemptId threading ----

    #[test]
    fn run_exchange_with_events_uses_caller_supplied_attempt_id() {
        use super::super::b2f_events::{AttemptId, B2fEvent, VecEventSink};
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 23753528\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: Some("FOOBAR".into()),
            intent: SessionIntent::Cms,
        };
        let sink = VecEventSink::new();
        let supplied_id = AttemptId(99);
        let _ = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink), supplied_id,
        );
        let events = sink.snapshot();
        for event in &events {
            let id = match event {
                B2fEvent::RemoteSidReceived { attempt_id, .. } => attempt_id,
                B2fEvent::SecureChallengeReceived { attempt_id } => attempt_id,
                B2fEvent::SecureResponseSent { attempt_id } => attempt_id,
                B2fEvent::PostAuthExchangeStarted { attempt_id } => attempt_id,
                B2fEvent::ConnectionClosed { attempt_id, .. } => attempt_id,
                _ => continue,
            };
            assert_eq!(*id, supplied_id, "event {event:?} has wrong attempt_id");
        }
    }

    // ---- Codex MAJOR #3: reject malformed F-prefix lines ----

    #[test]
    fn run_exchange_with_events_rejects_malformed_f_line() {
        use super::super::b2f_events::{AttemptId, B2fEvent, VecEventSink};
        // Server sends a malformed F-prefix line that isn't a valid B2F command.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        server.extend_from_slice(b"FLOL malformed\r");
        let mut reader = std::io::Cursor::new(server);
        let mut writer = Vec::new();
        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::Cms,
        };
        let sink = VecEventSink::new();
        let result = run_exchange_with_events(
            &mut reader, &mut writer, &config, vec![], |_| vec![], None, Some(&sink), AttemptId(1),
        );
        // PostAuthExchangeStarted must NOT fire — the line is not a valid B2F command.
        let events = sink.snapshot();
        assert!(
            !events.iter().any(|e| matches!(e, B2fEvent::PostAuthExchangeStarted { .. })),
            "PostAuthExchangeStarted wrongly fired for malformed F-line — events: {events:?}",
        );
        // The function should return Err(UnexpectedResponse).
        assert!(
            matches!(result, Err(ExchangeError::UnexpectedResponse(_))),
            "expected UnexpectedResponse, got: {result:?}",
        );
    }
}
