//! Backend-abstraction trait for tuxlink's Winlink interactions.
//!
//! Spec: docs/superpowers/specs/2026-05-18-winlink-backend-trait-design.md
//! bd issue: tuxlink-z5f
//!
//! This module defines the `WinlinkBackend` trait — the architectural
//! boundary that decouples tuxlink's UI/config layer from any one Winlink
//! protocol implementation. One implementation lives here:
//!
//! - [`NativeBackend`] — speaks B2F directly, stores messages in its own
//!   mailbox, and connects over plaintext or TLS telnet.
//!
//! Per [feedback_discipline_triage_rule]: the trait is the hard-to-undo
//! architectural decision; once defined, implementations are TDD plumbing.

use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

/// Mailbox folder selector. `#[non_exhaustive]` per tuxlink-z5f v2 P1 #5 —
/// future folders (Drafts, Spam, custom) added without breaking exhaustive
/// matches at call sites. `Copy + Clone + Debug` so the trait re-export
/// carries useful semantics.
///
/// Canonical path: `winlink_backend::MailboxFolder` (moved from the deleted
/// `pat_client` module in tuxlink-9phd Phase 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MailboxFolder {
    Inbox,
    Sent,
    Outbox,
    Archive,
    Deleted,
}

impl MailboxFolder {
    #[allow(dead_code)]
    pub(crate) fn as_path(&self) -> &'static str {
        match self {
            MailboxFolder::Inbox => "in",
            MailboxFolder::Sent => "sent",
            MailboxFolder::Outbox => "out",
            MailboxFolder::Archive => "archive",
            MailboxFolder::Deleted => "deleted",
        }
    }
}

#[cfg(test)]
mod mailbox_folder_tests {
    use super::*;

    #[test]
    fn deleted_folder_maps_to_deleted_path() {
        assert_eq!(MailboxFolder::Deleted.as_path(), "deleted");
    }
}

// Native backend wiring (see the NativeBackend section below).
use crate::config::{broadcast_grid, CmsTransport, Config};
use crate::native_mailbox::Mailbox;
use crate::winlink::ax25::{Address, KissLinkConfig};
use crate::winlink::message::{Message, RECEIVED_SESSION_HEADER, RECEIVED_SESSION_POST_OFFICE};
use crate::winlink::proposal::Answer;
use crate::winlink::session::{ExchangeRole, SessionIntent};
use crate::winlink::{compose, session, telnet};
use std::path::PathBuf;

// ============================================================================
// Supporting types (spec §3.2)
// ============================================================================

/// Newtype around the Winlink Message ID (MID) string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(s: impl Into<String>) -> Self {
        MessageId(s.into())
    }
}

/// Light header-only view returned by `list_messages`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageMeta {
    pub id: MessageId,
    pub subject: String,
    pub from: String,
    /// Recipient list. Drives the list "To" column (esp. Sent/Outbox).
    /// Recipient list. Added by Task 12 (tuxlink-zsm). NativeBackend populates
    /// this from the stored RFC5322 headers; spec §2.1 graceful degradation
    /// for backends that don't expose a recipient list.
    pub to: Vec<String>,
    /// RFC 3339 UTC timestamp. Backend emits canonical form.
    pub date: String,
    pub unread: bool,
    pub body_size: u64,
    /// Attachment-presence indicator for the list `#` column. Added by Task
    /// 12 (tuxlink-zsm). The full attachment list is materialized at read time
    /// (Task 13's RFC5322 parse), not in the list view.
    pub has_attachments: bool,
    /// The identity this message belongs to (Phase 7, tuxlink-noa0): the
    /// per-FULL namespace for received mail, or the sent/queued-as identity for
    /// the shared Sent/Outbox (read from the Phase-4 `<mid>.identity` sidecar in
    /// the listing loop). `None` = untagged (legacy / pre-Phase-4) → the mailbox
    /// identity filter treats it as matching only the "All identities" option.
    pub identity: Option<String>,
}

/// Full body returned by `read_message`. Byte fidelity per spec §3.2 v2
/// P0 #2 — Winlink B2F messages can carry binary MIME parts; UTF-8
/// conversion happens at the display boundary (Tauri command), not here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MessageBody {
    pub id: MessageId,
    pub raw_rfc5322: Vec<u8>,
}

/// Attachment carried in an outbound message. Spec §6.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAttachment {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// Outbound message — what `send_message` consumes. Intentionally NOT
/// `#[non_exhaustive]` (per spec §3.2) to keep caller-construction
/// ergonomic. Adding fields is an acknowledged breaking change.
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// RFC 3339 UTC timestamp. Caller provides; backend validates.
    pub date: String,
    pub attachments: Vec<OutboundAttachment>,
}

/// Transport selector for `connect`. `#[non_exhaustive]` so v0.5+ can add
/// Packet/Pactor/VARA HF/VARA FM/AX.25/KISS variants without breaking
/// existing match arms.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TransportConfig {
    /// CMS Telnet (plain or TLS), per existing `config::CmsTransport`.
    Cms { mode: crate::config::CmsTransport },
    /// AX.25 1200-baud packet over a KISS link (TCP / serial). The SSID rides
    /// the AX.25 *link* address; the B2F identity uses the base call (spec §4.4).
    Packet {
        link: KissLinkConfig,
        ssid: u8,
        role: PacketRole,
        /// [R4-3][R1-C15][R5-3] Which message pool this packet session
        /// belongs to. A dial defaults to `Cms` (existing callers); an
        /// armed Listen is always `P2p` (an inbound packet call is by
        /// definition a peer session — this station is not an RMS).
        intent: SessionIntent,
    },
}

/// What a packet connection does. `DialTo` is the operator pressing "Connect to"
/// (gateway OR peer — tuxlink reacts to the challenge, not a mode flag); `Listen`
/// is the idle armed-to-answer state (spec §2, §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketRole {
    DialTo { call: String, path: Vec<String> },
    Listen,
}

/// What a `PacketRole` + identity resolves into for the lifecycle: the SSID'd
/// link address, the base B2F call, the exchange role, and (for a dial) the
/// target + digipeater addresses. Mirrors `resolve_cms_endpoint`'s "config →
/// concrete endpoint" job for the packet transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPacket {
    pub link_mycall: Address,
    pub base_mycall: String,
    pub role: ExchangeRole,
    /// `Some((target, digis))` for a dial; `None` for listen.
    pub dial: Option<(Address, Vec<Address>)>,
    /// [R4-3][R1-C15][R5-3] Carried through from the `TransportConfig::Packet`
    /// the operator (or `packet_listen_transport_from_config`) built; travels
    /// alongside `role` so `native_packet_connect` can build an intent-aware
    /// `PacketConnectCtx` without re-deriving it.
    pub intent: SessionIntent,
}

/// Parse a `CALL` or `CALL-SSID` string into an [`Address`]. A bare call has
/// SSID 0. Rejects an SSID outside 0–15 or a malformed token.
fn parse_call_ssid(s: &str) -> Result<Address, BackendError> {
    let (call, ssid) = match s.rsplit_once('-') {
        Some((c, s_part)) => {
            let n: u8 = s_part
                .parse()
                .map_err(|_| BackendError::NotConfigured(format!("bad SSID in '{s_part}'")))?;
            (c, n)
        }
        None => (s, 0),
    };
    if ssid > 15 || call.is_empty() {
        return Err(BackendError::NotConfigured(format!("bad call/ssid '{s}'")));
    }
    Ok(Address {
        call: call.to_uppercase(),
        ssid,
    })
}

/// Resolve identity + role into the concrete addresses + exchange role. Enforces
/// the 0–2 digipeater cap (spec §1) and the identity split (spec §4.4).
///
/// `intent` [R4-3][R1-C15][R5-3] rides straight through to [`ResolvedPacket::intent`]
/// unexamined — this function resolves *addressing*, not message-pool policy; the
/// caller (`packet_connect_inner`) is the one that knows whether this is a dial
/// (operator-selected intent, default `Cms`) or a Listen answer (always `P2p`).
pub fn resolve_packet_endpoint(
    base_mycall: &str,
    ssid: u8,
    role: PacketRole,
    intent: SessionIntent,
) -> Result<ResolvedPacket, BackendError> {
    let base = base_mycall.trim().to_uppercase();
    let link_mycall = Address {
        call: base.clone(),
        ssid,
    };
    match role {
        PacketRole::Listen => Ok(ResolvedPacket {
            link_mycall,
            base_mycall: base,
            role: ExchangeRole::Answer,
            dial: None,
            intent,
        }),
        PacketRole::DialTo { call, path } => {
            if path.len() > 2 {
                return Err(BackendError::NotConfigured(format!(
                    "at most 2 digipeaters allowed (got {})",
                    path.len()
                )));
            }
            // FIX-3 [P3]: validate AX.25 address grammar on the target AND every
            // via hop BEFORE any Address is built — a malformed peer-derived hop
            // (lowercase, >6 base, SSID > 15, control char) must never reach
            // `Address::encode` / the RF address field. `parse_call_ssid` alone
            // only checks SSID range + non-empty; it would pass a bad base.
            crate::winlink::callsign::validate_ax25_hop(&call)
                .map_err(|e| BackendError::NotConfigured(format!("bad packet target: {e}")))?;
            for hop in &path {
                crate::winlink::callsign::validate_ax25_hop(hop).map_err(|e| {
                    BackendError::NotConfigured(format!("bad digipeater hop {hop:?}: {e}"))
                })?;
            }
            let target = parse_call_ssid(&call)?;
            let digis = path
                .iter()
                .map(|p| parse_call_ssid(p))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ResolvedPacket {
                link_mycall,
                base_mycall: base,
                role: ExchangeRole::Dial,
                dial: Some((target, digis)),
                intent,
            })
        }
    }
}

/// Best-effort reachability probe for the Phase 5 gate's `online` flag: a short
/// TCP connect to the API host. `false` on any error/timeout (fail-closed — an
/// unreachable API means offline, so an uncached tactical is refused, never allowed).
async fn host_reachable(host_port: &str) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(host_port),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Build the per-message proposals + compressed bodies for a B2F exchange
/// from a Mailbox's Outbox folder. Skips messages whose bytes fail to parse
/// or whose body cannot be turned into a proposal — mirroring the inline
/// pattern in `native_telnet_exchange` / `native_packet_exchange` /
/// `run_ardop_b2f_exchange` (winlink_backend.rs).
///
/// Pulled out so paths that bypass `NativeBackend::connect` (in particular
/// `ui_commands::telnet_p2p_connect` for tuxlink-l55l) build the same shape
/// of outbound without duplicating the loop.
///
/// # Drain gate + send-time MID selection (tuxlink-6c9y §5.5)
///
/// Spec §3 capability matrix requires every B2F session to drain only the
/// subset of Outbox appropriate to the session's
/// [`SessionIntent::routing_flag`]:
///
/// - `Cms` (flag `C`)        → CMS / Post Office mail pool.
/// - `RadioOnly` (flag `R`)  → radio-only mail pool.
/// - `PostOffice` (flag `L`) → local Post Office mail pool.
/// - `Mesh` (flag `C`)       → normal mail pool over a relay/mesh transport.
/// - `P2p` (no flag)          → unflagged peer-to-peer mail.
///
/// **Narrowed safety gate.** The on-disk [`MessageMeta`] schema still carries
/// no per-message `routing_flag` field, so a full per-message filter is not
/// yet possible. In its place this helper fail-closes for the two intents
/// whose unfiltered drain would tag spec-mismatched routing flags at the peer
/// — `P2p` and `RadioOnly` — returning [`BackendError::MessageRejected`] with
/// a diagnostic naming the gate + the bd issue tracking the residual schema
/// work (**tuxlink-u5hl**, which re-scopes those two). The Post Office modes
/// (`Cms`, `PostOffice`, `Mesh`) drain; for `PostOffice`/`Mesh` the caller's
/// explicit `selected` MID set IS the leakage guard, bounding which Outbox
/// drafts ship at send time.
///
/// **`selected` semantics.** `None` drains the whole Outbox (status-quo CMS
/// behavior; the back-compat path for every existing caller). `Some(set)`
/// intersects the live Outbox with `set` on the MID (`meta.id.0 ==
/// proposal.mid`): a draft ships iff its MID is in `set`. The selection is
/// advisory — a selected MID no longer present in the Outbox is silently
/// skipped (never appears in the listing), not fatal.
///
/// All callers (dial AND listen) catch the gate error and degrade to an empty
/// outbound list (Codex Phase 3-4 RE-REVIEW P2). The dial path previously
/// fail-closed via `?` propagation; the degrade-to-empty posture lets the
/// exchange proceed with no outbound proposed — the peer never sees an
/// off-spec routing-flag tag because an empty batch carries no proposals.
///
/// Concrete call sites:
/// - **Dial:** [`run_ardop_b2f_exchange`] / [`run_vara_b2f_exchange`] /
///   [`crate::ui_commands::telnet_p2p_connect`] — `unwrap_or_else` /
///   `match` on `BackendError::MessageRejected` to empty Vec.
/// - **Listen:** [`run_ardop_b2f_answer`] / [`run_vara_b2f_answer`] /
///   [`crate::winlink::telnet_listen`] — same `unwrap_or_else` pattern.
pub fn build_outbound_proposals(
    mailbox: &Mailbox,
    intent: SessionIntent,
    selected: Option<&std::collections::HashSet<String>>,
    active_full: Option<&str>,
) -> Result<Vec<session::OutboundMessage>, BackendError> {
    // Safety gate (narrowed — tuxlink-6c9y §5.5): P2p/RadioOnly still fail-closed
    // (6c9y does not address their leakage; tuxlink-u5hl re-scopes them). Cms/
    // PostOffice/Mesh drain; for the Post Office modes, `selected` IS the leakage
    // guard.
    if matches!(intent, SessionIntent::P2p | SessionIntent::RadioOnly) {
        return Err(BackendError::MessageRejected(format!(
            "safety gate: outbound mail filtering not yet implemented for \
             {intent:?} sessions (tracked as bd issue tuxlink-u5hl)."
        )));
    }
    let mut outbound = Vec::new();
    for meta in mailbox.list(MailboxFolder::Outbox)? {
        // §5.5(b): advisory selection intersected with the live Outbox on the MID
        // (meta.id.0 == proposal.mid). Vanished MID never appears here (skip-not-abort).
        if let Some(sel) = selected {
            if !sel.contains(&meta.id.0) {
                continue;
            }
        }
        // Identity drain gate (tuxlink-2ns7): a session drains only its own
        // queued mail. The shared Outbox holds every identity's drafts; a
        // session connected as `active_full` ships only the messages tagged
        // with that FULL. An untagged (legacy / pre-Phase-4) message has no
        // `<mid>.identity` sidecar and drains for ANY active identity, so the
        // migration never strands a pre-existing draft. `active_full == None`
        // disables the filter entirely (back-compat for callers not yet
        // identity-aware).
        if let Some(active) = active_full {
            if let Some(tag) = mailbox.read_identity_tag(MailboxFolder::Outbox, &meta.id) {
                if tag != active {
                    continue;
                }
            }
        }
        // Codex review 2026-06-03 [P2 #6] (tuxlink-61yg): per-message read
        // failures used to propagate `?` and discard the entire batch.
        // A single bad/missing file would silently withhold ALL readable
        // mail from the session. Now: skip and continue. A folder-wide
        // listing failure still propagates (the outer `?`).
        let body = match mailbox.read(MailboxFolder::Outbox, &meta.id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "build_outbound_proposals: skipping outbox message {:?}: {e}",
                    meta.id
                );
                continue;
            }
        };
        if let Ok(message) = Message::from_bytes(&body.raw_rfc5322) {
            if let Some((proposal, compressed)) = message.to_proposal() {
                let title = message.header("Subject").unwrap_or_default().to_string();
                outbound.push(session::OutboundMessage {
                    proposal,
                    title,
                    compressed,
                });
            }
        }
    }
    Ok(outbound)
}

#[cfg(test)]
mod build_outbound_proposals_tests {
    use super::*;
    use crate::native_mailbox::Mailbox;
    use crate::winlink::compose::compose_message;
    use std::collections::HashSet;
    use tempfile::tempdir;

    // tuxlink-9efs: the DIAL outbound drain degrades ONLY the safety-gate
    // rejection (MessageRejected) to an empty outbound; every other error
    // propagates so a corrupt / unreadable mailbox fail-closes instead of
    // masquerading as a successful empty send. (The OLD `unwrap_or_else`
    // degraded ALL errors — this test fails against that behavior.)
    #[test]
    fn dial_outbound_or_propagate_degrades_only_message_rejected() {
        // Safety-gate rejection -> degrade to empty (the intended skip).
        let degraded =
            dial_outbound_or_propagate(Err(BackendError::MessageRejected("gate".into())), "test");
        assert!(
            matches!(&degraded, Ok(v) if v.is_empty()),
            "MessageRejected should degrade to empty outbound; got {degraded:?}"
        );

        // Any OTHER error must propagate (fail-closed), NOT silently empty.
        let propagated = dial_outbound_or_propagate(Err(BackendError::InvalidSession), "test");
        assert!(
            matches!(propagated, Err(BackendError::InvalidSession)),
            "non-MessageRejected errors must propagate, not degrade to empty"
        );

        // Ok passes through unchanged.
        let ok = dial_outbound_or_propagate(Ok(Vec::new()), "test");
        assert!(matches!(ok, Ok(v) if v.is_empty()));
    }

    #[test]
    fn empty_outbox_returns_empty_vec() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, None).unwrap();
        assert!(
            out.is_empty(),
            "empty outbox should produce no proposals; got {out:?}"
        );
    }

    #[test]
    fn queued_drafts_produce_one_proposal_each() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // tuxlink-l55l: two queued outbound drafts addressed to two different
        // recipients — CMS semantics send ALL of them; recipient routing is
        // the gateway's job (CMS-as-Post-Office). Build via the same path the
        // compose flow uses so the bytes are a valid Winlink message.
        //
        // (tuxlink-u5hl: this test originally exercised the P2P path, but the
        // intent-filtered drain gate now refuses non-CMS drains until the
        // routing_flag schema lands. The no-peer-filter invariant is the same
        // for CMS and P2P intents; CMS exercises the unfiltered path today.)
        let m1 = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Drain-test-1",
            "first body",
            1_716_200_000,
        );
        let m2 = compose_message(
            "N7CPZ",
            &["cameronzucker@gmail.com"],
            &[],
            "Drain-test-2",
            "second body",
            1_716_200_001,
        );
        mailbox
            .store(MailboxFolder::Outbox, &m1.to_bytes())
            .unwrap();
        mailbox
            .store(MailboxFolder::Outbox, &m2.to_bytes())
            .unwrap();

        let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, None).unwrap();
        assert_eq!(
            out.len(),
            2,
            "two queued drafts should produce two proposals; got {} ({out:?})",
            out.len()
        );
        let titles: Vec<&str> = out.iter().map(|o| o.title.as_str()).collect();
        assert!(titles.contains(&"Drain-test-1"));
        assert!(titles.contains(&"Drain-test-2"));
    }

    #[test]
    fn no_per_peer_filtering_ships_all_drafts() {
        // The drain helper MUST NOT filter outbox by recipient/peer
        // callsign at dial-time. The peer (CMS gateway, WLE relay, or
        // RMS Relay post-office, depending on intent) acts as the
        // post-office and routes via its own CMS uplink. This test pins the
        // contract: queue a draft addressed to a third party, dial intent X,
        // and the draft must still be offered (so long as the intent passes
        // the safety gate — CMS today).
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        let third_party = compose_message(
            "N7CPZ",
            &["unrelated@example.org"],
            &[],
            "Routed-through-peer",
            "body",
            1_716_200_002,
        );
        mailbox
            .store(MailboxFolder::Outbox, &third_party.to_bytes())
            .unwrap();

        let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, None).unwrap();
        assert_eq!(
            out.len(),
            1,
            "drafts addressed to a third party MUST still be offered to the peer; got {out:?}"
        );
        assert_eq!(out[0].title, "Routed-through-peer");
    }

    // ────────────────────────────────────────────────────────────────────
    // tuxlink-u5hl — Codex Round 5 P1 #3: intent-filtered drain safety gate.
    // The full per-message routing-flag filter requires a MessageMeta schema
    // change; until that lands, non-CMS intents must fail-closed at the
    // drain helper rather than offer every Outbox message regardless of
    // intent (which would tag spec-mismatched routing flags at the peer).
    // ────────────────────────────────────────────────────────────────────

    /// Helper: queue one valid outbox message so `build_outbound_proposals`
    /// would have something to drain absent the safety gate. The gate must
    /// fire BEFORE iterating the outbox, so the presence of a draft proves
    /// the gate is intent-driven, not "no mail anyway."
    fn outbox_with_one_draft() -> (tempfile::TempDir, Mailbox) {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let m = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Safety-gate fixture",
            "body",
            1_716_200_000,
        );
        mailbox.store(MailboxFolder::Outbox, &m.to_bytes()).unwrap();
        (dir, mailbox)
    }

    #[test]
    fn safety_gate_fires_for_p2p_intent() {
        let (_dir, mailbox) = outbox_with_one_draft();
        let err = build_outbound_proposals(&mailbox, SessionIntent::P2p, None, None)
            .expect_err("safety gate must reject P2p drain — see tuxlink-u5hl");
        assert!(
            matches!(err, BackendError::MessageRejected(_)),
            "expected MessageRejected (safety gate); got {err:?}"
        );
        let msg = format!("{err}");
        assert!(
            msg.contains("safety gate"),
            "error must self-identify as safety gate; got: {msg}"
        );
        assert!(
            msg.contains("tuxlink-u5hl"),
            "error must reference the tracking bd issue; got: {msg}"
        );
    }

    #[test]
    fn safety_gate_fires_for_radio_only_intent() {
        let (_dir, mailbox) = outbox_with_one_draft();
        let err = build_outbound_proposals(&mailbox, SessionIntent::RadioOnly, None, None)
            .expect_err("safety gate must reject RadioOnly drain — see tuxlink-u5hl");
        assert!(
            matches!(err, BackendError::MessageRejected(_)),
            "expected MessageRejected (safety gate); got {err:?}"
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // tuxlink-6c9y §5.5 — send-time MID selection for the Post Office modes.
    // PostOffice/Mesh no longer fail-closed at the gate (their routing flags
    // are valid CMS-pool mail); instead the caller passes an explicit
    // `selected` MID set that bounds which Outbox drafts ship. The selection
    // is advisory: it is intersected with the live Outbox on the MID
    // (meta.id.0 == proposal.mid), and a selected MID no longer present in the
    // Outbox is silently skipped (not fatal). These tests pin that contract.
    //
    // MIDs are generated by `compose_message` (not settable), so each fixture
    // reads the ACTUAL generated MIDs back from the Outbox listing and builds
    // the selection from a chosen subset of those real MIDs.
    // ────────────────────────────────────────────────────────────────────

    #[test]
    fn post_office_intent_proposes_only_selected_mids() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        // Three drafts with distinct timestamps → distinct generated MIDs.
        for (i, secs) in [1_716_200_000, 1_716_200_001, 1_716_200_002]
            .into_iter()
            .enumerate()
        {
            let m = compose_message(
                "N7CPZ",
                &["W7AUX"],
                &[],
                &format!("Draft-{i}"),
                "body",
                secs,
            );
            mailbox.store(MailboxFolder::Outbox, &m.to_bytes()).unwrap();
        }
        let mids: Vec<String> = mailbox
            .list(MailboxFolder::Outbox)
            .unwrap()
            .into_iter()
            .map(|meta| meta.id.0)
            .collect();
        assert_eq!(
            mids.len(),
            3,
            "fixture must produce 3 distinct MIDs; got {mids:?}"
        );

        // Select exactly 2 of the 3 real MIDs.
        let selected: HashSet<String> = [mids[0].clone(), mids[2].clone()].into_iter().collect();
        let out =
            build_outbound_proposals(&mailbox, SessionIntent::PostOffice, Some(&selected), None)
                .unwrap();
        let returned: HashSet<String> = out.iter().map(|o| o.proposal.mid.clone()).collect();
        assert_eq!(
            returned, selected,
            "PostOffice must propose EXACTLY the selected MID subset; got {returned:?}"
        );
    }

    #[test]
    fn mesh_intent_drains_selected_not_gated() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let m = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Mesh-draft",
            "body",
            1_716_200_000,
        );
        mailbox.store(MailboxFolder::Outbox, &m.to_bytes()).unwrap();
        let mid = mailbox.list(MailboxFolder::Outbox).unwrap()[0].id.0.clone();
        let selected: HashSet<String> = [mid].into_iter().collect();

        // Mesh is NOT gated (its routing flag is the normal C pool, tuxlink-6c9y).
        let out =
            build_outbound_proposals(&mailbox, SessionIntent::Mesh, Some(&selected), None).unwrap();
        assert_eq!(
            out.len(),
            1,
            "Mesh drain must ship the selected draft, not gate; got {out:?}"
        );
    }

    #[test]
    fn selected_but_vanished_mid_is_skipped_not_fatal() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let m = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Present-draft",
            "body",
            1_716_200_000,
        );
        mailbox.store(MailboxFolder::Outbox, &m.to_bytes()).unwrap();
        let mid = mailbox.list(MailboxFolder::Outbox).unwrap()[0].id.0.clone();

        // Select the real MID PLUS a ghost MID that is not in the Outbox.
        let selected: HashSet<String> = [mid, "GHOST-MID-NOT-IN-OUTBOX".to_string()]
            .into_iter()
            .collect();
        let out =
            build_outbound_proposals(&mailbox, SessionIntent::PostOffice, Some(&selected), None)
                .unwrap();
        assert_eq!(
            out.len(),
            1,
            "a selected MID absent from the Outbox must be silently skipped, not fatal; got {out:?}"
        );
    }

    #[test]
    fn none_selection_drains_all_back_compat() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        for secs in [1_716_200_000, 1_716_200_001] {
            let m = compose_message("N7CPZ", &["W7AUX"], &[], "Draft", "body", secs);
            mailbox.store(MailboxFolder::Outbox, &m.to_bytes()).unwrap();
        }
        // `None` selection = drain everything (status-quo CMS behavior).
        let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, None).unwrap();
        assert_eq!(
            out.len(),
            2,
            "None selection must drain all Outbox drafts; got {out:?}"
        );
    }

    #[test]
    fn cms_intent_drains_unchanged_through_safety_gate() {
        // Status-quo invariant: CMS continues to drain all Outbox messages.
        // The safety gate is non-CMS-only; this test pins that CMS is NOT
        // accidentally regressed.
        let (_dir, mailbox) = outbox_with_one_draft();
        let out = build_outbound_proposals(&mailbox, SessionIntent::Cms, None, None)
            .expect("CMS drain must NOT be gated");
        assert_eq!(out.len(), 1, "CMS drain must yield all outbox messages");
    }

    // tuxlink-2ns7 Task 5: the shared Outbox is drained by the ACTIVE session
    // identity. A session connected as W1ABC ships only W1ABC's queued mail.
    #[test]
    fn drain_returns_only_active_identity_outbox() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // Queue one message as W1ABC and one as W7XYZ into the SHARED outbox.
        let alpha = compose_message("W1ABC", &["W1AW"], &[], "Alpha out", "a", 1_716_200_000);
        let xray = compose_message("W7XYZ", &["W1AW"], &[], "Xray out", "x", 1_716_200_600);
        mailbox
            .for_identity("W1ABC")
            .store(MailboxFolder::Outbox, &alpha.to_bytes())
            .unwrap();
        mailbox
            .for_identity("W7XYZ")
            .store(MailboxFolder::Outbox, &xray.to_bytes())
            .unwrap();

        // Active session = W1ABC: only Alpha's message is proposed.
        let out =
            build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W1ABC")).unwrap();
        assert_eq!(
            out.len(),
            1,
            "only the active identity's queued mail drains; got {out:?}"
        );
        assert_eq!(out[0].title, "Alpha out");

        // Active session = W7XYZ: only Xray's.
        let out2 =
            build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W7XYZ")).unwrap();
        assert_eq!(out2.len(), 1);
        assert_eq!(out2[0].title, "Xray out");
    }

    // tuxlink-2ns7 Task 5: an untagged legacy Outbox message (no .identity
    // sidecar) drains for ANY active identity — back-compat / migration safety:
    // a pre-Phase-4 queued draft has no tag and must not be stranded.
    #[test]
    fn untagged_outbox_message_drains_for_any_identity() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        // Store directly via the un-namespaced Mailbox (no .identity sidecar written).
        let legacy = compose_message("W1ABC", &["W1AW"], &[], "Legacy", "x", 1_716_200_000);
        mailbox
            .store(MailboxFolder::Outbox, &legacy.to_bytes())
            .unwrap();

        let out =
            build_outbound_proposals(&mailbox, SessionIntent::Cms, None, Some("W7XYZ")).unwrap();
        assert_eq!(
            out.len(),
            1,
            "an untagged legacy draft is not stranded by identity filtering"
        );
    }
}

/// Backend-instance identifier minted at backend construction time. Embedded
/// in every `Session` so `disconnect` can validate the session came from
/// this backend instance (v2 P0 #1). Process-local `AtomicU64` counter; no
/// UUID dep needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendInstanceId(pub(crate) u64);

static NEXT_BACKEND_ID: AtomicU64 = AtomicU64::new(1);

impl BackendInstanceId {
    pub(crate) fn next() -> Self {
        BackendInstanceId(NEXT_BACKEND_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Opaque session handle. Carries the backend-instance id so cross-backend
/// `disconnect` calls return `BackendError::InvalidSession`. See spec
/// §3.5 for Drop semantics rationale.
#[derive(Debug)]
pub struct Session {
    pub(crate) backend_id: BackendInstanceId,
    /// Backend-specific session payload. Held for future-use match arms in
    /// `disconnect` to call out to native cleanup.
    #[allow(dead_code)]
    pub(crate) inner: SessionInner,
}

#[derive(Debug)]
pub(crate) enum SessionInner {
    /// NativeBackend session. Variant kept for future v0.5+ session shapes.
    Native(()),
}

impl Drop for Session {
    fn drop(&mut self) {
        // Local cleanup only — see spec §3.5. No remote-disconnect call;
        // explicit WinlinkBackend::disconnect is the guaranteed release path.
        // Future native sessions will close their socket fd via Drop on the
        // inner stream.
    }
}

/// Backend connection status. Implementations cache + update internally;
/// `status()` reads the cache (MUST NOT do I/O).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BackendStatus {
    Disconnected,
    Connecting {
        transport: String,
    },
    /// Packet armed-but-idle: the AX.25 layer is listening to answer an inbound
    /// SABM, but no session is up. Distinct from `Connecting` (an active dial)
    /// and `Disconnected` (not armed). Carries the transport so the ribbon can
    /// render "Listening · Packet 1200". (tuxlink-orj)
    Listening {
        transport: String,
    },
    Connected {
        transport: String,
        peer: String,
        since_iso: String,
    },
    Disconnecting,
    Error {
        reason: String,
    },
}

/// Backend log line emitted via `stream_log()`.
#[derive(Debug, Clone)]
pub struct LogLine {
    /// Monotonic sequence number assigned by `SessionLogState::append`.
    /// 0 means "not yet assigned" (pre-append). The bridge writes to the
    /// `SessionLogState` buffer first; `seq` is set by `append`, never
    /// by the bridge or callers directly.
    pub seq: u64,
    pub timestamp_iso: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogSource {
    Backend,
    Transport,
    Wire,
}

// ============================================================================
// Error model (spec §3.3)
// ============================================================================

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BackendError {
    #[error("backend not configured: {0}")]
    NotConfigured(String),

    #[error("message not found: {0:?}")]
    NotFound(MessageId),

    #[error("authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("transport failed: {reason}")]
    TransportFailed {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("backend rejected message: {0}")]
    MessageRejected(String),

    #[error("backend unavailable: {reason}")]
    BackendUnavailable {
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("session does not belong to this backend instance")]
    InvalidSession,

    #[error("operation cancelled")]
    Cancelled,

    /// CMS sent a `*** ...` rejection line (e.g. "Callsign not authorized",
    /// "Secure login failed"). Payload is pre-redacted by
    /// `redaction::redact_freeform` (done at the handshake/session layer before
    /// it bubbles up here). Added by Task 12 (tuxlink-7do4) to give
    /// `cms_connect`'s Err arm a structured handle on the `***` payload for
    /// `auth_taxonomy::classify` — without this variant the payload was only
    /// reachable via the debug string of `TransportFailed`.
    #[error("remote error: {0}")]
    RemoteError(String),

    #[error("not implemented (this backend does not support this operation)")]
    NotImplemented,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("internal error: {msg}")]
    Internal {
        msg: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    },

    #[error("no active identity — authenticate before transmitting")]
    NoActiveIdentity,

    /// A tactical session attempted a CMS mode but its address is not verified
    /// CMS-registered (spec requirement 5). FULL identities and P2P/RF are unaffected.
    #[error("tactical address '{label}' is not verified CMS-registered ({reason}); CMS is unavailable for this identity")]
    TacticalNotCmsRegistered { label: String, reason: String },
}

// ============================================================================
// Trait surface (spec §3.1)
// ============================================================================

/// Per-connect inbound-selection plumbing threaded from `cms_connect` into
/// `native_connect`. Present only on the user-initiated CMS connect; other
/// connect callers pass `None` (and get accept-all). The registry is the SAME
/// Arc that lib.rs `.manage()`s and the resolve command reads (Codex #1).
///
/// Send+Sync+'static (Arc<dyn Sink> is Send+Sync; AttemptId is Copy;
/// SelectionRegistry is Arc<Mutex<…>>), so the whole bundle moves cleanly into
/// the connect path's `spawn_blocking` closure.
pub struct CmsSelectionContext {
    pub sink: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink>,
    pub attempt_id: crate::winlink::b2f_events::AttemptId,
    pub registry: crate::winlink::inbound_selection::SelectionRegistry,
}

/// Backend abstraction for Winlink interactions. See spec §3.1 for the
/// full contract; key invariants:
///
/// - `Send + Sync` — implementors MUST NOT hold a `std::sync::MutexGuard`
///   across an `.await`; use `tokio::sync::Mutex` or contain blocking work
///   in `tokio::task::spawn_blocking`.
/// - `status()` is non-async — implementations cache the value internally
///   and update during connect/disconnect/op flows. MUST NOT do I/O.
/// - `stream_log()` returns `BoxStream<'static, LogLine>` whose Drop
///   cancels the subscription.
#[async_trait]
pub trait WinlinkBackend: Send + Sync {
    async fn list_messages(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError>;

    /// Read a message from a specific folder. Added by Task 12
    /// (tuxlink-zsm): reading a Sent/Outbox message requires the folder,
    /// not just the MID — the prior `read_message` hardcoded Inbox
    /// (winlink_backend.rs, pre-zsm). `read_message` now delegates here
    /// with `MailboxFolder::Inbox` for back-compat. Implementors override
    /// this; `read_message` has a provided default that forwards.
    async fn read_message_in(
        &self,
        folder: MailboxFolder,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError>;

    /// Back-compat shim: read from the Inbox folder. Prefer
    /// [`WinlinkBackend::read_message_in`] when the folder is known
    /// (spec §2.1). Provided default forwards to `read_message_in(Inbox, id)`.
    async fn read_message(&self, id: &MessageId) -> Result<MessageBody, BackendError> {
        self.read_message_in(MailboxFolder::Inbox, id).await
    }

    /// Mark a message read. Best-effort: the default is a no-op.
    /// `NativeBackend` overrides it to drop a read-marker in its store.
    /// A failure here MUST NOT fail the read that triggered it — the caller
    /// (`message_read`) treats read-state as best-effort (tuxlink-xgn).
    async fn mark_read(&self, _folder: MailboxFolder, _id: &MessageId) -> Result<(), BackendError> {
        Ok(())
    }

    /// Set a message's read-state (mark read or unread). Folder-ref aware so
    /// user folders and Archive are covered. Best-effort: default is a no-op.
    /// `NativeBackend` overrides it to write/remove the read-marker.
    async fn set_read_state(
        &self,
        _folder: crate::native_mailbox::FolderRef,
        _id: &MessageId,
        _read: bool,
    ) -> Result<(), BackendError> {
        Ok(())
    }

    /// Move a message between folders (tuxlink-ca5x). The Inbox → Archive path
    /// is the canonical use today; future user folders (tuxlink-f62f) flow
    /// through the same trait method. `NativeBackend` overrides this to
    /// dispatch to its [`Mailbox::move_to`] (which carries the read-marker
    /// across folders + best-effort updates the search index). Default
    /// returns [`BackendError::NotImplemented`].
    async fn move_message(
        &self,
        _from: MailboxFolder,
        _to: MailboxFolder,
        _id: &MessageId,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    // ========================================================================
    // User folders (tuxlink-f62f — Phase 2 of the user-folders work).
    // ========================================================================

    /// List the user folders registered for this backend. `NativeBackend`
    /// reads `<root>/.folders.json`; default is empty.
    async fn list_user_folders(
        &self,
    ) -> Result<Vec<crate::user_folders::UserFolder>, BackendError> {
        Ok(Vec::new())
    }

    /// Create a new user folder with the given display name. `parent_slug`
    /// (spec D2/D3) nests the new folder under an existing top-level folder, or
    /// `None` creates a top-level folder. Validates and slug-derives. Default
    /// `NotImplemented`.
    async fn create_user_folder(
        &self,
        _display_name: &str,
        _parent_slug: Option<&str>,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Delete a user folder, cascading to its direct subfolders. `on_messages`
    /// controls disposition (spec §6 D6). Returns the slugs actually removed
    /// (parent + children) so the UI can clear a stale selection (A5). Default
    /// `NotImplemented`.
    async fn delete_user_folder(
        &self,
        _slug: &str,
        _on_messages: crate::native_mailbox::DeleteAction,
    ) -> Result<Vec<String>, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Re-parent a user folder (spec D3). `new_parent_slug == None` promotes it
    /// to top level. Metadata-only — no message files move. Default
    /// `NotImplemented`.
    async fn move_user_folder(
        &self,
        _slug: &str,
        _new_parent_slug: Option<&str>,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Rename a user folder (display name only; slug is stable per spec §3.1).
    /// Default `NotImplemented`.
    async fn rename_user_folder(
        &self,
        _slug: &str,
        _new_display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// List the messages in a user folder. Default empty.
    async fn list_user_messages(&self, _slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        Ok(Vec::new())
    }

    /// Read one message from a user folder. Default `NotFound`.
    async fn read_user_message(
        &self,
        _slug: &str,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        Err(BackendError::NotFound(id.clone()))
    }

    /// Move a message between any two folder references (system↔user etc).
    /// `NativeBackend` delegates to [`Mailbox::move_between`]; default
    /// `NotImplemented`.
    async fn move_between_folders(
        &self,
        _from: crate::native_mailbox::FolderRef,
        _to: crate::native_mailbox::FolderRef,
        _id: &MessageId,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Delete a message: move it from `from` into the shared `Deleted` (Trash)
    /// folder and record its origin in a `<mid>.trash` sidecar so Restore can
    /// return it (tuxlink-wl7n). `origin_full` is the source identity FULL for
    /// per-identity origins (Inbox/Archive/user folders); `None` for the shared
    /// Sent/Outbox. `NativeBackend` delegates to [`Mailbox::delete_message`];
    /// default `NotImplemented`.
    async fn delete_message_in(
        &self,
        _from: crate::native_mailbox::FolderRef,
        _id: &MessageId,
        _origin_full: Option<&str>,
    ) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Restore a deleted message from Trash back to its recorded origin
    /// (tuxlink-wl7n). `NativeBackend` delegates to [`Mailbox::restore_message`];
    /// default `NotImplemented`.
    async fn restore_message(&self, _id: &MessageId) -> Result<(), BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Permanently purge every message in Trash, returning the count purged
    /// (tuxlink-wl7n). `NativeBackend` delegates to [`Mailbox::empty_trash`];
    /// default `NotImplemented`.
    async fn empty_trash(&self) -> Result<usize, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Permanently purge one message from Trash, returning `1` if it was present
    /// and `0` otherwise (tuxlink-wl7n). `NativeBackend` delegates to
    /// [`Mailbox::purge_message`]; default `NotImplemented`.
    async fn purge_message(&self, _id: &MessageId) -> Result<usize, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Auto-purge sweep: permanently purge every Trash message older than
    /// `retention_days`, returning the count purged (tuxlink-wl7n).
    /// `NativeBackend` delegates to [`Mailbox::purge_expired`] with
    /// `chrono::Utc::now()`; default `NotImplemented`.
    async fn purge_expired_trash(&self, _retention_days: i64) -> Result<usize, BackendError> {
        Err(BackendError::NotImplemented)
    }

    /// Returns `Ok(id)` with the MID assigned at queue time.
    ///
    /// `NativeBackend` assigns a real filesystem-derived MID at queue time.
    async fn send_message(&self, msg: OutboundMessage) -> Result<MessageId, BackendError>;

    /// Queue with an explicit `From` override (Routines `local.compose`'s
    /// run-scoped `from_identity` param, spec §6 "Set identity"). `from:
    /// None` ⇒ identical to [`Self::send_message`] ("the app's current
    /// identity applies"). `from: Some(callsign)` composes+queues under that
    /// exact callsign WITHOUT touching `active_identity()` or persisted
    /// config — the override is per-call only, never the process-shared
    /// session-identity slot [`Self::set_active_identity`] writes. This is
    /// the mechanism spec §6 needs to keep "Set identity" genuinely
    /// run-scoped: mutating the shared slot instead would make parallel
    /// routine runs with different tactical calls race each other, exactly
    /// what run-scoping exists to prevent.
    ///
    /// Default delegates to [`Self::send_message`] and ignores `from` —
    /// matches this trait's existing "unimplemented override, `NativeBackend`
    /// supplies the real behavior" convention (see `abort`/`restore_message`
    /// above).
    async fn send_message_as(
        &self,
        msg: OutboundMessage,
        from: Option<String>,
    ) -> Result<MessageId, BackendError> {
        let _ = from;
        self.send_message(msg).await
    }

    /// Connect and run the exchange. `selection`: `None` ⇒ accept-all (download
    /// all inbound); `Some(CmsSelectionContext)` ⇒ on a CMS connect with the
    /// review-inbound preference on, prompt the operator to select which inbound
    /// messages to download.
    async fn connect(
        &self,
        transport: TransportConfig,
        selection: Option<CmsSelectionContext>,
    ) -> Result<Session, BackendError>;

    async fn disconnect(&self, session: Session) -> Result<(), BackendError>;

    /// Abort an in-flight [`WinlinkBackend::connect`] (tuxlink-9z2): shut down the
    /// connecting socket to unblock a slow TLS/login/exchange phase and return the
    /// backend to `Disconnected`. The aborted `connect` resolves to
    /// [`BackendError::Cancelled`]. Default is a no-op `Ok`. Safe to call when idle.
    async fn abort(&self) -> Result<(), BackendError> {
        Ok(())
    }

    /// Gracefully end an in-flight ESTABLISHED session by letting the link key a DISC
    /// to the remote (tuxlink-avu9), rather than the rude socket-kill of [`abort`].
    /// Unlike `abort`, it unwinds the read loop but leaves the transmit path open, so
    /// `Ax25Stream::drop`'s teardown can send its DISC and the remote isn't left
    /// half-open. Falls back to `abort` semantics if pressed again (force-kill). Default
    /// is a no-op `Ok`; only the packet path implements it meaningfully.
    async fn graceful_disconnect(&self) -> Result<(), BackendError> {
        Ok(())
    }

    /// Auth-only credential test per spec §4.3 (iii): connect to the CMS over
    /// the configured TCP/TLS path, complete the B2F handshake, emit the full
    /// [`crate::winlink::b2f_events::B2fEvent`] stream (including
    /// `PostAuthExchangeStarted` for the Mode 5 discriminator), then quit via
    /// `FF + FQ` without exchanging any messages. The outbox is NEVER read and
    /// the mailbox is NEVER mutated.
    ///
    /// Single-flight: shares [`WinlinkBackend::connect`]'s in-progress guard.
    /// A concurrent `connect` or `cms_connect_test` returns
    /// [`BackendError::BackendUnavailable`].
    ///
    /// RADIO-1 GUARDRAIL: CMS-TELNET ONLY FOREVER. Any future RF-transport
    /// extension requires (a) fresh RADIO-1 review per
    /// `docs/live-cms-testing-policy.md`, (b) explicit transmit-consent gate
    /// at the click moment, and (c) a separate command name
    /// (`cms_connect_test_rf`). See spec §2 out-of-scope + §4.3 (iii).
    ///
    /// Default implementation returns [`BackendError::NotImplemented`].
    async fn cms_connect_test(
        &self,
        events: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink>,
        attempt_id: crate::winlink::b2f_events::AttemptId,
    ) -> Result<(), BackendError> {
        let _ = events;
        let _ = attempt_id;
        Err(BackendError::NotImplemented)
    }

    /// Refresh the live config the connect paths read (tuxlink-ka7 / tuxlink-p5u).
    /// `NativeBackend` originally froze its `config` at construction, so the connect
    /// path read that stale snapshot — a UI host/transport/packet-param change only
    /// took effect after an app restart. The config-writing UI commands call this
    /// after persisting, so the NEXT connect honors the change restart-free. Default
    /// no-op for backends that hold no config snapshot.
    fn set_config(&self, _config: Config) {}

    fn status(&self) -> BackendStatus;

    fn stream_log(&self) -> BoxStream<'static, LogLine>;

    /// The authenticated identity active for this backend (Phase 3,
    /// bd-tuxlink-0063). The on-air station callsign for every transmit/listen
    /// path comes from here — `mycall()` — not from a wire DTO or
    /// `cfg.identity.active_full`. `Err(BackendError::NoActiveIdentity)` when no
    /// identity has been authenticated. `NativeBackend` overrides this with its
    /// inherent slot accessor; the default errors so non-native backends fail
    /// closed (no transmit identity ⇒ no transmit).
    fn active_identity(&self) -> Result<crate::identity::SessionIdentity, BackendError> {
        Err(BackendError::NoActiveIdentity)
    }

    /// Set the active default identity (after a successful authenticate). Default
    /// no-op; `NativeBackend` stores it in its in-memory slot (Phase 6, tuxlink-5ekg).
    fn set_active_identity(&self, _identity: crate::identity::SessionIdentity) {}

    /// Clear the active identity (lock / logout). Default no-op; `NativeBackend`
    /// empties its slot so subsequent FULL-identity ops require re-auth.
    fn clear_active_identity(&self) {}
}

// ============================================================================
// NativeBackend (spec §3.9)
// ============================================================================

/// A sink for per-step connect progress messages (tuxlink-gqo). The connect path
/// runs in `spawn_blocking`, so the sink must be `Send + Sync`; production wires
/// it (in `bootstrap::install_native`) to append a `LogSource::Transport` line to
/// the session log and emit it live. Decoupled from the `LogLine` machinery on
/// purpose — `winlink::telnet` only ever calls it with a `&str` phase message.
pub type ProgressSink = Arc<dyn Fn(&str) + Send + Sync>;

/// A sink for raw B2F wire lines (tuxlink-nki). The connect path tees every
/// on-wire protocol line (both directions) into this; `bootstrap::install_native`
/// wires it to append a `LogSource::Wire` line to the session log + emit it live,
/// so the operator can watch the real `[WL2K-...]`/`;FW`/`FF`/`FQ` dialogue under
/// the "Raw output" view. No-op by default (tests + the no-progress path).
pub type WireSink = Arc<dyn Fn(&str) + Send + Sync>;

/// A sink fired when native mailbox storage mutates. Production wires this to
/// a lightweight Tauri `mailbox:changed` event so React can invalidate mailbox
/// queries immediately instead of waiting for the 10s poll.
pub type MailboxChangeSink = Arc<dyn Fn() + Send + Sync>;

/// Phase 5 (tuxlink-tseu) tactical CMS-registration gate dependencies, bundled so
/// `NativeBackend` gains a single field + builder. Production builds the default
/// (empty access key => verifier fail-closes; real on-disk store path resolved at
/// call time; live reachability probe). Tests inject a mockito-backed verifier, a
/// temp store path, and a forced `online` flag for hermetic, network-free assertions.
struct TacticalCmsGate {
    verifier: crate::identity::TacticalRegistrationVerifier,
    /// `None` => resolve `crate::config::identity_store_path()` at call time
    /// (honors TUXLINK_CONFIG_DIR); `Some(path)` pins it (tests).
    store_path: Option<std::path::PathBuf>,
    /// `None` => probe `api.winlink.org` reachability at call time; `Some(b)` forces it (tests).
    online_override: Option<bool>,
}

impl Default for TacticalCmsGate {
    fn default() -> Self {
        Self {
            verifier: crate::identity::TacticalRegistrationVerifier::new(String::new()),
            store_path: None,
            online_override: None,
        }
    }
}

/// The native Winlink backend: speaks B2F directly (no Pat), stores messages in
/// its own [`Mailbox`], and connects over plaintext or TLS telnet. `connect`
/// runs the real CMS exchange on a blocking task; the actual on-air protocol is
/// validated by `src/bin/native_cms_probe.rs` and the `winlink::*` tests.
pub struct NativeBackend {
    backend_id: BackendInstanceId,
    /// Live config, refreshable via [`WinlinkBackend::set_config`] (tuxlink-ka7 /
    /// tuxlink-p5u). Behind a `RwLock` so a UI host/transport/packet-param change
    /// reaches the connect + send paths without an app restart; reads clone through
    /// [`Self::live_config`].
    config: RwLock<Config>,
    mailbox: Arc<Mailbox>,
    log_tx: broadcast::Sender<LogLine>,
    status: Arc<RwLock<BackendStatus>>,
    /// Broadcasts every BackendStatus transition (2026-05-31): the frontend's
    /// 5s status poll missed sub-second CMS-Z exchanges. Subscribers (the
    /// bootstrap's emitter task) translate these to Tauri events. Best-effort
    /// — send failures (no receivers) are swallowed in set_status.
    status_tx: broadcast::Sender<BackendStatus>,
    progress: ProgressSink,
    /// Sink for raw B2F wire lines (tuxlink-nki): tees the on-wire dialogue into
    /// the session log as `LogSource::Wire` so it surfaces under "Raw output". No-op
    /// by default; production wires it in `bootstrap::install_native`.
    wire: WireSink,
    /// Notifies the UI that mailbox storage changed. No-op in tests unless
    /// injected via [`Self::with_mailbox_change`].
    mailbox_change: MailboxChangeSink,
    /// Shutdown handle for the in-flight connect socket (tuxlink-9z2): a clone of
    /// the connecting `TcpStream`, set once TCP connects, taken + shut down by
    /// [`WinlinkBackend::abort`] to unblock a slow TLS/login/exchange phase.
    abort_handle: Arc<Mutex<Option<TcpStream>>>,
    /// Set by `abort` so the connect's resulting error maps to `Cancelled` (status
    /// `Disconnected`) rather than `Error`.
    aborting: Arc<AtomicBool>,
    /// Set by `graceful_disconnect` (tuxlink-avu9): like `aborting` it unwinds the
    /// in-flight exchange and maps the error to `Cancelled`, but it does NOT block the
    /// transmit path or shut the socket — so the link's Drop teardown still keys its
    /// DISC to the remote (no half-open orphan). The packet path stacks a
    /// `DisconnectableByteLink` keyed on this flag.
    disconnecting: Arc<AtomicBool>,
    /// Single-flight guard (Codex #1): true while a `connect` is running. A second
    /// concurrent `connect` is rejected rather than racing on the shared abort
    /// state and re-sending the outbox. Cleared by a connect-scoped RAII guard so
    /// it is released on every exit (return, `?`, panic).
    connect_in_progress: Arc<AtomicBool>,
    /// Live position source-of-truth (tuxlink-686). When present, the on-air
    /// locator is `arbiter.broadcast_grid()` — live + precision-reduced —
    /// superseding the stale `config` snapshot's grid. `None` in tests / the
    /// no-arbiter path, where `cms_locator(config)` is the fallback.
    position: Option<Arc<crate::position::PositionArbiter>>,
    /// Test-injected Packet listener allowlist (tuxlink-inde). When `Some`, the
    /// Packet `Listen` path uses this in-memory list instead of loading from
    /// `<config-dir>/listener/packet/allowed_stations.json`. Production
    /// (`bootstrap`/UI) leaves this `None` so the disk file is authoritative;
    /// tests inject a permissive list (e.g. `allow_all=TRUE`) to bypass the
    /// architectural default of "reject all until operator curates."
    packet_allowlist_override: Option<crate::winlink::listener::AllowedStations>,
    /// The active default SessionIdentity for NEW connect/compose/listen
    /// operations. In-memory only — NEVER serialized, never written to disk.
    /// Re-established each launch by an authenticated switch (Phase 6). `None`
    /// until the operator authenticates one this session.
    active_identity: RwLock<Option<crate::identity::SessionIdentity>>,
    /// Phase 5 tactical CMS-registration gate deps (tuxlink-tseu). Default in
    /// production (fail-closed empty key); injected in tests.
    tactical_gate: TacticalCmsGate,
}

/// Clears the single-flight + abort state when a `connect` ends, however it ends
/// (Codex #1 + #7): normal return, early `?`, or a panic in the blocking task.
struct ConnectGuard {
    in_progress: Arc<AtomicBool>,
    handle: Arc<Mutex<Option<TcpStream>>>,
}

impl Drop for ConnectGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.handle.lock() {
            *slot = None;
        }
        self.in_progress.store(false, Ordering::SeqCst);
    }
}

impl NativeBackend {
    /// Create a backend for `config`, storing messages under `mailbox_root`, with
    /// a no-op progress sink. Production uses [`NativeBackend::with_progress`] to
    /// surface connect progress in the session log; tests use this no-op form.
    pub fn new(config: Config, mailbox_root: impl Into<PathBuf>) -> Self {
        Self::with_progress(config, mailbox_root, Arc::new(|_: &str| {}))
    }

    /// Like [`NativeBackend::new`] but with a connect-progress sink (tuxlink-gqo).
    pub fn with_progress(
        config: Config,
        mailbox_root: impl Into<PathBuf>,
        progress: ProgressSink,
    ) -> Self {
        let (log_tx, _rx) = broadcast::channel(256);
        // 2026-05-31 operator-flagged: the 5s status poll missed sub-second
        // CMS-Z exchanges. status_tx broadcasts every BackendStatus
        // transition; bootstrap::install_native subscribes + emits Tauri
        // `backend_status:change` events so the frontend sees every state
        // (including the brief Connected window) without poll-rate aliasing.
        // Capacity 64: bursts of state churn (Connecting → Connected →
        // Disconnecting → Disconnected) fit; slow listeners just lose the
        // oldest event (acceptable — the periodic snapshot poll backstops).
        let (status_tx, _status_rx) = broadcast::channel(64);
        Self {
            backend_id: BackendInstanceId::next(),
            config: RwLock::new(config),
            mailbox: Arc::new(Mailbox::new(mailbox_root)),
            log_tx,
            status: Arc::new(RwLock::new(BackendStatus::Disconnected)),
            status_tx,
            progress,
            wire: Arc::new(|_: &str| {}),
            mailbox_change: Arc::new(|| {}),
            abort_handle: Arc::new(Mutex::new(None)),
            aborting: Arc::new(AtomicBool::new(false)),
            disconnecting: Arc::new(AtomicBool::new(false)),
            connect_in_progress: Arc::new(AtomicBool::new(false)),
            position: None,
            packet_allowlist_override: None,
            active_identity: RwLock::new(None),
            tactical_gate: TacticalCmsGate::default(),
        }
    }

    /// Subscribe to live status transitions. The bootstrap installs an
    /// emitter task that consumes this receiver and emits Tauri
    /// `backend_status:change` events. No-op when nothing is subscribed
    /// (broadcast::send returns Err that we swallow in set_status).
    pub fn subscribe_status(&self) -> broadcast::Receiver<BackendStatus> {
        self.status_tx.subscribe()
    }

    /// Attach the live position arbiter (tuxlink-686). Builder-style so existing
    /// constructors and tests are unaffected.
    pub fn with_position(mut self, arbiter: Arc<crate::position::PositionArbiter>) -> Self {
        self.position = Some(arbiter);
        self
    }

    /// Inject a Packet listener allowlist (tuxlink-inde). When set, the
    /// `Listen` role bypasses the disk-backed
    /// `<config-dir>/listener/packet/allowed_stations.json` lookup and uses
    /// this in-memory list instead. Production wires the disk file via
    /// `bootstrap`/UI; tests use this to permit the dialer's callsign without
    /// touching the user's filesystem.
    pub fn with_packet_allowlist(
        mut self,
        allowed: crate::winlink::listener::AllowedStations,
    ) -> Self {
        self.packet_allowlist_override = Some(allowed);
        self
    }

    /// Inject the Phase 5 tactical CMS gate deps for hermetic tests: a
    /// mockito/dead-URL verifier, a temp store path, and a forced `online` flag.
    /// No live network or global env mutation.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_tactical_gate(
        mut self,
        verifier: crate::identity::TacticalRegistrationVerifier,
        store_path: std::path::PathBuf,
        online: bool,
    ) -> Self {
        self.tactical_gate = TacticalCmsGate {
            verifier,
            store_path: Some(store_path),
            online_override: Some(online),
        };
        self
    }

    /// Attach a search index to the mailbox so incremental index hooks run on
    /// every `store`/`move_to`/`mark_read` (Codex adrev — find-messages P1).
    /// Builder-style; must be called before the `mailbox` Arc is cloned (i.e.
    /// before the backend is installed into `BackendState`). Panics if the
    /// Arc is already shared — that would be a programmer error in the boot path.
    pub fn with_index(mut self, index: Arc<std::sync::Mutex<crate::search::index::Index>>) -> Self {
        let mbox = Arc::try_unwrap(self.mailbox)
            .unwrap_or_else(|_| {
                panic!("with_index called after Arc<Mailbox> was shared — call before install")
            })
            .with_index(index);
        self.mailbox = Arc::new(mbox);
        self
    }

    /// Set the mailbox's default received-mail identity to the operator's sole
    /// FULL (tuxlink-2ns7). After this, a bare `store`/`list`/`read`/`mark_read`
    /// on the backend's mailbox resolves Inbox/Archive + user folders under
    /// `mailbox/<FULL>/` (Sent/Outbox stay shared) — matching where
    /// [`Mailbox::migrate_legacy_layout`] re-homes legacy mail. Builder-style;
    /// must be called before the `mailbox` Arc is shared (i.e. before install),
    /// like [`Self::with_index`]. Panics if the Arc is already shared — a
    /// programmer error in the boot path.
    pub fn with_default_identity(mut self, full: &crate::identity::Callsign) -> Self {
        let mbox = Arc::try_unwrap(self.mailbox)
            .unwrap_or_else(|_| panic!("with_default_identity called after Arc<Mailbox> was shared — call before install"))
            .with_default_identity(full);
        self.mailbox = Arc::new(mbox);
        self
    }

    /// Attach a raw-wire log sink (tuxlink-nki). Builder-style so existing
    /// constructors and tests are unaffected; no-op by default.
    pub fn with_wire_log(mut self, wire: WireSink) -> Self {
        self.wire = wire;
        self
    }

    /// Attach a mailbox-change sink (tuxlink-b2sk). Builder-style so tests and
    /// bootstrap can observe native mailbox mutations without changing the
    /// WinlinkBackend trait surface.
    pub fn with_mailbox_change(mut self, sink: MailboxChangeSink) -> Self {
        self.mailbox_change = sink;
        self
    }

    /// Set the active default identity for new operations. In-memory only.
    pub fn set_active_identity(&self, s: crate::identity::SessionIdentity) {
        match self.active_identity.write() {
            Ok(mut slot) => *slot = Some(s),
            Err(poisoned) => *poisoned.into_inner() = Some(s),
        }
    }

    /// Clear the active default identity (lock / shutdown). Subsequent transmit /
    /// listen-arm / Outbox-drain require a re-auth.
    pub fn clear_active_identity(&self) {
        match self.active_identity.write() {
            Ok(mut slot) => *slot = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
    }

    /// Clone the active SessionIdentity for a single operation.
    /// `Err(NoActiveIdentity)` if the operator hasn't authenticated one yet.
    pub fn active_identity(&self) -> Result<crate::identity::SessionIdentity, BackendError> {
        let guard = self
            .active_identity
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone().ok_or(BackendError::NoActiveIdentity)
    }

    /// Shared compose+store body for [`WinlinkBackend::send_message`] and
    /// [`WinlinkBackend::send_message_as`] — the only difference between the
    /// two trait methods is how `from` is resolved (session/config lookup
    /// vs. a direct per-call override), so both funnel here once `from` is
    /// known. Mirrors the pre-refactor `send_message` body exactly (same
    /// `compose_message_with_files` + `Outbox` store + change-notify calls).
    fn queue_message(&self, msg: OutboundMessage, from: String) -> Result<MessageId, BackendError> {
        // The trait carries an RFC 3339 date; fall back to now if unparseable.
        let unix_secs = parse_rfc3339_secs(&msg.date).unwrap_or_else(now_unix_secs);
        let to: Vec<&str> = msg.to.iter().map(String::as_str).collect();
        let cc: Vec<&str> = msg.cc.iter().map(String::as_str).collect();
        let message = compose::compose_message_with_files(
            &from,
            &to,
            &cc,
            &msg.subject,
            &msg.body,
            &msg.attachments,
            unix_secs,
        )
        .map_err(|e| BackendError::MessageRejected(e.to_string()))?;
        let id = self
            .mailbox
            .store(MailboxFolder::Outbox, &message.to_bytes())?;
        (self.mailbox_change)();
        Ok(id)
    }

    /// Phase 5 (tuxlink-tseu): refuse CMS entry for a tactical session whose
    /// address is not verified CMS-registered (fail-closed). A FULL session is a
    /// no-op (`Ok`). Reads + persists the 24h cache from the on-disk IdentityStore.
    /// Never gates non-CMS transports — the caller only invokes this on the CMS path.
    async fn enforce_tactical_cms_gate(
        &self,
        session_id: &crate::identity::SessionIdentity,
    ) -> Result<(), BackendError> {
        use crate::identity::{gate_cms_entry, Address, GateOutcome, IdentityStore};
        let label = match session_id.address_as() {
            Address::Tactical(l) => l.clone(),
            Address::Full(_) => return Ok(()), // FULL identities are never CMS-gated
        };
        let parent = session_id.mycall().clone();
        let store_path = self
            .tactical_gate
            .store_path
            .clone()
            .unwrap_or_else(crate::config::identity_store_path);
        let mut store = IdentityStore::load(&store_path).unwrap_or_default();
        let online = match self.tactical_gate.online_override {
            Some(b) => b,
            None => host_reachable("api.winlink.org:443").await,
        };
        let now = now_unix_secs();
        let before = store
            .tactical()
            .iter()
            .find(|t| t.label == label && t.parent.as_str() == parent.as_str())
            .map(|t| t.cms.clone());
        let outcome = gate_cms_entry(
            &mut store,
            &label,
            &parent,
            &self.tactical_gate.verifier,
            online,
            now,
        )
        .await;
        // Persist ONLY if a re-verification actually refreshed the cached state.
        // The common case (cached, or offline with no verify) changes nothing, so a
        // blanket save would needlessly rewrite the whole IdentityStore on every
        // gated connect — widening the window to clobber a concurrent store writer
        // for zero benefit. A failed save never upgrades a refusal to Allow: the
        // decision was made on in-memory state.
        let after = store
            .tactical()
            .iter()
            .find(|t| t.label == label && t.parent.as_str() == parent.as_str())
            .map(|t| t.cms.clone());
        if after != before {
            let _ = store.save();
        }
        match outcome {
            GateOutcome::Allow => Ok(()),
            GateOutcome::Refuse(reason) => Err(BackendError::TacticalNotCmsRegistered {
                label,
                reason: format!("{reason:?}"),
            }),
        }
    }

    /// Clone the live config (tuxlink-ka7 / tuxlink-p5u). The connect + send paths
    /// read through here so a [`WinlinkBackend::set_config`] refresh applies on the
    /// next operation without an app restart. Recovers a poisoned lock's inner value
    /// rather than panicking — a poisoned config lock must not brick every connect.
    fn live_config(&self) -> Config {
        self.config
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    fn set_status(&self, status: BackendStatus) {
        if let Ok(mut s) = self.status.write() {
            *s = status.clone();
        }
        // Best-effort broadcast for the event-emitter task (2026-05-31). Send
        // returns Err when there are no active subscribers — that's fine, the
        // RwLock above remains the snapshot source for backend_status polls.
        let _ = self.status_tx.send(status);
    }
}

#[async_trait]
impl WinlinkBackend for NativeBackend {
    /// Phase 3 (bd-tuxlink-0063): expose the inherent active-identity slot on
    /// the trait so the Tauri command layer (which holds an
    /// `Arc<dyn WinlinkBackend>`) can read the authenticated station call.
    /// Delegates to the inherent accessor; named explicitly to avoid resolving
    /// back into this trait method.
    fn active_identity(&self) -> Result<crate::identity::SessionIdentity, BackendError> {
        NativeBackend::active_identity(self)
    }

    fn set_active_identity(&self, identity: crate::identity::SessionIdentity) {
        NativeBackend::set_active_identity(self, identity)
    }

    fn clear_active_identity(&self) {
        NativeBackend::clear_active_identity(self)
    }

    async fn list_messages(&self, folder: MailboxFolder) -> Result<Vec<MessageMeta>, BackendError> {
        self.mailbox.list(folder)
    }

    async fn read_message_in(
        &self,
        folder: MailboxFolder,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        self.mailbox.read(folder, id)
    }

    async fn mark_read(&self, folder: MailboxFolder, id: &MessageId) -> Result<(), BackendError> {
        self.mailbox.mark_read(folder, id)
    }

    async fn set_read_state(
        &self,
        folder: crate::native_mailbox::FolderRef,
        id: &MessageId,
        read: bool,
    ) -> Result<(), BackendError> {
        self.mailbox.set_read_state(&folder, id, read)
    }

    async fn move_message(
        &self,
        from: MailboxFolder,
        to: MailboxFolder,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        self.mailbox.move_to(from, to, id)
    }

    async fn list_user_folders(
        &self,
    ) -> Result<Vec<crate::user_folders::UserFolder>, BackendError> {
        Ok(self.mailbox.list_user_folders())
    }

    async fn create_user_folder(
        &self,
        display_name: &str,
        parent_slug: Option<&str>,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        self.mailbox.create_user_folder(display_name, parent_slug)
    }

    async fn delete_user_folder(
        &self,
        slug: &str,
        on_messages: crate::native_mailbox::DeleteAction,
    ) -> Result<Vec<String>, BackendError> {
        self.mailbox.delete_user_folder(slug, on_messages)
    }

    async fn move_user_folder(
        &self,
        slug: &str,
        new_parent_slug: Option<&str>,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        self.mailbox.move_user_folder(slug, new_parent_slug)
    }

    async fn rename_user_folder(
        &self,
        slug: &str,
        new_display_name: &str,
    ) -> Result<crate::user_folders::UserFolder, BackendError> {
        self.mailbox.rename_user_folder(slug, new_display_name)
    }

    async fn list_user_messages(&self, slug: &str) -> Result<Vec<MessageMeta>, BackendError> {
        self.mailbox.list_user(slug)
    }

    async fn read_user_message(
        &self,
        slug: &str,
        id: &MessageId,
    ) -> Result<MessageBody, BackendError> {
        self.mailbox.read_user(slug, id)
    }

    async fn move_between_folders(
        &self,
        from: crate::native_mailbox::FolderRef,
        to: crate::native_mailbox::FolderRef,
        id: &MessageId,
    ) -> Result<(), BackendError> {
        self.mailbox.move_between(from, to, id)?;
        (self.mailbox_change)();
        Ok(())
    }

    async fn delete_message_in(
        &self,
        from: crate::native_mailbox::FolderRef,
        id: &MessageId,
        origin_full: Option<&str>,
    ) -> Result<(), BackendError> {
        // tuxlink-wl7n: deleting an Outbox message is ALWAYS permitted, including
        // during a live session — it is the operator's "cancel this queued send"
        // control. No actively-transmitting guard (the design's original Outbox
        // guard was struck per operator 2026-06-21): sessions are long and the
        // Outbox is an awaiting-send holding area, so blocking/greying delete
        // there reads as a broken client; and the send loop snapshots messages at
        // connect time, so deleting the file does not corrupt an in-flight
        // transfer.
        //
        // `Mailbox::delete_message` accepts a `FolderRef`, so both a system
        // folder and a user folder write a `<mid>.trash` sidecar recording the
        // origin (the system `as_path()` name or the user-folder slug) plus the
        // origin identity. Restore reads that sidecar to return the message to
        // its source folder — including back into a user folder (tuxlink-wl7n).
        let now = chrono::Utc::now().to_rfc3339();
        self.mailbox.delete_message(from, id, origin_full, &now)?;
        (self.mailbox_change)();
        Ok(())
    }

    async fn restore_message(&self, id: &MessageId) -> Result<(), BackendError> {
        self.mailbox.restore_message(id)?;
        (self.mailbox_change)();
        Ok(())
    }

    async fn empty_trash(&self) -> Result<usize, BackendError> {
        let n = self.mailbox.empty_trash()?;
        (self.mailbox_change)();
        Ok(n)
    }

    async fn purge_message(&self, id: &MessageId) -> Result<usize, BackendError> {
        // `Mailbox::purge_message` unlinks ignore-NotFound, so it cannot itself
        // report whether the message was present. Probe the Trash listing first
        // so the trait's 0/1 contract is honored.
        let present = self
            .mailbox
            .list(MailboxFolder::Deleted)?
            .iter()
            .any(|m| m.id == *id);
        self.mailbox.purge_message(id)?;
        (self.mailbox_change)();
        Ok(usize::from(present))
    }

    async fn purge_expired_trash(&self, retention_days: i64) -> Result<usize, BackendError> {
        let n = self
            .mailbox
            .purge_expired(chrono::Utc::now(), retention_days)?;
        if n > 0 {
            (self.mailbox_change)();
        }
        Ok(n)
    }

    async fn send_message(&self, msg: OutboundMessage) -> Result<MessageId, BackendError> {
        // GH #691 / tuxlink-spbw: queueing to the Outbox is NOT transmitting, so it
        // must not require an AUTHENTICATED active identity. Use the active
        // identity's address when one is authenticated; otherwise fall back to the
        // operator's configured active-full callsign so an RF-only / offline
        // operator (callsign configured, no stored CMS password) can still draft a
        // message into the Outbox. The authenticated-identity gate stays where it
        // belongs — at connect/transmit time, NOT at queue time.
        let from: String = match self.active_identity() {
            Ok(session_id) => address_string(session_id.address_as()).to_string(),
            Err(_) => self
                .live_config()
                .identity
                .active_full
                .clone()
                .ok_or(BackendError::NoActiveIdentity)?,
        };
        self.queue_message(msg, from)
    }

    async fn send_message_as(
        &self,
        msg: OutboundMessage,
        from: Option<String>,
    ) -> Result<MessageId, BackendError> {
        // Routines `local.compose`'s run-scoped `from_identity` override
        // (trait doc comment above). `from: None` delegates straight to
        // `send_message` (its active-identity-then-config resolution is not
        // duplicated here); `from: Some(..)` funnels into the same
        // `queue_message` helper `send_message` itself uses, just with the
        // override skipping that resolution entirely.
        let from = match from {
            Some(f) => f,
            None => {
                return self.send_message(msg).await;
            }
        };
        self.queue_message(msg, from)
    }

    async fn connect(
        &self,
        transport: TransportConfig,
        selection: Option<CmsSelectionContext>,
    ) -> Result<Session, BackendError> {
        // Dispatch to per-transport paths. The packet path runs no CMS inbound
        // selection, so it drops `selection` (callers pass `None` there anyway).
        if let TransportConfig::Packet {
            link,
            ssid,
            role,
            intent,
        } = transport
        {
            return self.packet_connect_inner(link, ssid, role, intent).await;
        }
        let mode = match transport {
            TransportConfig::Cms { mode } => mode,
            _ => return Err(BackendError::NotImplemented),
        };

        // Single-flight (Codex #1): refuse a concurrent connect rather than racing
        // on the shared abort state and re-sending the outbox. The RAII guard
        // releases the flag + clears the abort handle on EVERY exit — normal
        // return, early `?`, or a panic in the blocking task (Codex #7).
        if self
            .connect_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackendError::BackendUnavailable {
                reason: "a CMS connection is already in progress".to_string(),
                source: None,
            });
        }
        let _guard = ConnectGuard {
            in_progress: self.connect_in_progress.clone(),
            handle: self.abort_handle.clone(),
        };

        let config = self.live_config();
        let mailbox = self.mailbox.clone();

        // Fresh abort epoch: clear any stale flag/handle from a prior connect so
        // an earlier abort can't bleed into this one (tuxlink-9z2).
        self.aborting.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            *slot = None;
        }

        self.set_status(BackendStatus::Connecting {
            transport: format!("{mode:?}"),
        });

        // The exchange is blocking (sockets + files); run it off the async runtime.
        // `progress` surfaces per-step connect progress in the session log
        // (tuxlink-gqo); `abort_handle` receives the connecting socket so abort can
        // shut it down (tuxlink-9z2). Both are Arcs cloned into the blocking task.
        // `position` is the live arbiter clone (tuxlink-686): when present,
        // `native_connect` uses the arbiter's `broadcast_grid()` as the on-air
        // locator, superseding the stale `config` snapshot's grid.
        let progress = self.progress.clone();
        let wire = self.wire.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let position = self.position.clone();
        let mailbox_change = self.mailbox_change.clone();
        // tuxlink-0063 (Phase 3): resolve the active identity BEFORE the blocking
        // task and thread it in. `?` surfaces `NoActiveIdentity` to the operator
        // if no identity has been authenticated — the dial cannot proceed without
        // a Part 97 station principal to ID as on air.
        let session_id = self.active_identity()?;
        // Phase 5 (tuxlink-tseu): a tactical session may only enter CMS modes when
        // its tactical address is verified CMS-registered (fail-closed). FULL
        // identities are never gated. On refusal, set a terminal status and return
        // WITHOUT dialing. P2P/Packet never reaches here (early-returned above).
        if let Err(e) = self.enforce_tactical_cms_gate(&session_id).await {
            self.set_status(BackendStatus::Error {
                reason: e.to_string(),
            });
            return Err(e);
        }
        let outcome = tokio::task::spawn_blocking(move || {
            native_connect(
                &config,
                &session_id,
                &mailbox,
                mode,
                &*progress,
                &*wire,
                &*mailbox_change,
                &abort_handle,
                aborting,
                position.as_deref(),
                selection,
            )
        })
        .await
        .map_err(|e| BackendError::Internal {
            msg: format!("native connect task failed: {e}"),
            source: None,
        })?;

        // An error after an operator abort is a cancellation, not a failure. The
        // `_guard` clears the abort handle + single-flight flag when this fn returns.
        match abort_aware_outcome(outcome, self.aborting.load(Ordering::SeqCst)) {
            Ok(()) => {
                self.set_status(BackendStatus::Connected {
                    transport: format!("{mode:?}"),
                    // tuxlink-3o0: the peer is the host actually dialed (the
                    // operator's configured host, or the TUXLINK_CMS_HOST override)
                    // — no longer a hardcoded const.
                    peer: resolve_cms_host(&self.live_config()),
                    since_iso: now_iso8601_utc(),
                });
                Ok(Session {
                    backend_id: self.backend_id,
                    inner: SessionInner::Native(()),
                })
            }
            Err(BackendError::Cancelled) => {
                self.set_status(BackendStatus::Disconnected);
                Err(BackendError::Cancelled)
            }
            Err(e) => {
                self.set_status(BackendStatus::Error {
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }

    async fn disconnect(&self, session: Session) -> Result<(), BackendError> {
        if session.backend_id != self.backend_id {
            return Err(BackendError::InvalidSession);
        }
        self.set_status(BackendStatus::Disconnected);
        Ok(())
    }

    async fn abort(&self) -> Result<(), BackendError> {
        // Mark the abort (so the in-flight connect's error maps to Cancelled), shut
        // down the connecting socket to unblock a slow TLS/login/exchange phase, and
        // return to Disconnected. A no-op if nothing is in flight (handle is None).
        self.aborting.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            if let Some(sock) = slot.take() {
                let _ = sock.shutdown(Shutdown::Both);
            }
        }
        self.set_status(BackendStatus::Disconnected);
        Ok(())
    }

    /// Graceful packet teardown (tuxlink-avu9). Sets `disconnecting` (NOT `aborting`)
    /// and does NOT shut the socket: the in-flight exchange's read unwinds via the
    /// stacked `DisconnectableByteLink`, the exchange returns, and `Ax25Stream::drop`
    /// keys its DISC over the still-open write — so the remote sees a clean session end
    /// instead of being orphaned half-open. A subsequent `abort()` still hard-kills
    /// (force path), preserving the RADIO-1 runaway stop.
    async fn graceful_disconnect(&self) -> Result<(), BackendError> {
        self.disconnecting.store(true, Ordering::SeqCst);
        self.set_status(BackendStatus::Disconnected);
        Ok(())
    }

    /// Auth-only credential test per spec §4.3 (iii). Shares the single-flight
    /// guard with `connect` so a concurrent `cms_connect` or `cms_connect_test`
    /// returns `BackendUnavailable`.  Mirrors `native_connect`'s TCP/TLS dial
    /// path but calls `telnet::connect_and_auth_test` instead of
    /// `connect_and_exchange`, so it never reads inbound proposals and never
    /// mutates the mailbox.
    ///
    /// RADIO-1 GUARDRAIL: CMS-TELNET ONLY. See trait doc.
    async fn cms_connect_test(
        &self,
        events: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink>,
        attempt_id: crate::winlink::b2f_events::AttemptId,
    ) -> Result<(), BackendError> {
        // Single-flight: shared with connect().
        if self
            .connect_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackendError::BackendUnavailable {
                reason: "a CMS connection is already in progress".to_string(),
                source: None,
            });
        }
        let _guard = ConnectGuard {
            in_progress: self.connect_in_progress.clone(),
            handle: self.abort_handle.clone(),
        };

        let config = self.live_config();

        // Fresh abort epoch (mirrors native_connect).
        self.aborting.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            *slot = None;
        }

        self.set_status(BackendStatus::Connecting {
            transport: "CmsAuthTest".to_string(),
        });

        // tuxlink-0063 (Phase 3): the on-air station ID is the session's full
        // callsign — the Part 97 principal — not `config.identity.active_full`.
        // Resolve before spawn_blocking so `?` surfaces NoActiveIdentity to the
        // caller immediately (mirrors native_connect / NativeBackend::connect).
        let session_id = self.active_identity()?;
        // Phase 5 (tuxlink-tseu): cms_connect_test is a CMS-Telnet entry point — it
        // dials + authenticates against the CMS — so it is gated identically to
        // NativeBackend::connect. A tactical session may not enter CMS modes unless
        // verified CMS-registered (fail-closed); otherwise the password test would
        // be an ungated CMS-Telnet sibling that defeats the gate. FULL sessions are
        // never gated. On refusal: terminal status + return WITHOUT dialing.
        if let Err(e) = self.enforce_tactical_cms_gate(&session_id).await {
            self.set_status(BackendStatus::Error {
                reason: e.to_string(),
            });
            return Err(e);
        }
        let callsign = session_id.mycall().as_str().to_uppercase();
        let locator =
            crate::position::effective_broadcast_locator(&config, self.position.as_deref());
        let password = crate::winlink::credentials::read_password(&callsign)
            .ok()
            .filter(|p| !p.is_empty());

        let plaintext_override = std::env::var("TUXLINK_CMS_PLAINTEXT").is_ok();
        let port_override = std::env::var("TUXLINK_CMS_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok());
        let transport_mode = config.connect.transport;
        let (port, transport) =
            resolve_cms_endpoint(transport_mode, plaintext_override, port_override);
        let host = resolve_cms_host(&config);

        let exchange_config = session::ExchangeConfig {
            mycall: callsign,
            targetcall: telnet::CMS_TARGET_CALL.to_string(),
            locator,
            password,
            intent: session::SessionIntent::Cms,
        };

        let progress = self.progress.clone();
        let wire = self.wire.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let events_arc = events.clone();

        let outcome = tokio::task::spawn_blocking(move || {
            let register_socket = |sock: &std::net::TcpStream| {
                if let Ok(clone) = sock.try_clone() {
                    if let Ok(mut slot) = abort_handle.lock() {
                        if aborting.load(Ordering::SeqCst) {
                            let _ = clone.shutdown(Shutdown::Both);
                        } else {
                            *slot = Some(clone);
                        }
                    }
                }
            };
            telnet::connect_and_auth_test(
                &host,
                port,
                transport,
                &exchange_config,
                &*progress,
                &*wire,
                &register_socket,
                Some(events_arc.as_ref()),
                attempt_id,
            )
            .map_err(|e| {
                use crate::winlink::handshake::HandshakeError;
                use crate::winlink::session::ExchangeError;
                use telnet::TelnetError;
                match e {
                    TelnetError::Exchange(ExchangeError::RemoteError(payload)) => {
                        BackendError::RemoteError(payload)
                    }
                    TelnetError::Exchange(ExchangeError::Handshake(
                        HandshakeError::RemoteError(payload),
                    )) => BackendError::RemoteError(payload),
                    other => BackendError::TransportFailed {
                        reason: format!("{other:?}"),
                        source: None,
                    },
                }
            })
        })
        .await
        .map_err(|e| BackendError::Internal {
            msg: format!("cms_connect_test task failed: {e}"),
            source: None,
        })?;

        // abort_aware_outcome expects Result<(), BackendError>; map the
        // ExchangeResult to () since cms_connect_test discards message data.
        let unit_outcome = outcome.map(|_| ());
        match abort_aware_outcome(unit_outcome, self.aborting.load(Ordering::SeqCst)) {
            Ok(()) => {
                self.set_status(BackendStatus::Disconnected);
                Ok(())
            }
            Err(BackendError::Cancelled) => {
                self.set_status(BackendStatus::Disconnected);
                Err(BackendError::Cancelled)
            }
            Err(e) => {
                self.set_status(BackendStatus::Disconnected);
                Err(e)
            }
        }
    }

    /// Refresh the live config the connect + send paths read (tuxlink-ka7 /
    /// tuxlink-p5u). Called by the config-writing UI commands after they persist, so
    /// the next connect honors a host/transport/packet-param change restart-free.
    /// Recovers a poisoned lock rather than panicking — a failed write must not wedge
    /// the backend.
    fn set_config(&self, config: Config) {
        match self.config.write() {
            Ok(mut slot) => *slot = config,
            Err(poisoned) => *poisoned.into_inner() = config,
        }
    }

    fn status(&self) -> BackendStatus {
        self.status
            .read()
            .map(|s| s.clone())
            .unwrap_or(BackendStatus::Error {
                reason: "status RwLock poisoned".to_string(),
            })
    }

    fn stream_log(&self) -> BoxStream<'static, LogLine> {
        let rx = self.log_tx.subscribe();
        BroadcastStream::new(rx)
            .filter_map(|res| async move { res.ok() })
            .boxed()
    }
}

impl NativeBackend {
    /// Packet-transport connect path (Task 5/6): resolve the endpoint, open the
    /// KISS link, connect/answer, and run the exchange. Wired here from the
    /// `WinlinkBackend::connect` dispatch above.
    async fn packet_connect_inner(
        &self,
        link: KissLinkConfig,
        ssid: u8,
        role: PacketRole,
        intent: SessionIntent,
    ) -> Result<Session, BackendError> {
        // Single-flight guard (same as the CMS arm).
        if self
            .connect_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(BackendError::BackendUnavailable {
                reason: "a connection is already in progress".to_string(),
                source: None,
            });
        }
        let _guard = ConnectGuard {
            in_progress: self.connect_in_progress.clone(),
            handle: self.abort_handle.clone(),
        };

        self.aborting.store(false, Ordering::SeqCst);
        // tuxlink-avu9: fresh graceful-disconnect epoch alongside the abort epoch.
        self.disconnecting.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.abort_handle.lock() {
            *slot = None;
        }

        // tuxlink-0063 (Phase 3, Task 3.8): derive the base call from the active
        // SessionIdentity — the authenticated Part 97 principal — NOT from
        // config.identity.active_full. The SSID stays config-driven; only the
        // base call moves to the session. Fail-closed: NoActiveIdentity returns
        // before any KISS/TNC state is touched; ConnectGuard is already constructed
        // so the single-flight flag is correctly cleared on this early return.
        let session_id = self.active_identity()?;
        let base = session_id.mycall().as_str().to_uppercase();
        // plan 2 Task 5c (`connection_history`): capture the DIALED target
        // callsign BEFORE `role` moves into `resolve_packet_endpoint` below —
        // `None` for a `Listen` role (an inbound-answer session; no target
        // callsign is known until a peer answers, and recording nothing for
        // that case is unchanged from today's behavior, not a regression).
        let dial_target = if let PacketRole::DialTo { call, .. } = &role {
            Some(call.clone())
        } else {
            None
        };
        // Decide the armed-state status before `role` is moved into resolve
        // (tuxlink-orj): Listen → Listening (armed), DialTo → Connecting (dial).
        let initial_status = initial_packet_status(&role, ssid);
        let resolved = resolve_packet_endpoint(&base, ssid, role, intent)?;

        let config = self.live_config();
        let mailbox = self.mailbox.clone();
        let progress = self.progress.clone();
        let wire = self.wire.clone();
        let abort_handle = self.abort_handle.clone();
        let aborting = self.aborting.clone();
        let disconnecting = self.disconnecting.clone();
        // tuxlink-uvi7: thread the shared PositionArbiter into the packet path so the
        // B2F greeting broadcasts the live locator (Arc is Send + 'static, safe to move
        // across spawn_blocking). Mirrors native_connect (telnet).
        let position = self.position.clone();
        let allowlist_override = self.packet_allowlist_override.clone();

        self.set_status(initial_status);

        let outcome = tokio::task::spawn_blocking(move || {
            native_packet_connect(
                &config,
                &mailbox,
                link,
                resolved,
                &*progress,
                &wire,
                &abort_handle,
                aborting,
                disconnecting,
                position,
                allowlist_override,
            )
        })
        .await
        .map_err(|e| BackendError::Internal {
            msg: format!("packet connect task failed: {e}"),
            source: None,
        })?;

        // tuxlink-avu9: a graceful disconnect unwinds the exchange too — treat its
        // error as a clean cancel (Disconnected), not a transport failure.
        let stopped =
            self.aborting.load(Ordering::SeqCst) || self.disconnecting.load(Ordering::SeqCst);
        match abort_aware_outcome(outcome, stopped) {
            Ok(()) => {
                // plan 2 Task 5c (`connection_history`): "connect, forward
                // staged outbox traffic" completed successfully — this IS
                // the session/exchange-completion chokepoint for packet
                // (radio.rs's own doc: "does dial + B2F exchange in one
                // call"). Only for a DialTo session — an inbound Listen
                // answer has no dialed target to record.
                if let Some(ref target) = dial_target {
                    crate::connection_history::record_success(target, "packet");
                }
                self.set_status(BackendStatus::Connected {
                    transport: format!("Packet-{ssid}"),
                    peer: "packet".to_string(),
                    since_iso: now_iso8601_utc(),
                });
                Ok(Session {
                    backend_id: self.backend_id,
                    inner: SessionInner::Native(()),
                })
            }
            Err(BackendError::Cancelled) => {
                self.set_status(BackendStatus::Disconnected);
                Err(BackendError::Cancelled)
            }
            Err(e) => {
                self.set_status(BackendStatus::Error {
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }
}

// ============================================================================
// Packet-transport functions (native_packet_exchange + native_packet_connect)
// Stubs — fully implemented in Tasks 5 and 6.
// ============================================================================

/// Run one B2F exchange over an already-connected AX.25 stream. By-value
/// ownership: the stream is consumed + dropped on return (DISC fires from
/// `Ax25Stream::drop`). Generic over `Read + Write` so it is fully
/// unit-tested with an in-memory `FakeAx25Stream` — no network, no RF.
///
/// Session-identity context for a packet exchange. Groups the per-session
/// identity parameters (`base_mycall`, `targetcall`, `password`, `role`,
/// `locator`) to keep `native_packet_exchange` under the clippy
/// `too_many_arguments` threshold (7).
struct PacketConnectCtx<'a> {
    /// B2F identity call (base callsign, no SSID; spec §4.4).
    base_mycall: &'a str,
    /// Peer callsign (gateway or P2P peer).
    targetcall: &'a str,
    /// Winlink password for gateway secure-login (None for P2P).
    password: Option<String>,
    /// Exchange role: Dial (slave) for DialTo, Answer (master) for Listen.
    role: ExchangeRole,
    /// Grid locator at configured broadcast precision.
    locator: &'a str,
    /// [R4-3][R1-C15][R5-3] Which message pool this session belongs to.
    /// Carried through to [`session::ExchangeConfig::intent`] via
    /// [`exchange_config_for_packet`].
    intent: SessionIntent,
}

/// The [`session::ExchangeConfig`] for a packet session — pure, so the intent
/// contract [R5-3] is pinned without a KISS link.
fn exchange_config_for_packet(ctx: &PacketConnectCtx<'_>) -> session::ExchangeConfig {
    session::ExchangeConfig {
        mycall: ctx.base_mycall.to_string(), // BASE call — no SSID in B2F identity
        targetcall: ctx.targetcall.to_string(),
        locator: ctx.locator.to_string(),
        password: ctx.password.clone(),
        intent: ctx.intent,
    }
}

/// Streams whose `read()` returns `Ok(0)` for "no data yet" rather than EOF (the
/// `Ax25Stream` defect-J contract) expose closed-ness here so [`BlockingB2fStream`]
/// can tell a transient idle read from a genuine end-of-link.
trait MaybeClosed {
    fn is_closed(&self) -> bool;
}
impl MaybeClosed for crate::winlink::ax25::Ax25Stream {
    fn is_closed(&self) -> bool {
        crate::winlink::ax25::Ax25Stream::is_closed(self)
    }
}

/// Adapts an `Ax25Stream` to the `std::io::Read` EOF contract for the B2F layer.
///
/// `Ax25Stream::read` returns `Ok(0)` for "no data buffered yet" (defect-J), but
/// `BufReader`/`read_until` — which `wire::read_line` is built on — treat `Ok(0)`
/// as **EOF**. So on a real RF link, the first inter-frame gap longer than the
/// link poll window would abort the handshake/exchange as `ConnectionClosed`.
/// This adapter blocks until ≥1 byte arrives, the link genuinely closes (`Ok(0)`
/// while `is_closed()`, e.g. an inbound DISC), or an error — making `Ok(0)` mean
/// EOF as the contract requires. Found by a Codex adversarial round (2026-05-22);
/// the localhost TCP-relay e2e test masked it because bytes arrive instantly.
struct BlockingB2fStream<S>(S);

impl<S: std::io::Read + MaybeClosed> std::io::Read for BlockingB2fStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let n = self.0.read(buf)?;
            // n > 0: data. n == 0 && closed: genuine EOF. n == 0 && open: no data
            // yet — loop. `Ax25Stream::read` naps a poll interval when idle, so
            // this is a poll, not a busy-spin.
            if n > 0 || self.0.is_closed() {
                return Ok(n);
            }
        }
    }
}

impl<S: std::io::Write> std::io::Write for BlockingB2fStream<S> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Identity split (spec §4.4): `base_mycall` is the B2F call (no SSID); the
/// SSID rode the AX.25 link address in the `connect`/`answer` call that
/// produced `stream`. `locator` is the operator's grid reduced to the
/// configured broadcast precision (pass `cms_locator(config)`, already exists).
fn native_packet_exchange<S: std::io::Read + std::io::Write + Send + 'static>(
    stream: S,
    ctx: PacketConnectCtx<'_>,
    mailbox: &Mailbox,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
) -> Result<(), BackendError> {
    // [R4-3][R1-C15][R5-3] Build the exchange config off `ctx` BEFORE destructuring
    // it below — `exchange_config_for_packet` takes `&ctx` (clones what it needs),
    // so this must run before `role`/`base_mycall`/`intent` are moved out of `ctx`.
    let exchange_config = exchange_config_for_packet(&ctx);
    let PacketConnectCtx {
        base_mycall,
        role,
        intent,
        ..
    } = ctx;
    // Split the owned stream into simultaneous read + write halves via a shared
    // Arc<Mutex> (the same pattern as telnet's shared-socket approach). The
    // exchange is strictly turn-based so the lock is never contended.
    use std::sync::{Arc, Mutex};
    trait RW: std::io::Read + std::io::Write + Send {}
    impl<T: std::io::Read + std::io::Write + Send> RW for T {}

    let shared: Arc<Mutex<Box<dyn RW>>> = Arc::new(Mutex::new(Box::new(stream)));

    struct ReadHalf(Arc<Mutex<Box<dyn RW>>>);
    struct WriteHalf(Arc<Mutex<Box<dyn RW>>>);
    impl std::io::Read for ReadHalf {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").read(b)
        }
    }
    impl std::io::Write for WriteHalf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ax25 lock").write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().expect("ax25 lock").flush()
        }
    }
    let mut reader = std::io::BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared.clone());

    // [R4-3][R1-C15][R5-3] intent-aware outbound drain (resolves the prior
    // TODO(tuxlink-u5hl follow-up)): mirrors the CMS/VARA/ARDOP dial paths'
    // skip-not-abort degrade — the safety gate for P2p/RadioOnly (§5.5) means
    // this legitimately returns an empty outbound for a P2P packet session
    // today; the exchange still runs (and still receives), it just proposes
    // nothing outbound until tuxlink-u5hl re-scopes the gate.
    let outbound = build_outbound_proposals(mailbox, intent, None, Some(base_mycall))
        .unwrap_or_else(|e| {
            eprintln!(
                "native_packet_exchange: outbound drain skipped ({e}); exchange continues with empty outbound"
            );
            Vec::new()
        });
    let outbound_log = outbound_log_items(&outbound);

    progress("AX.25 connected; negotiating messages…");
    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        role,
        &exchange_config,
        outbound,
        |proposals, _manifest| {
            Ok(proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect())
        },
        Some(wire_log),
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e:?}"),
        source: None,
    })?;

    // P1.4 (Codex post-impl review): file accepted messages FIRST, then surface
    // any rejection error. The prior ordering returned early on rejections, leaving
    // successfully-sent MIDs in the Outbox where they would be re-offered on the
    // next connection (duplicate send). Moving them to Sent is idempotent even
    // when `result.sent` is empty (all-rejected batch); the error still surfaces.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
    }
    emit_exchange_result_progress(&result, &outbound_log, progress);
    if !result.rejected.is_empty() {
        return Err(BackendError::MessageRejected(format!(
            "CMS rejected mid(s): {}",
            result.rejected.join(", ")
        )));
    }
    // `shared` drops here → stream drops → DISC fires (Ax25Stream::drop).
    Ok(())
}

/// Open the KISS link, connect (dial) or answer (listen), and run the exchange.
/// Per RADIO-1, the agent never runs this against a real KISS modem — tests
/// exercise `native_packet_exchange` with `FakeAx25Stream` only.
#[allow(clippy::too_many_arguments)]
fn native_packet_connect(
    config: &Config,
    mailbox: &Arc<Mailbox>,
    link: KissLinkConfig,
    resolved: ResolvedPacket,
    progress: &dyn Fn(&str),
    wire: &WireSink,
    abort_handle: &Mutex<Option<TcpStream>>,
    aborting: Arc<AtomicBool>,
    disconnecting: Arc<AtomicBool>,
    position: Option<Arc<crate::position::PositionArbiter>>,
    allowlist_override: Option<crate::winlink::listener::AllowedStations>,
) -> Result<(), BackendError> {
    let params = config.packet.params.clone().into_params();
    // tuxlink-uvi7: resolve the on-air locator via the shared PositionArbiter (live GPS
    // or manual grid, reduced to broadcast precision) — same path as native_connect
    // (telnet). The packet path previously used the static cms_locator(config), which
    // reads only config.identity.grid, so a GPS-derived grid never reached the B2F
    // greeting and it went out as an empty `()` (on-air K7YCA-8 2026-06-16).
    let locator = crate::position::effective_broadcast_locator(config, position.as_deref());
    let base = resolved.base_mycall.clone();
    // tuxlink-nfwv: trace every keyed/received AX.25 frame to the session log (`raw`
    // level → operator "Show Raw" / alpha-tester bug-report attachment). Reuses the
    // wire sink; `&**wire` still serves the B2F text wire_log below.
    let frame_log: Option<crate::winlink::ax25::FrameLogger> = Some(wire.clone());
    let wire_log: &dyn Fn(&str) = &**wire;

    // ── P6: managed Dire Wolf interception (RADIO-1) ──────────────────────────
    //
    // If the operator's link is the ManagedDireWolf variant, tuxlink spawns +
    // supervises its OWN Dire Wolf here, then REBINDS `link` to the loopback Tcp
    // KISS port Dire Wolf serves — so the rest of this function (the dial/listen +
    // exchange below) runs UNCHANGED against a normal Tcp link. Both dial and
    // listen flow through this function, so wiring here covers both.
    //
    // `_managed_guard` holds the live ManagedDireWolf for the WHOLE fn scope. Its
    // Drop runs the explicit 5s shutdown() (the RADIO-1 clean de-key) on EVERY
    // exit path — normal return after the exchange, a `?` early-return below, OR a
    // panic unwinding the stack — because Rust runs Drop for in-scope values on
    // unwind. This is the mechanism that guarantees the transmitter is de-keyed
    // and the process reaped however the session ends.
    let mut link = link;
    let _managed_guard;
    // Take OWNED copies of the managed-variant fields up front so reassigning
    // `link` below does not run afoul of a borrow held by the `if let` pattern.
    // (Only the managed arm clones; the common Tcp/Serial/Bluetooth links match
    // `None` here and fall straight through unchanged.)
    let managed_fields = match &link {
        KissLinkConfig::ManagedDireWolf { audio_device, ptt } => {
            Some((audio_device.clone(), ptt.clone()))
        }
        _ => None,
    };
    if let Some((audio_device, ptt)) = managed_fields {
        use crate::winlink::ax25::{
            pick_free_kiss_port, read_sys_snapshot, resolve_managed_device, DwLifecycleError,
            ManagedDireWolf, ManagedDireWolfCfg, ManagedDireWolfGuard,
        };

        progress("Starting managed Dire Wolf…");

        // 1. Resolve the persisted stable id against the LIVE system to its current
        //    plughw + card<N> index. None ⇒ the configured card is unplugged /
        //    renamed — surface a clear error, never spawn against the wrong card.
        let resolved_dev =
            resolve_managed_device(&audio_device, &read_sys_snapshot()).ok_or_else(|| {
                BackendError::TransportFailed {
                    reason: format!(
                        "configured sound card not found (id: {}). Plug it in, or pick a \
                         different audio device in packet settings.",
                        audio_device.value
                    ),
                    source: None,
                }
            })?;

        // 2. Pick a free loopback KISS port for both the conf KISSPORT and our dial.
        let kiss_port = pick_free_kiss_port().map_err(|e| BackendError::TransportFailed {
            reason: format!("could not allocate a localhost KISS port for Dire Wolf: {e}"),
            source: None,
        })?;

        // 3. Spawn + supervise Dire Wolf. Map the "not installed" case to an
        //    install-affordance message; every other lifecycle error to a named
        //    TransportFailed reason.
        let managed = ManagedDireWolf::spawn(ManagedDireWolfCfg {
            adevice: resolved_dev.alsa_plughw,
            card_index: resolved_dev.card_index,
            mycall: resolved.base_mycall.clone(),
            ptt,
            kiss_port,
        })
        .map_err(|e| match e {
            DwLifecycleError::DireWolfNotInstalled => BackendError::TransportFailed {
                reason: "Dire Wolf is not installed — install it, or switch to a \
                         bring-your-own KISS endpoint (TCP/serial/Bluetooth) in packet settings."
                    .to_string(),
                source: None,
            },
            other => BackendError::TransportFailed {
                reason: format!("managed Dire Wolf failed to start: {other}"),
                source: None,
            },
        })?;

        // 4. Rebind `link` to the loopback Tcp KISS endpoint Dire Wolf now serves;
        //    the rest of this fn proceeds as a normal Tcp connect.
        let (host, port) = managed.endpoint();
        link = KissLinkConfig::Tcp {
            host: host.to_string(),
            port,
        };

        // 5. Hold the guard for the whole fn scope (RADIO-1 clean de-key on exit).
        _managed_guard = ManagedDireWolfGuard(managed);
    }

    progress("Opening KISS link…");
    // Open the KISS link with an abort handle (tuxlink-9z2 pattern, mirroring
    // native_connect's register_socket). The TCP arm yields a try_clone'd TcpStream
    // the operator's abort() can `.shutdown()`; shutting it makes the link's read
    // return 0 (FIN), which recv_frame maps to ConnectionAborted, unwinding a blocked
    // answer()/connect() poll loop. The SERIAL arm has no socket, so it wraps the link
    // in AbortableByteLink keyed on the SAME `aborting` flag: abort() sets the flag and
    // the next serial read returns ConnectionAborted, unwinding the loop (tuxlink-nj1).
    let (bytelink, abort_socket) =
        crate::winlink::ax25::connect_link_with_abort(&link, aborting.clone()).map_err(|e| {
            BackendError::TransportFailed {
                reason: format!("KISS link: {e}"),
                source: None,
            }
        })?;
    if let Some(sock) = abort_socket {
        // Check `aborting` INSIDE the abort_handle lock (mirrors native_connect /
        // Codex #2): abort() sets `aborting` then locks to take the socket, so doing
        // the check + store under the same lock means whichever side acquires it
        // first, the socket still ends up shut down if an abort has already fired —
        // no TOCTOU window. If an abort landed during the (un-abortable) TCP-connect
        // window, shut the socket down now so answer()/connect() fails fast.
        if let Ok(mut slot) = abort_handle.lock() {
            if aborting.load(Ordering::SeqCst) {
                let _ = sock.shutdown(Shutdown::Both);
            } else {
                *slot = Some(sock);
            }
        }
    }

    // tuxlink-avu9: stack the graceful-disconnect wrapper OUTSIDE the abort wrapper.
    // A graceful Stop sets `disconnecting`, which unwinds the exchange read here but
    // leaves the write open, so the link's Drop teardown keys its DISC to the remote.
    let bytelink = crate::winlink::ax25::wrap_disconnectable(bytelink, disconnecting.clone());

    // Controller directive L: push KISS TNC params before connect/answer.
    // The straightforward approach is to call kiss_param inside the link before
    // handing it to ax25::connect/answer. However, `connect_link` returns
    // Box<dyn ByteLink> with no kiss_param accessor on the trait surface (P2's
    // ByteLink is bare Read+Write). Pushing params through the `Ax25Params`
    // passed to `connect`/`answer` is the P2 design — `connect` calls
    // `kiss_param` internally on the link for txdelay/persistence/slot_time
    // (per datalink.rs connect implementation). So no separate param-push is
    // needed here; the P2 `connect`/`answer` call owns it. Follow-up filed as
    // bd issue if P2 does NOT push params in answer() (see Task 6 commit body).

    match resolved.dial {
        Some((target, digis)) => {
            let intent = resolved.intent;
            progress(&format!("Connecting to {}…", target.call));

            // ─── Peer-observation recording (Task 15) ──────────────────────
            // Arm the drop-guard BEFORE the AX.25 connect (`connect_with_logger`
            // — the actual CONNECT transmission), gated on P2P intent [spec §3]:
            // a link-level connect failure (below) still records `DialAttempted`
            // → Fail on the guard's drop [R3-11]. CMS/gateway/RadioOnly/
            // PostOffice dials resolve no sink and never construct a guard.
            // Mirrors the VARA (`dial_observation_sink`) / ARDOP
            // (`ardop_dial_observation_sink`) dial gates (Tasks 13-14).
            let obs_guard = if intent == SessionIntent::P2p {
                crate::contacts::observation::observation_sink().map(|s| {
                    crate::contacts::observation::ObservationGuard::new(
                        s,
                        crate::contacts::observation::PeerObservation {
                            path: crate::contacts::observation::ObservedPath::Rf {
                                transport: crate::contacts::reachability::ChannelTransport::Packet,
                                via: digis
                                    .iter()
                                    .map(crate::winlink::ax25::datalink::fmt_addr)
                                    .collect(),
                                freq_hz: None,
                                bandwidth: None,
                            },
                            direction: crate::contacts::reachability::Direction::Outgoing,
                            presented_target: target.call.clone(),
                            phase: crate::contacts::observation::ObservationPhase::DialAttempted,
                        },
                    )
                })
            } else {
                None
            };

            let stream = crate::winlink::ax25::connect_with_logger(
                bytelink,
                resolved.link_mycall,
                target.clone(),
                &digis,
                &params,
                frame_log.clone(),
            )
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("AX.25 connect: {e}"),
                source: None,
            })?;
            // `obs_guard` (if armed) fired above on the `?` early-return, at
            // `DialAttempted` → Fail [R3-11]. Past this point the AX.25 link is
            // up — advance to `Connected`.
            if let Some(g) = &obs_guard {
                g.set_phase(crate::contacts::observation::ObservationPhase::Connected);
            }
            // P1.3 (Codex post-impl review): read_password is deferred until AFTER
            // the KISS link is established. The prior placement (before connect_link)
            // caused the OS-keyring migration to run even when the link failed — e.g.
            // when unit tests intentionally use a closed loopback port, the keyring
            // write still fired. Deferring until link-up means a failed connect_arq
            // never touches the operator's keyring, and Listen arming (password: None)
            // never triggers it at all. Option (a) per the Codex review.
            let password = crate::winlink::credentials::read_password(&base)
                .ok()
                .filter(|p| !p.is_empty());
            let result = native_packet_exchange(
                BlockingB2fStream(stream),
                PacketConnectCtx {
                    base_mycall: &base,
                    targetcall: &target.call,
                    password,
                    role: ExchangeRole::Dial,
                    locator: &locator,
                    intent,
                },
                mailbox,
                progress,
                wire_log,
            );
            if let Some(g) = &obs_guard {
                g.set_phase(match &result {
                    Ok(()) => crate::contacts::observation::ObservationPhase::B2fOk,
                    Err(_) => crate::contacts::observation::ObservationPhase::B2fFail,
                });
            }
            result
        }
        None => {
            // ── Listener-arms gate (tuxlink-inde) — armed BEFORE answer()
            //
            // Codex review 2026-06-03 [P2 — arm-time]: the original code
            // created `arms` AFTER `ax25::answer()` returned, so the TTL
            // gate was effectively a no-op (the arms record was always
            // freshly-minted at peer-receipt time, never compared against
            // the operator's true arm moment). The TTL check now meaningfully
            // expires the listener if a SABM arrives more than DEFAULT_TTL
            // after the operator armed: a peer that lands past the consent
            // window gets RejectExpired rather than silent accept.
            //
            // Reject path: drop the stream. Ax25Stream::drop fires DISC because
            // the link is established (the UA we just sent armed Drop teardown
            // via tuxlink-2y4). Reject events append to the shared forensics
            // log alongside the arm record.
            //
            // The full architecture (multi-peer continuous-armed listener with
            // shared arms record across multiple SABMs in one armed window)
            // is the follow-up; current model is one-arm one-answer cycle.
            use crate::winlink::listener::packet_gate::{
                gate_inbound_peer_now, listener_forensics_log_path, peer_id_from_ax25,
                reject_reason, ListenerRejectEvent,
            };
            use crate::winlink::listener::{
                packet_gate, AllowedStations, ListenerArmsRecord, ListenerDecision, TransportKind,
            };

            // Codex review 2026-06-03 [P2 — load-error visibility]: the
            // previous code silently substituted AllowedStations::default()
            // (allow_all=FALSE, empty list) on a corrupt-or-unreadable
            // allowlist file. The operator saw a normal "allowlist" reject
            // and couldn't tell whether the gate was working as configured
            // OR the allowlist had been wiped. We now (a) surface the load
            // error verbatim via progress(), and (b) use a distinct
            // "allowlist-load-error" reject reason so the forensics log +
            // session log clearly distinguish "configured allowlist denied
            // this peer" from "couldn't load the allowlist; failing closed."
            let mut load_failed_reason: Option<String> = None;
            let allowed = if let Some(injected) = allowlist_override.clone() {
                // Test injection (tuxlink-inde): bypasses the disk-file lookup.
                // Production never sets this; `bootstrap`/UI relies on the file.
                injected
            } else {
                match AllowedStations::load_from(&packet_gate::packet_allowed_stations_path()) {
                    Ok(a) => a,
                    Err(e) => {
                        let reason_str = format!("{e}");
                        progress(&format!(
                            "Packet allowlist load failed: {reason_str}. Failing closed (reject all inbound until repaired)."
                        ));
                        load_failed_reason = Some(reason_str);
                        // Codex review 2026-06-03 [P1] (tuxlink-7vea): after the
                        // foundation default flip to allow_all=TRUE,
                        // AllowedStations::default() now accepts everyone — so
                        // a corrupt allowlist file would silently widen the
                        // gate to fail-OPEN. Explicit restrict-mode here keeps
                        // the load-error path fail-CLOSED as the progress
                        // message claims.
                        AllowedStations::new().with_allow_all(false)
                    }
                }
            };
            let arms = ListenerArmsRecord::arm_default(TransportKind::Packet);

            progress("Listening for an inbound peer…");
            let (peer, stream) = crate::winlink::ax25::answer_with_logger(
                bytelink,
                resolved.link_mycall,
                &params,
                frame_log.clone(),
            )
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("AX.25 answer: {e}"),
                source: None,
            })?;
            progress(&format!("Answered {}.", peer.call));

            let peer_id = peer_id_from_ax25(peer.clone());
            let decision = gate_inbound_peer_now(&peer_id, &allowed, &arms);

            if decision != ListenerDecision::Accept {
                // If the gate rejected with "allowlist" AND we know the load
                // failed, upgrade the reject reason to a distinct
                // "allowlist-load-error" so the operator can distinguish.
                let reason: &str = match (&decision, &load_failed_reason) {
                    (ListenerDecision::RejectAllowlist, Some(_)) => "allowlist-load-error",
                    _ => reject_reason(&decision).unwrap_or("unknown"),
                };
                let log_path = listener_forensics_log_path();
                let event = ListenerRejectEvent::new(TransportKind::Packet, reason, &peer_id);
                let _ = event.append_to_log(&log_path);
                // R3-F5: count the rejected inbound on the quarantine limiter's
                // failed path (no roster record) — the spoofing-loop counter's
                // only real-burst source for packet.
                crate::contacts::observation::record_inbound_reject(
                    crate::contacts::reachability::ChannelTransport::Packet,
                );

                let msg = format!(
                    "Rejected inbound from {} (reason: {}). Dropping link.",
                    peer.call, reason,
                );
                progress(&msg);

                // Drop the stream → Ax25Stream::drop sends DISC + best-effort
                // awaits UA/DM.
                drop(stream);
                return Err(BackendError::AuthFailed {
                    reason: format!(
                        "listener gate rejected inbound peer {} ({})",
                        peer.call, reason
                    ),
                });
            }

            // ─── Peer-observation recording (Task 15) ──────────────────────
            // Arm the drop-guard for the inbound answer at `B2fStarted`, ABOVE
            // the exchange call — mirrors the ARDOP/VARA answer-site placement
            // standard (arm above local mailbox/tempdir resolution; the packet
            // answer path takes its mailbox as a caller-supplied `Arc`, so a
            // peer that already answered records even if the exchange call
            // itself fails). No intent check needed here:
            // `packet_listen_transport_from_config` pins every Listen role to
            // `SessionIntent::P2p` (Task 12), and a rejected inbound (allowlist/
            // expired) already returned above — never reaches this point, so
            // never records, by construction. The guard fires on drop with its
            // last phase: `Accepted` on a clean exchange, `B2fFail` on failure,
            // and — if a wedge/panic-unwind unwinds past here — the
            // `B2fStarted` it was armed at, which classifies as Fail [R3-11].
            let obs_guard = crate::contacts::observation::observation_sink().map(|s| {
                crate::contacts::observation::ObservationGuard::new(
                    s,
                    crate::contacts::observation::PeerObservation {
                        path: crate::contacts::observation::ObservedPath::Rf {
                            transport: crate::contacts::reachability::ChannelTransport::Packet,
                            via: vec![],
                            freq_hz: None,
                            bandwidth: None,
                        },
                        direction: crate::contacts::reachability::Direction::Incoming,
                        presented_target: peer.call.clone(),
                        phase: crate::contacts::observation::ObservationPhase::B2fStarted,
                    },
                )
            });

            // Listen (Answer role) does not need a password — peers do not challenge.
            // password: None is intentional; no read_password call here.
            let result = native_packet_exchange(
                BlockingB2fStream(stream),
                PacketConnectCtx {
                    base_mycall: &base,
                    targetcall: &peer.call,
                    password: None,
                    role: ExchangeRole::Answer,
                    locator: &locator,
                    intent: resolved.intent,
                },
                mailbox,
                progress,
                wire_log,
            );
            if let Some(g) = &obs_guard {
                g.set_phase(match &result {
                    Ok(()) => crate::contacts::observation::ObservationPhase::Accepted,
                    Err(_) => crate::contacts::observation::ObservationPhase::B2fFail,
                });
            }
            result
        }
    }
}

/// The `BackendStatus` a packet connection STARTS in, by role (tuxlink-orj).
/// `Listen` is armed-but-idle → `Listening`; `DialTo` is an active dial →
/// `Connecting`. Pure (no I/O) so the role→status decision is unit-tested
/// without a KISS link. Set before `spawn_blocking`, it persists for the whole
/// armed wait (the ribbon polls `status()`), so an armed Listen reads honestly
/// as "Listening · Packet 1200" instead of a misleading "Connecting".
fn initial_packet_status(role: &PacketRole, ssid: u8) -> BackendStatus {
    let transport = format!("Packet-{ssid}");
    match role {
        PacketRole::Listen => BackendStatus::Listening { transport },
        PacketRole::DialTo { .. } => BackendStatus::Connecting { transport },
    }
}

/// Resolve the CMS host to dial (tuxlink-3o0). Precedence: the `TUXLINK_CMS_HOST`
/// env var wins if set (the dev escape hatch, mirroring `bin/native_cms_probe`);
/// otherwise the operator's configured `config.connect.host` is used (set via the
/// inline SettingsPanel's `config_set_connect`). This replaces the former
/// hardcoded `CMS_HOST` const fallback — the default now lives in
/// `config::default_cms_host` and reaches here through the persisted config.
fn resolve_cms_host(config: &Config) -> String {
    std::env::var("TUXLINK_CMS_HOST").unwrap_or_else(|_| config.connect.host.clone())
}

/// Resolve the CMS `(port, transport)` from the configured `mode` plus optional
/// dev overrides (tuxlink-gqo). `TUXLINK_CMS_PLAINTEXT` forces plaintext telnet —
/// the dev escape hatch for hosts that expose no TLS (the dev default cms-z has no
/// 8773 TLS listener, while production `server.winlink.org` does); `TUXLINK_CMS_PORT`
/// overrides the port. With no overrides the configured transport stands, so the
/// persisted/production CmsSsl default keeps its 8773 TLS endpoint. Mirrors the
/// `bin/native_cms_probe` env contract so the app and the probe agree.
fn resolve_cms_endpoint(
    mode: CmsTransport,
    plaintext_override: bool,
    port_override: Option<u16>,
) -> (u16, telnet::Transport) {
    let transport = if plaintext_override {
        telnet::Transport::Plaintext
    } else {
        match mode {
            CmsTransport::CmsSsl => telnet::Transport::Tls,
            CmsTransport::Telnet => telnet::Transport::Plaintext,
        }
    };
    let default_port = match transport {
        telnet::Transport::Tls => 8773,
        telnet::Transport::Plaintext => 8772,
    };
    (port_override.unwrap_or(default_port), transport)
}

/// Map a raw connect outcome to the caller-facing result (tuxlink-9z2): an error
/// that follows an operator abort becomes `Cancelled`; a success stands (the
/// connect completed before the abort landed); a non-aborted error stands.
fn abort_aware_outcome(
    outcome: Result<(), BackendError>,
    aborted: bool,
) -> Result<(), BackendError> {
    match outcome {
        Err(_) if aborted => Err(BackendError::Cancelled),
        other => other,
    }
}

/// The static (config-grid-only) locator, reduced to broadcast precision.
///
/// tuxlink-uvi7: production now resolves the on-air locator via
/// `crate::position::effective_broadcast_locator` (GPS-arbiter aware) on BOTH the
/// telnet and packet paths. This static helper is retained as the no-arbiter
/// reference the `resolve_locator` / `cms_locator_*` tests assert against, so it
/// has no production caller — `allow(dead_code)` rather than `cfg(test)` to keep
/// the shared `broadcast_grid` import live in non-test builds.
#[allow(dead_code)]
fn cms_locator(config: &Config) -> String {
    config
        .identity
        .grid
        .as_deref()
        .map(|g| broadcast_grid(g, config.privacy.position_precision))
        .unwrap_or_default()
}

/// The on-air locator: delegates to [`crate::position::effective_broadcast_locator`],
/// which is the single source of truth for the on-air grid (honoring both precision
/// AND the `gps_state` privacy control). This thin wrapper exists only for callers
/// that already hold a `Config` reference and an optional arbiter in the
/// winlink_backend context.
///
/// GPS-derived positions go on air ONLY when `gps_state == BroadcastAtPrecision`;
/// under `Off` or `LocalUiOnly` the on-air locator falls back to the stored
/// config grid. A hand-set Manual grid broadcasts regardless of `gps_state`.
///
/// Currently only consumed by the in-module tests; production `native_connect`
/// calls `effective_broadcast_locator` directly. Scoped to `cfg(test)` so non-test
/// builds don't flag it as dead code. If a non-test caller appears later, drop
/// the gate.
#[cfg(test)]
fn resolve_locator(config: &Config, position: Option<&crate::position::PositionArbiter>) -> String {
    crate::position::effective_broadcast_locator(config, position)
}

/// The inbound-proposal decider `native_connect` hands to the telnet exchange:
/// either the accept-all closure or the operator-selecting decider (tuxlink-bsiy).
/// Boxed because the two arms have distinct concrete types; it stays on the
/// blocking exchange thread, so no `Send`/`Sync` bound is required.
type InboundDecider = Box<
    dyn Fn(
        &[crate::winlink::proposal::Proposal],
        &[crate::winlink::proposal::PendingMessage],
    ) -> Result<Vec<Answer>, session::ExchangeError>,
>;

/// Run one CMS exchange (blocking): build the outbox into proposals, connect over
/// the chosen transport, accept all offered messages, then file what arrived into
/// the inbox and move what was sent into the sent folder.
//
// native_connect coordinates a multi-faceted connect flow (config + mailbox +
// transport + progress/wire-log callbacks + abort plumbing + position arbiter);
// refactoring to fewer args would require introducing a builder/options struct
// that's not justified for v0.2. Tracked separately if it ever becomes load-bearing.
#[allow(clippy::too_many_arguments)]
fn native_connect(
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    mode: CmsTransport,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    mailbox_change: &dyn Fn(),
    abort_handle: &Mutex<Option<TcpStream>>,
    aborting: Arc<AtomicBool>,
    position: Option<&crate::position::PositionArbiter>,
    selection: Option<CmsSelectionContext>,
) -> Result<(), BackendError> {
    // tuxlink-0063 (Phase 3): the on-air station ID is the session's full
    // callsign — the Part 97 principal — not `config.identity.active_full`.
    // Threading `&SessionIdentity` makes on-air impersonation a compile error:
    // the dial path can no longer read a callsign out of config. The password
    // lookup keyed on this callsign follows the session too, which is correct.
    let callsign = session_id.mycall().as_str().to_uppercase();
    // tuxlink-686 / Codex P1-A: resolve the on-air locator via the single shared
    // helper that honors BOTH precision (tuxlink-882) AND the gps_state privacy
    // control. GPS grids go on air only when gps_state == BroadcastAtPrecision;
    // Off/LocalUiOnly fall back to the config grid. Manual broadcasts regardless.
    let locator = crate::position::effective_broadcast_locator(config, position);

    // Dev overrides (tuxlink-gqo) mirror `bin/native_cms_probe`: TUXLINK_CMS_PLAINTEXT
    // forces plaintext (cms-z exposes no 8773 TLS), TUXLINK_CMS_PORT overrides the
    // port. Absent both, the configured transport stands (production = CmsSsl/8773).
    let plaintext_override = std::env::var("TUXLINK_CMS_PLAINTEXT").is_ok();
    let port_override = std::env::var("TUXLINK_CMS_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok());
    let (port, transport) = resolve_cms_endpoint(mode, plaintext_override, port_override);

    // Turn each queued outbox message into a proposal + compressed body. CMS
    // dial drains the whole Outbox (intent=Cms, selected=None) — via the shared
    // helper, which gains skip-not-abort on per-message read failures
    // (tuxlink-6c9y consolidates this third drain loop into the helper).
    let outbound = build_outbound_proposals(
        mailbox,
        SessionIntent::Cms,
        None,
        Some(session_id.mycall().as_str()),
    )?;
    let outbound_log = outbound_log_items(&outbound);

    // P1.3 (Codex post-impl review): defer read_password until after all config
    // validation and outbox-building steps have succeeded. Placing it here — just
    // before ExchangeConfig is built — ensures the OS-keyring migration only runs
    // when we are actually about to open a socket. Tests that fail in the preceding
    // steps (no callsign, mailbox errors) never touch the keyring. Option (a) per
    // the Codex review; the telnet path builds ExchangeConfig inline so "after link
    // open" translates to "after outbox build but before connect_and_exchange".
    let password = crate::winlink::credentials::read_password(&callsign)
        .ok()
        .filter(|p| !p.is_empty());

    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: telnet::CMS_TARGET_CALL.to_string(),
        locator,
        password,
        intent: SessionIntent::Cms,
    };

    // The CMS host comes from the operator's configured `config.connect.host`
    // (tuxlink-3o0, set in the inline SettingsPanel); `TUXLINK_CMS_HOST` still
    // overrides it as a dev escape hatch. See `resolve_cms_host`.
    let host = resolve_cms_host(config);

    // Hand each freshly-connected socket to the abort handle (tuxlink-9z2) so an
    // operator abort can `.shutdown()` it. A clone failure just leaves abort a
    // no-op for this attempt — connect proceeds normally. If an abort already
    // landed during the (un-abortable) TCP-connect window, shut this socket down
    // immediately so the connect fails fast instead of running to completion in
    // the background.
    let register_socket = |sock: &TcpStream| {
        if let Ok(clone) = sock.try_clone() {
            // Check `aborting` INSIDE the abort_handle lock (Codex #2): abort() sets
            // `aborting` then locks to take the socket, so doing the check + store
            // under the same lock means whichever side acquires it first, the socket
            // still ends up shut down if an abort has fired — no TOCTOU window.
            if let Ok(mut slot) = abort_handle.lock() {
                if aborting.load(Ordering::SeqCst) {
                    let _ = clone.shutdown(Shutdown::Both);
                } else {
                    *slot = Some(clone);
                }
            }
        }
    };

    // Choose the inbound-proposal decider by the PRESENCE of a selection context.
    // `cms_connect` builds the context iff the operator's FRESH on-disk
    // `review_inbound_before_download` preference is on, so a context here means
    // "prompt the operator" and no context means accept-all (preference off, or a
    // non-prompting caller: wizard probe, packet, tests).
    //
    // tuxlink-bsiy connect-path staleness fix: gating on the context — NOT on this
    // `config` snapshot's `review_inbound_before_download` — is deliberate. `config`
    // is the backend's in-memory `live_config`, refreshed only by `set_config`,
    // which `config_set_review_inbound` skips when the backend is not yet installed.
    // A preference toggled before the backend comes up therefore never reaches
    // `live_config`, so reading the flag here silently dropped the prompt. The
    // context (built from the fresh disk read) is the authoritative signal.
    //
    // The boxed decider stays on this blocking thread, so it does NOT need
    // Send/Sync; it satisfies `connect_and_exchange`'s `F: Fn(...)` bound.
    use crate::winlink::b2f_events::B2fEvent;
    use crate::winlink::inbound_selection::PendingProposalDto;
    use crate::winlink::proposal::Proposal;
    let decide: InboundDecider = match selection {
        Some(ctx) => {
            let CmsSelectionContext {
                sink,
                attempt_id,
                registry,
            } = ctx;
            let emit = move |request_id: u64, dtos: &[PendingProposalDto]| {
                sink.push(B2fEvent::InboundProposalsOffered {
                    request_id,
                    proposals: dtos.to_vec(),
                    attempt_id,
                });
            };
            Box::new(crate::winlink::inbound_selection::build_selecting_decider(
                registry,
                attempt_id,
                emit,
                aborting.clone(),
            ))
        }
        None => Box::new(
            |proposals: &[Proposal], _manifest: &[crate::winlink::proposal::PendingMessage]| {
                Ok(proposals
                    .iter()
                    .map(|_| Answer::Accept { resume_offset: 0 })
                    .collect())
            },
        ),
    };

    let result = telnet::connect_and_exchange(
        &host,
        port,
        transport,
        &exchange_config,
        outbound,
        progress,
        wire_log,
        &register_socket,
        decide,
    )
    .map_err(|e| {
        // Task 12 (tuxlink-7do4): intercept *** payload variants so
        // cms_connect's Err arm has a structured BackendError::RemoteError
        // to classify via auth_taxonomy::classify. All other TelnetError
        // variants (TCP/TLS failures, other exchange errors) → TransportFailed
        // as before.
        use crate::winlink::handshake::HandshakeError;
        use session::ExchangeError;
        use telnet::TelnetError;
        match e {
            TelnetError::Exchange(ExchangeError::RemoteError(payload)) => {
                BackendError::RemoteError(payload)
            }
            TelnetError::Exchange(ExchangeError::Handshake(HandshakeError::RemoteError(
                payload,
            ))) => BackendError::RemoteError(payload),
            // tuxlink-bsiy: the selecting decider returns Cancelled when the
            // operator aborts a pending selection prompt. Map it explicitly so the
            // cancel path is structural here rather than relying solely on
            // `abort_aware_outcome`'s flag check at the caller.
            TelnetError::Exchange(ExchangeError::Cancelled) => BackendError::Cancelled,
            other => BackendError::TransportFailed {
                reason: format!("{other:?}"),
                source: None,
            },
        }
    })?;

    // P1.4 (Codex post-impl review): file accepted messages FIRST, then surface
    // any rejection error. The prior ordering returned early on rejections, leaving
    // successfully-sent MIDs in the Outbox where they would be re-offered on the
    // next connection (duplicate send). Moving them to Sent is idempotent even
    // when `result.sent` is empty (all-rejected batch); the error still surfaces.
    file_exchange_result(mailbox, &result, SessionIntent::Cms, mailbox_change)?;
    emit_exchange_result_progress(&result, &outbound_log, progress);
    if !result.rejected.is_empty() {
        return Err(BackendError::MessageRejected(format!(
            "CMS rejected mid(s): {}",
            result.rejected.join(", ")
        )));
    }
    Ok(())
}

/// Emit operator-facing movement details after a B2F exchange completes.
///
/// This is intentionally separate from `wire_log`: raw B2F traffic already
/// exists for detailed debugging, while these lines answer the practical
/// operator question "what mail moved?" in the plain session log.
pub(crate) fn emit_exchange_result_progress(
    result: &session::ExchangeResult,
    outbound: &[(String, String)],
    progress: &dyn Fn(&str),
) {
    if result.received.is_empty()
        && result.sent.is_empty()
        && result.rejected.is_empty()
        && result.deferred.is_empty()
    {
        progress("No messages exchanged.");
        return;
    }

    for message in &result.received {
        let subject = message_log_header(message, "Subject", "(no subject)", 80);
        let from = message_log_header(message, "From", "(unknown sender)", 48);
        let mid = message_log_header(message, "Mid", "(no MID)", 64);
        progress(&format!(
            "Received \"{subject}\" from {from} (MID {mid}) -> Inbox."
        ));
    }

    for mid in &result.sent {
        let title = outbound_title_for_mid(outbound, mid);
        progress(&format!("Sent \"{title}\" (MID {mid}) -> Sent."));
    }

    for mid in &result.rejected {
        let title = outbound_title_for_mid(outbound, mid);
        progress(&format!("Remote rejected \"{title}\" (MID {mid})."));
    }

    for mid in &result.deferred {
        let title = outbound_title_for_mid(outbound, mid);
        progress(&format!("Remote deferred \"{title}\" (MID {mid})."));
    }
}

pub(crate) fn outbound_log_items(outbound: &[session::OutboundMessage]) -> Vec<(String, String)> {
    outbound
        .iter()
        .map(|message| (message.proposal.mid.clone(), message.title.clone()))
        .collect()
}

fn outbound_title_for_mid(outbound: &[(String, String)], mid: &str) -> String {
    outbound
        .iter()
        .find(|(message_mid, _)| message_mid == mid)
        .map(|(_, title)| compact_log_field(title, 80))
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "(unknown subject)".to_string())
}

fn message_log_header(message: &Message, name: &str, fallback: &str, max_chars: usize) -> String {
    let value = message.header(name).unwrap_or(fallback);
    let compact = compact_log_field(value, max_chars);
    if compact.is_empty() {
        fallback.to_string()
    } else {
        compact
    }
}

fn compact_log_field(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    let mut used = 0usize;
    let mut truncated = false;

    for ch in value.chars() {
        if used >= max_chars {
            truncated = true;
            break;
        }
        let normalized = if ch.is_control() || ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if normalized == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        out.push(normalized);
        used += 1;
    }

    let mut compact = out.trim().to_string();
    if truncated {
        compact.push_str("...");
    }
    compact
}

/// Persist a completed exchange into the mailbox and emit one change
/// notification if at least one message was received or sent.
///
/// `intent` determines whether inbound messages are stamped with
/// `X-Tuxlink-Received-Session`. Only `SessionIntent::PostOffice` applies the
/// marker (value `"post-office"`); all other intents store messages byte-identical.
pub(crate) fn file_exchange_result(
    mailbox: &Mailbox,
    result: &session::ExchangeResult,
    intent: SessionIntent,
    mailbox_change: &dyn Fn(),
) -> Result<(), BackendError> {
    let mut changed = false;
    for message in &result.received {
        if intent == SessionIntent::PostOffice {
            let mut m = message.clone();
            m.set_header(RECEIVED_SESSION_HEADER, RECEIVED_SESSION_POST_OFFICE);
            mailbox.store(MailboxFolder::Inbox, &m.to_bytes())?;
        } else {
            mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
        }
        changed = true;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
        changed = true;
    }
    if changed {
        mailbox_change();
    }
    Ok(())
}

/// Run a B2F mail exchange over an already-`connect_arq`'d ARDOP transport
/// (tuxlink-ytg). The transport was spawned + ARQ-connected by
/// `modem_ardop_connect`; this function consumes it for the duration of the
/// exchange and returns it so the caller can `disconnect()` + drop it under its
/// own state machine (the Tauri command in `modem_commands.rs` resets
/// `ModemSession` after this returns).
///
/// Mirrors `native_connect`'s mailbox plumbing: builds outbound from
/// `mailbox`'s Outbox folder, runs the exchange in `Dial` role (slave/IRS —
/// the operator's send/receive against a CMS or peer), files received messages
/// into Inbox, moves sent ones from Outbox to Sent.
///
/// The transport surface is `Box<dyn ModemTransport>` so any future modem
/// (Dire Wolf, sonde) that implements the same trait flows through this
/// path unchanged.
///
/// # RADIO-1
///
/// The caller MUST have consumed a per-invocation consent token before
/// invoking this function. This function does NO consent gating of its own —
/// the gate is upstream at the Tauri command boundary, where it can refuse
/// I/O / state mutation pre-gate.
/// Dial-path outbound drain (tuxlink-9efs). ONLY the safety-gate rejection
/// ([`BackendError::MessageRejected`]) degrades to an empty outbound list —
/// failing closed there would orphan the already-up ARQ/VARA session for an
/// intended skip, and an empty batch proposes nothing so no off-spec routing
/// flag rides out. EVERY OTHER error (corrupt / unreadable mailbox, I/O, etc.)
/// propagates so a data error fail-closes instead of masquerading as a
/// successful empty send/receive. Mirrors the narrow handling in
/// [`crate::ui_commands::telnet_p2p_connect`].
///
/// NOTE: the listen/answer paths ([`run_ardop_b2f_answer`] /
/// [`run_vara_b2f_answer`]) intentionally degrade ALL errors and do NOT use
/// this helper — narrowing them is out of scope for tuxlink-9efs.
fn dial_outbound_or_propagate(
    result: Result<Vec<session::OutboundMessage>, BackendError>,
    ctx: &str,
) -> Result<Vec<session::OutboundMessage>, BackendError> {
    match result {
        Ok(v) => Ok(v),
        Err(BackendError::MessageRejected(reason)) => {
            eprintln!(
                "{ctx}: outbound drain skipped ({reason}); dial proceeds with empty outbound"
            );
            Ok(Vec::new())
        }
        Err(e) => Err(e),
    }
}

#[allow(clippy::too_many_arguments)] // config + session_id together are one logical "session context"
pub fn run_ardop_b2f_exchange(
    transport: &mut dyn crate::winlink::modem::ModemTransport,
    target: &str,
    intent: SessionIntent,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    use crate::winlink::modem::ardop::b2f;

    // tuxlink-0063 (Phase 3, Task 3.6): the on-air station ID is the session's
    // full callsign — the Part 97 principal — not `config.identity.active_full`.
    // Threading `&SessionIdentity` makes on-air impersonation a compile error.
    let callsign = session_id.mycall().as_str().to_uppercase();
    let locator = crate::position::effective_broadcast_locator(config, position);
    // The ARDOP B2F path can dial either a CMS gateway (intent=Cms) or a peer
    // station (intent=P2p — added in tuxlink-9ls2). Only the gateway path may
    // receive a `;PQ` challenge; peers never challenge per the FBB master/
    // slave split. So the password fetch is gated on intent: for non-CMS
    // dials we skip the keyring read entirely — both a hygiene win (no
    // unnecessary keyring traffic) and a defensive guarantee that a stale
    // CMS secret cannot leak into a peer handshake.
    // The CMS path reads the secure-login password from the canonical `tuxlink`
    // keyring service via credentials::read_password (same source the telnet path
    // uses). tuxlink-kc3q: previously read the legacy `tuxlink-pat` service directly.
    let password = if intent == SessionIntent::Cms {
        crate::winlink::credentials::read_password(&callsign)
            .ok()
            .filter(|p| !p.is_empty())
    } else {
        None
    };

    // Turn each queued outbox message into a proposal + compressed body.
    //
    // Intent-filtered drain (tuxlink-u5hl Codex Round 5 P1 #3 +
    // Phase 3-4 RE-REVIEW P2; narrowed tuxlink-9efs): for non-CMS intents the
    // safety gate fires and returns `MessageRejected` — degrade THAT to an empty
    // outbound rather than failing the dial. The ARQ link is already up by the
    // time we get here, so failing closed on the intended skip would only orphan
    // the operator's session, and an empty batch proposes nothing so no off-spec
    // routing flag rides out. Any OTHER error (corrupt / unreadable mailbox,
    // etc.) fail-closes via `?` so a data error cannot masquerade as a
    // successful empty send. NOT symmetric with `run_ardop_b2f_answer`, which
    // still degrades every error by design.
    let outbound = dial_outbound_or_propagate(
        build_outbound_proposals(mailbox, intent, None, Some(session_id.mycall().as_str())),
        "run_ardop_b2f_exchange",
    )?;
    let outbound_log = outbound_log_items(&outbound);

    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: target.to_string(),
        locator,
        password,
        intent,
    };

    let result = b2f::run_b2f_exchange(
        transport,
        ExchangeRole::Dial,
        &exchange_config,
        outbound,
        |proposals, _manifest| {
            Ok(proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect())
        },
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e}"),
        source: None,
    })?;

    // File received messages into the inbox; move delivered ones to sent.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
    }
    if let Some(progress) = progress {
        emit_exchange_result_progress(&result, &outbound_log, progress);
    }
    Ok(())
}

/// Run the ARDOP B2F exchange as the **answerer** for the ARDOP listener
/// (tuxlink-61yg). Mirror of `run_ardop_b2f_exchange` but with
/// `ExchangeRole::Answer` + `SessionIntent::P2p`, parameterised on the
/// connected peer's callsign.
///
/// Loads operator Outbox BEFORE the exchange so pending mail rides the
/// inbound session out to the peer. After the exchange completes, persists
/// `result.received` to Inbox + moves `result.sent` MIDs from Outbox to
/// Sent. Same Inbox-FIRST ordering as the dialer path's Codex P1.4 fix.
#[allow(clippy::too_many_arguments)] // config + session_id together are one logical "session context"
pub fn run_ardop_b2f_answer(
    transport: &mut dyn crate::winlink::modem::ModemTransport,
    peer_callsign: &str,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    use crate::winlink::modem::ardop::b2f;
    // tuxlink-0063 (Phase 3, Task 3.6): the on-air station ID is the session's
    // full callsign captured AT LISTENER-ARM TIME — not config.identity.active_full.
    let callsign = session_id.mycall().as_str().to_uppercase();
    let locator = crate::position::effective_broadcast_locator(config, position);

    // Intent-filtered drain (tuxlink-u5hl). Listener answerer: catch the
    // safety-gate error and degrade to empty outbound rather than failing
    // the inbound session — the peer is already on the link and inbound
    // mail filing still works without us shipping outbound. Symmetric with
    // the telnet_listen answerer (`progress("Outbox read failed …")`).
    let outbound = build_outbound_proposals(mailbox, SessionIntent::P2p, None, Some(session_id.mycall().as_str())).unwrap_or_else(|e| {
        eprintln!(
            "run_ardop_b2f_answer: outbound drain skipped ({e}); inbound continues with empty outbound"
        );
        Vec::new()
    });
    let outbound_log = outbound_log_items(&outbound);
    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: peer_callsign.to_string(),
        locator,
        password: None,
        intent: SessionIntent::P2p,
    };
    let result = b2f::run_b2f_exchange(
        transport,
        ExchangeRole::Answer,
        &exchange_config,
        outbound,
        |proposals, _manifest| {
            Ok(proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect())
        },
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e}"),
        source: None,
    })?;

    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
    }
    if let Some(progress) = progress {
        emit_exchange_result_progress(&result, &outbound_log, progress);
    }
    Ok(())
}

/// Run the VARA B2F exchange as the **answerer** for the VARA listener
/// (tuxlink-9ls2). Mirror of `run_ardop_b2f_answer` but adapted for VARA:
/// the VARA data socket carries raw bytes (no FEC/framing layer above TCP),
/// so we drive `session::run_exchange_with_role` directly on
/// `transport.data_stream()` rather than going through an intermediate
/// B2F-over-X wrapper module.
///
/// VARA's host protocol exposes the connected-mode payload as a plain
/// `TcpStream`; we `try_clone()` the writer half and wrap the reader half
/// in `BufReader` so `run_exchange_with_role` (which takes separate
/// `R: BufRead` and `W: Write` arguments) gets the duplex split it needs.
/// Unlike ARDOP's `b2f::run_b2f_exchange` we don't need an `Arc<Mutex>`
/// shared-handle pattern because VARA's data socket IS an OS-level TCP
/// stream we can clone — the kernel arbitrates the duplex halves.
///
/// Loads operator Outbox BEFORE the exchange so pending mail rides the
/// inbound session out to the peer. After the exchange completes,
/// persists `result.received` to Inbox + moves `result.sent` MIDs from
/// Outbox to Sent. Same Inbox-FIRST ordering as the ARDOP path.
#[allow(clippy::too_many_arguments)] // config + session_id together are one logical "session context"
pub fn run_vara_b2f_answer(
    transport: &mut crate::winlink::modem::vara::VaraTransport,
    peer_callsign: &str,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    use std::io::BufReader;

    // Thin wrapper over [`run_vara_b2f_answer_io`] — see
    // [`run_vara_b2f_exchange`] for why the split exists (the listener's
    // consumer task runs the answer on pre-cloned data halves while a
    // concurrent PTT pump owns the transport's cmd socket, tuxlink-yrrjq).
    let writer =
        transport
            .data_stream()
            .try_clone()
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("VARA data-socket try_clone failed: {e}"),
                source: None,
            })?;
    let reader = BufReader::new(transport.data_stream().try_clone().map_err(|e| {
        BackendError::TransportFailed {
            reason: format!("VARA data-socket try_clone (reader) failed: {e}"),
            source: None,
        }
    })?);
    run_vara_b2f_answer_io(
        reader,
        writer,
        peer_callsign,
        config,
        session_id,
        mailbox,
        position,
        progress,
    )
}

/// IO-generic core of [`run_vara_b2f_answer`]: the B2F answer exchange over
/// already-split data-socket halves, so the listener's consumer task can key
/// the rig from a concurrent PTT pump that owns the cmd socket while the
/// answer turns run (tuxlink-yrrjq — the answer side transmits too).
#[allow(clippy::too_many_arguments)]
pub fn run_vara_b2f_answer_io(
    mut reader: impl std::io::BufRead,
    mut writer: impl std::io::Write,
    peer_callsign: &str,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    // tuxlink-0063 (Phase 3, Task 3.7): the on-air station ID is the session's
    // full callsign captured AT LISTENER-ARM TIME — not config.identity.active_full.
    // Threading `&SessionIdentity` makes on-air impersonation a compile error.
    let callsign = session_id.mycall().as_str().to_uppercase();
    let locator = crate::position::effective_broadcast_locator(config, position);

    // Intent-filtered drain (tuxlink-u5hl). Listener answerer: catch the
    // safety-gate error and degrade to empty outbound rather than failing
    // the inbound session — symmetric with `run_ardop_b2f_answer` above and
    // the telnet_listen answerer. The peer is already on the link; inbound
    // mail filing still works without us shipping outbound.
    let outbound = build_outbound_proposals(mailbox, SessionIntent::P2p, None, Some(session_id.mycall().as_str())).unwrap_or_else(|e| {
        eprintln!(
            "run_vara_b2f_answer: outbound drain skipped ({e}); inbound continues with empty outbound"
        );
        Vec::new()
    });
    let outbound_log = outbound_log_items(&outbound);
    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: peer_callsign.to_string(),
        locator,
        password: None,
        intent: SessionIntent::P2p,
    };

    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        ExchangeRole::Answer,
        &exchange_config,
        outbound,
        |proposals, _manifest| {
            Ok(proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect())
        },
        None,
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e:?}"),
        source: None,
    })?;

    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
    }
    if let Some(progress) = progress {
        emit_exchange_result_progress(&result, &outbound_log, progress);
    }
    Ok(())
}

/// Run a B2F mail exchange over an already-`CONNECTED` VARA transport
/// (tuxlink-0ye6 Task 3.4 — VARA's dial-role analog of
/// [`run_ardop_b2f_exchange`] / [`run_vara_b2f_answer`]).
///
/// The transport must already be in a `CONNECTED <mycall> <target> [bw]`
/// state — the Tauri-command layer is responsible for sending `CONNECT` on
/// the cmd port and waiting for the `CONNECTED` event before calling this
/// function. This wrapper drives the B2F protocol over the VARA data socket
/// only; it does NOT touch the cmd port.
///
/// Mirrors [`run_vara_b2f_answer`]'s data-socket plumbing (try_clone the
/// underlying `TcpStream` to split duplex halves into a `BufReader` for
/// reads + the raw stream for writes), but takes `ExchangeRole::Dial` so
/// the slave/IRS turn order matches the dialer's view of the exchange.
///
/// Intent threads through `ExchangeConfig.intent` per
/// [`SessionIntent::routing_flag`] — the operator's typed intent
/// determines the routing-flag posture of the exchange (CMS → 'C', P2p →
/// no flag, RadioOnly → 'R'). Outbox messages drain through
/// `build_outbound_proposals` (same shape ARDOP uses); the per-message
/// flag-aware filter is informational at this stage — the production
/// mailbox does not yet stamp `RoutingFlag` on stored messages, so the
/// existing outbox iteration is the correct drain set for any intent.
///
/// # CMS password
///
/// For `intent == Cms`, fetches the operator's CMS password from the
/// canonical `tuxlink` keyring service via credentials::read_password (same
/// source the ARDOP path uses) so a `;PQ` challenge from the gateway can be
/// answered. For non-CMS
/// intents, skips the keyring read — peers never challenge per the FBB
/// master/slave split and a stale CMS secret should not leak into a peer
/// handshake.
///
/// # RADIO-1
///
/// The caller MUST have established the `CONNECTED` link via the cmd-port
/// `CONNECT` flow (which is itself the RF-transmitting step). This
/// function only drives the data-socket exchange — it does NOT initiate
/// transmission on its own. Aborting an in-flight exchange is the
/// session-layer `abort_in_flight` path (Task 4.1), not this function.
#[allow(clippy::too_many_arguments)] // config + session_id together are one logical "session context"
pub fn run_vara_b2f_exchange(
    transport: &mut crate::winlink::modem::vara::VaraTransport,
    target: &str,
    intent: SessionIntent,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    use std::io::BufReader;

    // Split the duplex VARA data socket into BufRead + Write halves.
    // Same pattern as `run_vara_b2f_answer` — VARA's data socket is an
    // OS-level TCP stream we can clone; the kernel arbitrates the duplex
    // halves. The B2F engine is strictly turn-based so the split is safe
    // (only one side reads or writes at any instant).
    //
    // Thin wrapper: the exchange itself lives in [`run_vara_b2f_exchange_io`]
    // so the VARA dial path can run it on pre-cloned data halves while a
    // concurrent PTT pump owns the transport's cmd socket (tuxlink-yrrjq).
    let writer =
        transport
            .data_stream()
            .try_clone()
            .map_err(|e| BackendError::TransportFailed {
                reason: format!("VARA data-socket try_clone failed: {e}"),
                source: None,
            })?;
    let reader = BufReader::new(transport.data_stream().try_clone().map_err(|e| {
        BackendError::TransportFailed {
            reason: format!("VARA data-socket try_clone (reader) failed: {e}"),
            source: None,
        }
    })?);
    run_vara_b2f_exchange_io(
        reader, writer, target, intent, config, session_id, mailbox, position, progress,
    )
}

/// IO-generic core of [`run_vara_b2f_exchange`]: the B2F dial exchange over
/// already-split data-socket reader/writer halves. Exists so the VARA dial
/// path can drive the exchange on pre-cloned halves while a concurrent PTT
/// pump owns the transport's cmd socket — VARA raises `PTT ON`/`PTT OFF` on
/// the cmd port for the ENTIRE ARQ session, including mid-exchange, and the
/// HOST must key the rig in response (tuxlink-yrrjq; VARA has no PTT
/// mechanism of its own).
#[allow(clippy::too_many_arguments)] // config + session_id together are one logical "session context"
pub fn run_vara_b2f_exchange_io(
    mut reader: impl std::io::BufRead,
    mut writer: impl std::io::Write,
    target: &str,
    intent: SessionIntent,
    config: &Config,
    session_id: &crate::identity::SessionIdentity,
    mailbox: &Mailbox,
    position: Option<&crate::position::PositionArbiter>,
    progress: Option<&dyn Fn(&str)>,
) -> Result<(), BackendError> {
    // tuxlink-0063 (Phase 3, Task 3.7): the on-air station ID is the session's
    // full callsign — the Part 97 principal — not `config.identity.active_full`.
    // Threading `&SessionIdentity` makes on-air impersonation a compile error.
    let callsign = session_id.mycall().as_str().to_uppercase();
    let locator = crate::position::effective_broadcast_locator(config, position);

    // CMS gateway may issue a `;PQ` challenge — fetch the operator's CMS
    // password from the canonical `tuxlink` keyring service via
    // credentials::read_password (same source ARDOP uses). For peer intents the FBB
    // master/slave split forbids challenges, so skip the keyring read:
    // both a hygiene win and a defensive guarantee that a stale CMS
    // secret cannot leak into a peer handshake.
    let password = if intent == SessionIntent::Cms {
        crate::winlink::credentials::read_password(&callsign)
            .ok()
            .filter(|p| !p.is_empty())
    } else {
        None
    };

    // Intent-filtered drain (tuxlink-u5hl Codex Round 5 P1 #3 +
    // Phase 3-4 RE-REVIEW P2; narrowed tuxlink-9efs): for non-CMS intents the
    // safety gate fires and returns `MessageRejected` — degrade THAT to an empty
    // outbound rather than failing the dial. The VARA CONNECT has already
    // completed by the time we reach here, so failing closed on the intended
    // skip would only orphan the operator's session, and an empty batch proposes
    // nothing so no off-spec routing flag rides out. Any OTHER error (corrupt /
    // unreadable mailbox, etc.) fail-closes via `?` so a data error cannot
    // masquerade as a successful empty send. NOT symmetric with
    // `run_vara_b2f_answer`, which still degrades every error by design.
    let outbound = dial_outbound_or_propagate(
        build_outbound_proposals(mailbox, intent, None, Some(session_id.mycall().as_str())),
        "run_vara_b2f_exchange",
    )?;
    let outbound_log = outbound_log_items(&outbound);
    let exchange_config = session::ExchangeConfig {
        mycall: callsign,
        targetcall: target.to_string(),
        locator,
        password,
        intent,
    };

    let result = session::run_exchange_with_role(
        &mut reader,
        &mut writer,
        ExchangeRole::Dial,
        &exchange_config,
        outbound,
        |proposals, _manifest| {
            Ok(proposals
                .iter()
                .map(|_| Answer::Accept { resume_offset: 0 })
                .collect())
        },
        None,
    )
    .map_err(|e| BackendError::TransportFailed {
        reason: format!("{e:?}"),
        source: None,
    })?;

    // File received messages into the inbox; move delivered ones to sent.
    // Inbox-FIRST ordering matches the ARDOP path's Codex P1.4 fix.
    for message in &result.received {
        mailbox.store(MailboxFolder::Inbox, &message.to_bytes())?;
    }
    for mid in &result.sent {
        mailbox.move_to(
            MailboxFolder::Outbox,
            MailboxFolder::Sent,
            &MessageId(mid.clone()),
        )?;
    }
    if let Some(progress) = progress {
        emit_exchange_result_progress(&result, &outbound_log, progress);
    }
    Ok(())
}

/// Seconds since the Unix epoch, now.
fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse an RFC 3339 timestamp to seconds since the epoch.
fn parse_rfc3339_secs(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

/// The `&str` a Winlink `From:` header uses for a session: the full callsign for
/// a FULL identity, or the tactical label for a tactical one.
fn address_string(a: &crate::identity::Address) -> &str {
    match a {
        crate::identity::Address::Full(c) => c.as_str(),
        crate::identity::Address::Tactical(s) => s.as_str(),
    }
}

/// Format the current wall-clock instant as an RFC 3339 / ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`). Minimal epoch-based formatter. Mirrors the manual
/// formatter in `ui_commands.rs` (`format_unix_ts`) and `wizard.rs`; precision
/// is whole seconds.
fn now_iso8601_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let sec = secs % 60;
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

/// Convert days since 1970-01-01 to (year, month, day) on the proleptic
/// Gregorian calendar (Howard Hinnant's `civil_from_days`). Same algorithm as
/// `ui_commands::days_to_ymd`.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

#[cfg(test)]
impl NativeBackend {
    /// In-process stub for unit tests that exercise `BackendState::install`
    /// lifecycle without touching real telnet or a real mailbox. Uses the
    /// shared `native_test_config()` helper; mailbox root is a tempdir.
    ///
    /// The tempdir is Box::leak'd so it lives for the test process's lifetime
    /// without requiring the caller to hold a TempDir handle. Tests are
    /// short-lived processes; the OS reclaims the allocation on exit.
    pub fn test_fixture() -> Self {
        let tempdir = tempfile::tempdir().unwrap();
        let leaked_path = Box::leak(Box::new(tempdir)).path().to_path_buf();
        Self::new(crate::test_helpers::native_test_config(), leaked_path)
    }
}

#[cfg(test)]
mod mailbox_change_tests {
    use super::*;
    use crate::winlink::compose::compose_message;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    #[tokio::test]
    async fn send_message_notifies_mailbox_change() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        let dir = tempdir().unwrap();
        let notified = Arc::new(AtomicUsize::new(0));
        let notified_for_sink = notified.clone();
        // native_test_config() has active_full = "N7CPZ"; set a matching active
        // identity so send_message no longer errors on NoActiveIdentity.
        let backend = NativeBackend::new(crate::test_helpers::native_test_config(), dir.path())
            .with_mailbox_change(Arc::new(move || {
                notified_for_sink.fetch_add(1, Ordering::SeqCst);
            }));
        backend.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("N7CPZ").unwrap(),
        )));

        backend
            .send_message(OutboundMessage {
                to: vec!["W1AW".to_string()],
                cc: vec![],
                subject: "Queued".to_string(),
                body: "body".to_string(),
                date: "2026-06-06T00:00:00Z".to_string(),
                attachments: vec![],
            })
            .await
            .expect("message queues");

        assert_eq!(notified.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn file_exchange_result_notifies_once_for_received_and_sent_changes() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let queued = compose_message(
            "N7CPZ",
            &["W1AW"],
            &[],
            "Outgoing",
            "outbound body",
            1_716_200_000,
        );
        let queued_id = mailbox
            .store(MailboxFolder::Outbox, &queued.to_bytes())
            .unwrap();
        let received = compose_message(
            "W1AW",
            &["N7CPZ"],
            &[],
            "Incoming",
            "inbound body",
            1_716_200_001,
        );
        let result = session::ExchangeResult {
            received: vec![received],
            sent: vec![queued_id.0],
            rejected: vec![],
            deferred: vec![],
            relay_state: crate::winlink::relay_banner::RelayState::NotRelay,
        };
        let notified = AtomicUsize::new(0);
        let notify = || {
            notified.fetch_add(1, Ordering::SeqCst);
        };

        file_exchange_result(&mailbox, &result, SessionIntent::Cms, &notify)
            .expect("files exchange result");

        assert_eq!(notified.load(Ordering::SeqCst), 1);
        assert_eq!(mailbox.list(MailboxFolder::Inbox).unwrap().len(), 1);
        assert_eq!(mailbox.list(MailboxFolder::Sent).unwrap().len(), 1);
        assert!(mailbox.list(MailboxFolder::Outbox).unwrap().is_empty());
    }

    #[test]
    fn file_exchange_result_does_not_notify_when_nothing_changed() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let notified = AtomicUsize::new(0);
        let notify = || {
            notified.fetch_add(1, Ordering::SeqCst);
        };

        file_exchange_result(
            &mailbox,
            &session::ExchangeResult::default(),
            SessionIntent::Cms,
            &notify,
        )
        .expect("empty exchange is valid");

        assert_eq!(notified.load(Ordering::SeqCst), 0);
    }

    fn outbound_log_fixture(mid: &str, subject: &str) -> session::OutboundMessage {
        let mut msg = Message::new();
        msg.set_header("Mid", mid);
        msg.set_header("Subject", subject);
        msg.set_header("From", "N7CPZ");
        msg.set_body(b"body\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().expect("valid outbound proposal");
        session::OutboundMessage {
            proposal,
            title: subject.to_string(),
            compressed,
        }
    }

    #[test]
    fn exchange_result_progress_lists_message_movement() {
        let sent = outbound_log_fixture("SENTMID0001", "Outbound\r\nsubject");
        let rejected = outbound_log_fixture("REJMID00001", "Already there");

        let mut received = Message::new();
        received.set_header("Mid", "INMID000001");
        received.set_header("Subject", "Incoming\nweather");
        received.set_header("From", "W1AW");
        received.set_body(b"body\r\n".to_vec());

        let result = session::ExchangeResult {
            received: vec![received],
            sent: vec![sent.proposal.mid.clone()],
            rejected: vec![rejected.proposal.mid.clone()],
            deferred: vec!["DEFMID00001".to_string()],
            relay_state: crate::winlink::relay_banner::RelayState::NotRelay,
        };
        let outbound_messages = vec![sent, rejected];
        let outbound = outbound_log_items(&outbound_messages);
        let lines = std::cell::RefCell::new(Vec::new());

        emit_exchange_result_progress(&result, &outbound, &|line| {
            lines.borrow_mut().push(line.to_string());
        });

        assert_eq!(
            lines.into_inner(),
            vec![
                "Received \"Incoming weather\" from W1AW (MID INMID000001) -> Inbox.",
                "Sent \"Outbound subject\" (MID SENTMID0001) -> Sent.",
                "Remote rejected \"Already there\" (MID REJMID00001).",
                "Remote deferred \"(unknown subject)\" (MID DEFMID00001).",
            ]
        );
    }

    #[test]
    fn exchange_result_progress_reports_no_traffic() {
        let lines = std::cell::RefCell::new(Vec::new());

        emit_exchange_result_progress(&session::ExchangeResult::default(), &[], &|line| {
            lines.borrow_mut().push(line.to_string())
        });

        assert_eq!(lines.into_inner(), vec!["No messages exchanged."]);
    }

    // ---- tuxlink-6c9y A6: Post Office inbound routing marker ----------------

    /// PostOffice sessions must stamp inbound messages with
    /// `X-Tuxlink-Received-Session: post-office`.
    #[test]
    fn file_exchange_result_stamps_post_office_marker_on_received_message() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let received = compose_message(
            "W1AW",
            &["N7CPZ"],
            &[],
            "Post Office Inbound",
            "body from local pool",
            1_716_200_000,
        );
        let result = session::ExchangeResult {
            received: vec![received],
            sent: vec![],
            rejected: vec![],
            deferred: vec![],
            relay_state: crate::winlink::relay_banner::RelayState::NotRelay,
        };
        let noop = || {};

        file_exchange_result(&mailbox, &result, SessionIntent::PostOffice, &noop)
            .expect("file_exchange_result succeeds for PostOffice");

        let ids = mailbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(ids.len(), 1, "one message should be in Inbox");
        let body = mailbox.read(MailboxFolder::Inbox, &ids[0].id).unwrap();
        let stored =
            Message::from_bytes(&body.raw_rfc5322).expect("stored bytes are valid Message");
        assert_eq!(
            stored.header(RECEIVED_SESSION_HEADER),
            Some(RECEIVED_SESSION_POST_OFFICE),
            "PostOffice session must stamp X-Tuxlink-Received-Session: post-office"
        );
    }

    /// Non-PostOffice sessions (e.g. Cms) must NOT stamp the marker.
    /// The stored bytes are byte-identical to the original.
    #[test]
    fn file_exchange_result_does_not_stamp_marker_for_cms_session() {
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let received = compose_message(
            "W1AW",
            &["N7CPZ"],
            &[],
            "CMS Inbound",
            "body from cms",
            1_716_200_001,
        );
        let original_bytes = received.to_bytes();
        let result = session::ExchangeResult {
            received: vec![received],
            sent: vec![],
            rejected: vec![],
            deferred: vec![],
            relay_state: crate::winlink::relay_banner::RelayState::NotRelay,
        };
        let noop = || {};

        file_exchange_result(&mailbox, &result, SessionIntent::Cms, &noop)
            .expect("file_exchange_result succeeds for Cms");

        let ids = mailbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(ids.len(), 1, "one message should be in Inbox");
        let body = mailbox.read(MailboxFolder::Inbox, &ids[0].id).unwrap();
        let stored =
            Message::from_bytes(&body.raw_rfc5322).expect("stored bytes are valid Message");
        assert_eq!(
            stored.header(RECEIVED_SESSION_HEADER),
            None,
            "Cms session must NOT stamp X-Tuxlink-Received-Session"
        );
        assert_eq!(
            body.raw_rfc5322, original_bytes,
            "Cms stored bytes must be byte-identical to original"
        );
    }

    // =========================================================================
    // Task 3.3 (tuxlink-0063): send_message writes From: from the active
    // SessionIdentity.address_as(), NOT from config.identity.active_full.
    //
    // Config callsign: W7AUX. Active session authenticates N7CPZ (FULL).
    // After send_message, the stored Outbox message must contain "From: N7CPZ"
    // and must NOT contain "From: W7AUX".
    // =========================================================================
    #[tokio::test]
    async fn send_message_from_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        let dir = tempdir().unwrap();

        // Build a backend whose config callsign is W7AUX but whose active session
        // authenticates as N7CPZ.
        let mut cfg = crate::test_helpers::native_test_config();
        cfg.identity.active_full = Some("W7AUX".to_string());
        let backend = NativeBackend::new(cfg, dir.path());
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));
        backend.set_active_identity(session_id);

        let id = backend
            .send_message(OutboundMessage {
                to: vec!["W1AW".to_string()],
                cc: vec![],
                subject: "From-header test".to_string(),
                body: "body".to_string(),
                date: "2026-06-11T00:00:00Z".to_string(),
                attachments: vec![],
            })
            .await
            .expect("message queues");

        // Read the raw bytes from the Outbox and inspect the From header.
        let mailbox = &backend.mailbox;
        let body = mailbox
            .read(MailboxFolder::Outbox, &id)
            .expect("read stored message");
        let raw = String::from_utf8_lossy(&body.raw_rfc5322);

        assert!(
            raw.contains("From: N7CPZ"),
            "From header must be the session callsign N7CPZ; got:\n{raw}"
        );
        assert!(
            !raw.contains("From: W7AUX"),
            "From header must NOT be the config callsign W7AUX; got:\n{raw}"
        );
    }

    // tuxlink-spbw / GH #691: queueing to the Outbox must NOT require an
    // authenticated active identity. With a configured callsign but no active
    // session (the RF-only / offline operator — callsign set, no stored CMS
    // password), send_message falls back to config.identity.active_full for the
    // From, so the draft queues instead of failing "no active identity —
    // authenticate before transmitting".
    #[tokio::test]
    async fn send_message_queues_without_active_identity_using_config_callsign() {
        let dir = tempdir().unwrap();
        let mut cfg = crate::test_helpers::native_test_config();
        cfg.identity.active_full = Some("W6BI".to_string());
        let backend = NativeBackend::new(cfg, dir.path());
        // NO set_active_identity — the slot is empty (not authenticated).

        let id = backend
            .send_message(OutboundMessage {
                to: vec!["W1AW".to_string()],
                cc: vec![],
                subject: "queue without auth".to_string(),
                body: "body".to_string(),
                date: "2026-06-15T00:00:00Z".to_string(),
                attachments: vec![],
            })
            .await
            .expect("draft queues to the Outbox without an authenticated identity");

        let body = backend
            .mailbox
            .read(MailboxFolder::Outbox, &id)
            .expect("read stored message");
        let raw = String::from_utf8_lossy(&body.raw_rfc5322);
        assert!(
            raw.contains("From: W6BI"),
            "From must fall back to the configured callsign W6BI; got:\n{raw}"
        );
    }

    // Guard: the fallback does NOT over-relax — with neither an active session
    // NOR a configured active_full callsign, there is genuinely no From, so
    // send_message still returns NoActiveIdentity.
    #[tokio::test]
    async fn send_message_still_errs_when_no_identity_configured_at_all() {
        let dir = tempdir().unwrap();
        let mut cfg = crate::test_helpers::native_test_config();
        cfg.identity.active_full = None;
        let backend = NativeBackend::new(cfg, dir.path());

        let err = backend
            .send_message(OutboundMessage {
                to: vec!["W1AW".to_string()],
                cc: vec![],
                subject: "no identity".to_string(),
                body: "body".to_string(),
                date: "2026-06-15T00:00:00Z".to_string(),
                attachments: vec![],
            })
            .await
            .expect_err("no active identity and no configured callsign → error");
        assert!(matches!(err, BackendError::NoActiveIdentity));
    }
}

#[cfg(test)]
mod native_read_state_tests {
    use super::*;
    use crate::config::{
        CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig,
        PositionPrecision, PositionSource, PrivacyConfig, CONFIG_SCHEMA_VERSION,
    };
    use crate::native_mailbox::Mailbox;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn offline_config() -> Config {
        Config {
            elmer: crate::config::ElmerConfig::default(),
            p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
            ft8: crate::config::Ft8Config::default(),
            wwv_offair: None,
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: false,
                transport: CmsTransport::Telnet,
                host: crate::config::default_cms_host(),
            },
            identity: IdentityConfig {
                active_full: None,
                identifier: None,
                grid: None,
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
            active_connection: None,
            onboarding: Some(crate::config::OnboardingConfig::default()),
        }
    }

    // tuxlink-882: the CMS handshake locator must be reduced to the configured
    // broadcast precision — a stored 6-char grid never leaks past a 4-char setting.
    #[test]
    fn cms_locator_reduces_to_broadcast_precision() {
        let mut cfg = offline_config();
        cfg.identity.grid = Some("CN87ux".to_string());

        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        assert_eq!(
            cms_locator(&cfg),
            "CN87",
            "default precision must broadcast 4-char"
        );

        cfg.privacy.position_precision = PositionPrecision::SixCharGrid;
        assert_eq!(
            cms_locator(&cfg),
            "CN87ux",
            "opt-in precision broadcasts 6-char"
        );
    }

    #[test]
    fn cms_locator_empty_when_no_grid() {
        assert_eq!(cms_locator(&offline_config()), "");
    }

    // ========================================================================
    // tuxlink-686: resolve_locator — arbiter-sourced locator tests
    // ========================================================================

    fn cfg_with_grid(grid: &str) -> Config {
        let mut cfg = offline_config();
        cfg.identity.grid = Some(grid.to_string());
        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        cfg
    }

    // No-arbiter fallback: resolve_locator(cfg, None) == cms_locator(cfg).
    #[test]
    fn resolve_locator_no_arbiter_falls_back_to_config() {
        let cfg = cfg_with_grid("CN87ux");
        assert_eq!(
            resolve_locator(&cfg, None),
            cms_locator(&cfg),
            "no arbiter: resolve_locator must equal cms_locator"
        );
        assert_eq!(
            resolve_locator(&cfg, None),
            "CN87",
            "config fallback must apply 4-char reduction"
        );
    }

    // Arbiter reduces to precision.
    #[test]
    fn resolve_locator_arbiter_reduces_to_precision() {
        let cfg = offline_config();
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::FourCharGrid,
        );
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87",
            "arbiter with FourCharGrid precision must broadcast 4-char grid"
        );
    }

    // ★ KEY TEST: arbiter SUPERSEDES a stale config grid.
    // This proves that a runtime grid change (or GPS fix) reaches the air
    // even though the backend's config snapshot was taken at construction time.
    #[test]
    fn resolve_locator_arbiter_supersedes_stale_config_grid() {
        // Config was baked at startup with DM33; arbiter has been updated to CN87ux.
        let cfg = cfg_with_grid("DM33"); // stale startup snapshot
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::FourCharGrid,
        );
        let locator = resolve_locator(&cfg, Some(&arbiter));
        // Must be the live arbiter's grid, NOT the stale config grid.
        assert_eq!(
            locator, "CN87",
            "arbiter must supersede the stale config snapshot (got {}; expected CN87, not DM33)",
            locator
        );
        assert_ne!(
            locator, "DM33",
            "stale config grid must NOT reach the air when the arbiter is present"
        );
    }

    // Codex P1-A retrofit: arbiter source=Gps with no fix; gps_state=Off.
    // Old behavior (pre-P1-A): arbiter authoritative when present → return "".
    // New behavior: gps_state=Off + source=Gps → fall back to config grid regardless
    // of whether the arbiter has a fix. The GPS grid must NEVER go on air under Off.
    // cfg_with_grid uses offline_config() which has gps_state=Off.
    #[test]
    fn resolve_locator_arbiter_gps_no_fix_with_gps_off_falls_back_to_config_grid() {
        let cfg = cfg_with_grid("CN87ux"); // config has a grid; gps_state=Off
                                           // Arbiter with GPS source but no fix yet.
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None, // no manual grid fallback either
            PositionPrecision::FourCharGrid,
        );
        // gps_state=Off: must return config grid (precision-reduced), not "".
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87",
            "gps_state=Off with no fix: must fall back to config grid, never broadcast GPS"
        );
    }

    // Complementary: arbiter source=Gps, BroadcastAtPrecision, NO fix yet → "".
    // With BroadcastAtPrecision, we go through the arbiter path; arbiter has no
    // position → broadcast_grid() returns None → unwrap_or_default() → "".
    #[test]
    fn resolve_locator_arbiter_gps_no_fix_with_broadcast_at_precision_returns_empty() {
        let mut cfg = cfg_with_grid("CN87ux");
        cfg.privacy.gps_state = GpsState::BroadcastAtPrecision;
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        // BroadcastAtPrecision + no fix: arbiter path taken; arbiter has nothing → "".
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "",
            "BroadcastAtPrecision with no GPS fix: arbiter returns empty (no fallback to config)"
        );
    }

    // SixCharGrid opt-in: arbiter passes the full 6-char grid through to the air.
    #[test]
    fn resolve_locator_arbiter_respects_six_char_precision() {
        let cfg = offline_config();
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Manual,
            Some("CN87ux".into()),
            PositionPrecision::SixCharGrid,
        );
        assert_eq!(
            resolve_locator(&cfg, Some(&arbiter)),
            "CN87ux",
            "SixCharGrid opt-in must broadcast the full 6-char grid"
        );
    }

    // ========================================================================
    // Codex P1-A: gps_state privacy gating — GPS grid must NEVER go on air
    // when gps_state is Off or LocalUiOnly. These tests cover resolve_locator
    // (which now delegates to effective_broadcast_locator in position/mod.rs).
    // ========================================================================

    fn cfg_with_grid_and_gps_state(grid: &str, gps_state: GpsState) -> Config {
        let mut cfg = offline_config();
        cfg.identity.grid = Some(grid.to_string());
        cfg.privacy.gps_state = gps_state;
        cfg.privacy.position_precision = PositionPrecision::FourCharGrid;
        cfg
    }

    // source=Gps + gps_state=Off + config.grid=Some("DM33") + GPS fix "CN87ux"
    // → result is the CONFIG grid ("DM33"), NOT "CN87".
    #[test]
    fn resolve_locator_gps_off_never_broadcasts_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::Off);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "DM33",
            "gps_state=Off: GPS fix must NOT go on air (got {locator}; expected DM33)"
        );
    }

    // source=Gps + gps_state=LocalUiOnly → config grid (no GPS on air).
    #[test]
    fn resolve_locator_gps_local_ui_only_never_broadcasts_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::LocalUiOnly);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "DM33",
            "gps_state=LocalUiOnly: GPS fix must NOT go on air (got {locator}; expected DM33)"
        );
    }

    // source=Gps + gps_state=BroadcastAtPrecision → the arbiter's GPS grid ("CN87").
    #[test]
    fn resolve_locator_gps_broadcast_at_precision_sends_gps_grid() {
        let cfg = cfg_with_grid_and_gps_state("DM33", GpsState::BroadcastAtPrecision);
        let arbiter = crate::position::PositionArbiter::new(
            PositionSource::Gps,
            None,
            PositionPrecision::FourCharGrid,
        );
        arbiter.apply_gps_fix(crate::position::Fix::test("CN87ux"));
        let locator = resolve_locator(&cfg, Some(&arbiter));
        assert_eq!(
            locator, "CN87",
            "gps_state=BroadcastAtPrecision: live GPS grid must go on air (got {locator})"
        );
    }

    // source=Manual + gps_state=Off → arbiter's manual grid (broadcasts regardless).
    #[test]
    fn resolve_locator_manual_broadcasts_regardless_of_gps_state() {
        for gps_state in [
            GpsState::Off,
            GpsState::LocalUiOnly,
            GpsState::BroadcastAtPrecision,
        ] {
            let cfg = cfg_with_grid_and_gps_state("DM33", gps_state);
            let arbiter = crate::position::PositionArbiter::new(
                PositionSource::Manual,
                Some("CN87ux".into()),
                PositionPrecision::FourCharGrid,
            );
            let locator = resolve_locator(&cfg, Some(&arbiter));
            assert_eq!(
                locator, "CN87",
                "Manual source must broadcast regardless of gps_state={gps_state:?} (got {locator})"
            );
        }
    }

    // tuxlink-xgn: the NativeBackend override of `mark_read` flips a message
    // from unread to read as observed through `list_messages` (the surface the
    // mailbox_list command consumes). Seeding goes through a sibling Mailbox at
    // the same root — the backend's `mailbox` field is private, so sharing the
    // on-disk root is the public seam (no test-only production code).
    #[tokio::test]
    async fn native_backend_mark_read_flips_unread_seen_via_list() {
        let dir = tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let raw = compose_message("N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000).to_bytes();
        let id = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend = NativeBackend::new(offline_config(), dir.path());
        assert!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "seeded inbox message should start unread"
        );

        backend.mark_read(MailboxFolder::Inbox, &id).await.unwrap();

        assert!(
            !backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "after mark_read the message should be read"
        );
    }

    // tuxlink-etxt Task 3: set_read_state round-trips read ↔ unread via
    // WinlinkBackend::set_read_state (folder-ref aware, covers user folders).
    #[tokio::test]
    async fn native_backend_set_read_state_round_trips() {
        use crate::native_mailbox::FolderRef;
        let dir = tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let raw = compose_message("N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000).to_bytes();
        let id = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend = NativeBackend::new(offline_config(), dir.path());
        assert!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "seeded inbox message should start unread"
        );

        backend
            .set_read_state(FolderRef::System(MailboxFolder::Inbox), &id, true)
            .await
            .unwrap();
        assert!(
            !backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "after set_read_state(true) the message should be read"
        );

        backend
            .set_read_state(FolderRef::System(MailboxFolder::Inbox), &id, false)
            .await
            .unwrap();
        assert!(
            backend.list_messages(MailboxFolder::Inbox).await.unwrap()[0].unread,
            "after set_read_state(false) the message should be unread again"
        );
    }

    // tuxlink-wl7n Task 7: delete_message_in moves a message out of its source
    // folder and into the shared Deleted (Trash) folder, recoverable via the
    // sidecar. Mirrors the seed-via-sibling-Mailbox seam used by the read-state
    // and bulk-move NativeBackend tests. This covers the default-identity path:
    // the seed stores under the default namespace and the delete passes
    // `origin_full = None`, so source resolution stays in that namespace. The
    // per-identity origin path (origin_full selects the source/restore namespace)
    // is covered by `native_mailbox`'s `delete_from_user_folder_*` test and the
    // bulk-identity `delete_bulk` test (tuxlink-wl7n Codex #2).
    #[tokio::test]
    async fn native_backend_delete_message_moves_to_trash() {
        use crate::native_mailbox::FolderRef;
        let dir = tempdir().unwrap();
        let seed = Mailbox::new(dir.path());
        let raw = compose_message("N7CPZ", &["W1AW"], &[], "Bye", "body", 1_716_200_000).to_bytes();
        let id = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend = NativeBackend::new(offline_config(), dir.path());
        backend
            .delete_message_in(FolderRef::System(MailboxFolder::Inbox), &id, None)
            .await
            .unwrap();

        assert!(
            backend
                .list_messages(MailboxFolder::Inbox)
                .await
                .unwrap()
                .is_empty(),
            "the deleted message left its source folder"
        );
        let trash = backend.list_messages(MailboxFolder::Deleted).await.unwrap();
        assert_eq!(trash.len(), 1, "the deleted message is present in Trash");
        assert_eq!(trash[0].id, id, "the same message is now in Trash");
    }

    // tuxlink-wl7n: deleting an Outbox message is always permitted (the operator's
    // "cancel this queued send"), including during a live session — no
    // actively-transmitting guard (struck per operator 2026-06-21). The basic
    // Outbox-delete-to-Trash path is covered by the default-identity test above
    // and the native_mailbox layer.

    // tuxlink-gqo: the dev transport resolver. With no env overrides the configured
    // transport stands (production keeps CmsSsl/8773); TUXLINK_CMS_PLAINTEXT forces
    // plaintext/8772 so the app can reach cms-z (which exposes no 8773 TLS).
    #[test]
    fn resolve_cms_endpoint_defaults_to_configured_transport() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, false, None),
            (8773, telnet::Transport::Tls)
        );
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::Telnet, false, None),
            (8772, telnet::Transport::Plaintext)
        );
    }

    #[test]
    fn resolve_cms_endpoint_plaintext_override_forces_plaintext_8772() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, true, None),
            (8772, telnet::Transport::Plaintext)
        );
    }

    #[test]
    fn resolve_cms_endpoint_honors_explicit_port_override() {
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, false, Some(8774)),
            (8774, telnet::Transport::Tls)
        );
        assert_eq!(
            resolve_cms_endpoint(CmsTransport::CmsSsl, true, Some(2323)),
            (2323, telnet::Transport::Plaintext)
        );
    }

    // tuxlink-3o0: the host resolver. Absent the TUXLINK_CMS_HOST env override,
    // the operator's configured `config.connect.host` is the dial target — the
    // default-host const is gone; the value now flows from persisted config.
    //
    // NOTE: this test deliberately does NOT read/set the TUXLINK_CMS_HOST env var
    // (process-global; would race under parallel `cargo test`). It asserts the
    // no-override branch by building a config whose host differs from any plausible
    // env value AND skipping the assertion if the env override happens to be set in
    // this process (the override-wins branch is documented, not unit-asserted, for
    // the same race reason — mirrors `resolve_cms_endpoint`'s env-free unit tests).
    #[test]
    fn resolve_cms_host_uses_configured_host_when_no_env_override() {
        if std::env::var("TUXLINK_CMS_HOST").is_ok() {
            // An override is set in this process; the config-branch is not exercised
            // here. Don't fight process-global env under parallel tests.
            return;
        }
        let mut cfg = offline_config_with_callsign();
        cfg.connect.host = "example.invalid".to_string();
        assert_eq!(
            resolve_cms_host(&cfg),
            "example.invalid",
            "with no TUXLINK_CMS_HOST override, the configured host is the dial target"
        );
    }

    // tuxlink-3o0 — THE KEY connect-exercise (operator's hard requirement). NOT a
    // shell mock: a real `TcpListener` on an ephemeral 127.0.0.1 port is dialed
    // through the SAME production code path the app uses —
    //   host      ← resolve_cms_host(&config)   (sourced from config.connect.host)
    //   transport ← resolve_cms_endpoint(Telnet) (yields Plaintext)
    //   dial      ← telnet::connect_and_exchange (the real socket open)
    // and the listener's accept() proves the dial physically connected. This proves
    // host + port + transport flow from config → a real socket.
    //
    // SAFETY (RADIO-1 / live-CMS): the target is a 127.0.0.1 listener we bind in
    // this test — NEVER a real or remote CMS. The dial host is taken from
    // `resolve_cms_host`, which we point at "127.0.0.1" via the config; the port is
    // the listener's own ephemeral port (NOT 8772/8773), so even a misconfigured
    // resolver cannot reach a real CMS from here. The fake server speaks just enough
    // of the telnet login + B2F handshake (then FQ) to let the client complete and
    // return cleanly, mirroring `telnet::tests::connects_to_a_local_mock_and_runs_an_exchange`.
    #[test]
    fn config_host_and_transport_dial_a_real_local_socket() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;

        // Skip if a dev override is set in this process — it would redirect the dial
        // away from our local listener (process-global env; don't fight it here).
        if std::env::var("TUXLINK_CMS_HOST").is_ok() {
            return;
        }

        // A local fake CMS on 127.0.0.1 — not the live CMS, not RF. It accepts the
        // dial, answers the telnet login, sends a B2F handshake + immediate quit (FQ),
        // then drains the client's writes until EOF so we never close mid-exchange.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bound = listener.local_addr().unwrap();
        let (connected_tx, connected_rx) = std::sync::mpsc::channel::<std::net::SocketAddr>();
        let server = std::thread::spawn(move || {
            let (mut sock, peer) = listener.accept().unwrap();
            // Signal the test that the dial physically connected (the proof point).
            let _ = connected_tx.send(peer);
            sock.write_all(b"Callsign :\rPassword :\r[WL2K-5.0-B2FHM$]\rCMS>\rFQ\r")
                .unwrap();
            let mut buf = [0u8; 256];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
        });

        // Build a config whose CMS host is the loopback listener (transport Telnet =
        // plaintext, so no TLS handshake is attempted against the fake server).
        let mut cfg = offline_config_with_callsign();
        cfg.connect.host = "127.0.0.1".to_string();
        cfg.connect.transport = CmsTransport::Telnet;

        // Resolve host + transport EXACTLY as native_connect does. The host MUST come
        // from the config (no env override, guarded above); the port is the listener's
        // ephemeral port (the test's stand-in for the resolve_cms_endpoint default).
        let host = resolve_cms_host(&cfg);
        assert_eq!(
            host, "127.0.0.1",
            "dial host must be sourced from config.connect.host"
        );
        let (_default_port, transport) = resolve_cms_endpoint(cfg.connect.transport, false, None);
        assert_eq!(
            transport,
            telnet::Transport::Plaintext,
            "Telnet transport must resolve to Plaintext"
        );

        let exchange_config = session::ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: telnet::CMS_TARGET_CALL.to_string(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::Cms,
        };
        let result = telnet::connect_and_exchange(
            &host,
            bound.port(),
            transport,
            &exchange_config,
            vec![],
            &|_| {},
            &|_| {},
            &|_| {},
            |_, _| Ok(vec![]),
        )
        .expect("dial to the local listener should connect and complete a clean exchange");

        // The listener accepted a connection → the dial physically connected.
        let connected_peer = connected_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the local listener should have accepted the dial");
        assert_eq!(
            connected_peer.ip().to_string(),
            "127.0.0.1",
            "the connection must originate from loopback (never a real CMS)"
        );
        // The exchange ran to completion against the fake server (nothing to send/recv).
        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
    }

    // tuxlink-9z2: an error that follows an operator abort is a cancellation;
    // otherwise the raw outcome stands (success keeps, real error keeps).
    #[test]
    fn abort_aware_outcome_maps_error_to_cancelled_when_aborted() {
        let mapped = abort_aware_outcome(
            Err(BackendError::TransportFailed {
                reason: "socket shutdown".into(),
                source: None,
            }),
            true,
        );
        assert!(matches!(mapped, Err(BackendError::Cancelled)));
    }

    #[test]
    fn abort_aware_outcome_preserves_real_error_when_not_aborted() {
        let mapped = abort_aware_outcome(
            Err(BackendError::TransportFailed {
                reason: "real failure".into(),
                source: None,
            }),
            false,
        );
        assert!(matches!(mapped, Err(BackendError::TransportFailed { .. })));
    }

    #[test]
    fn abort_aware_outcome_preserves_success_even_if_aborted() {
        // The connect completed before the abort landed — keep the success.
        assert!(abort_aware_outcome(Ok(()), true).is_ok());
    }

    #[tokio::test]
    async fn native_backend_abort_is_safe_with_no_inflight_connect() {
        let dir = tempdir().unwrap();
        let backend = NativeBackend::new(offline_config(), dir.path());
        // Nothing in flight: abort must not panic, returns Ok, leaves Disconnected.
        backend.abort().await.unwrap();
        assert!(matches!(backend.status(), BackendStatus::Disconnected));
    }

    // Codex #1: single-flight. With a connect already in flight, a second connect
    // is rejected immediately (before any network/config work) rather than racing
    // on the shared abort state and re-sending the outbox.
    #[tokio::test]
    async fn connect_rejects_a_concurrent_connect() {
        let dir = tempdir().unwrap();
        let backend = NativeBackend::new(offline_config(), dir.path());
        backend.connect_in_progress.store(true, Ordering::SeqCst);
        let result = backend
            .connect(
                TransportConfig::Cms {
                    mode: CmsTransport::Telnet,
                },
                None,
            )
            .await;
        assert!(
            matches!(result, Err(BackendError::BackendUnavailable { .. })),
            "a concurrent connect should be rejected, got {result:?}"
        );
    }

    // Phase 5 (tuxlink-tseu): a tactical session whose address is not verified
    // CMS-registered is refused at the CMS path BEFORE any dial; a FULL session is
    // never CMS-gated (the gate is a no-op and the connect proceeds to the dial).
    #[tokio::test]
    async fn tactical_unverified_is_refused_cms_without_dialing_but_full_is_not_gated() {
        use crate::identity::{
            Callsign, FullIdentity, IdentityHandle, IdentityStore, SessionIdentity,
            TacticalCmsState, TacticalIdentity, TacticalRegistrationVerifier,
        };
        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("identities.json");
        let mut store = IdentityStore::load(&store_path).unwrap();
        store
            .add_full(FullIdentity {
                callsign: Callsign::parse("W1ABC").unwrap(),
                label: None,
                has_cms_account: true,
                cms_registered: true,
            })
            .unwrap();
        store
            .add_tactical(TacticalIdentity {
                label: "EOC-3".into(),
                parent: Callsign::parse("W1ABC").unwrap(),
                cms: TacticalCmsState::Unknown,
            })
            .unwrap();
        store.save().unwrap();

        // Telnet + a loopback host => a closed plaintext port (8772) on the FULL
        // half, so the FULL dial fails fast (connection refused) rather than
        // hanging on the real CMS. The gate runs before the dial regardless.
        let mut cfg = offline_config_with_callsign();
        cfg.connect.host = "127.0.0.1".to_string();
        let backend = NativeBackend::new(cfg, tmp.path().join("mbox")).with_tactical_gate(
            // dead URL + a key: a tactical Unknown while offline never calls verify anyway.
            TacticalRegistrationVerifier::with_base_url("http://127.0.0.1:1/".into(), "K".into()),
            store_path.clone(),
            /*online=*/ false,
        );

        // Active = tactical EOC-3 under W1ABC -> Unknown + offline -> Refuse, no dial.
        backend.set_active_identity(
            SessionIdentity::tactical(
                IdentityHandle::for_test(Callsign::parse("W1ABC").unwrap()),
                "EOC-3".into(),
            )
            .unwrap(),
        );
        let err = backend
            .connect(
                TransportConfig::Cms {
                    mode: CmsTransport::Telnet,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, BackendError::TacticalNotCmsRegistered { .. }),
            "got {err:?}"
        );

        // Active = FULL W1ABC -> gate is a no-op; the connect proceeds to the dial and
        // fails with a TRANSPORT error (closed loopback port), NOT
        // TacticalNotCmsRegistered.
        backend.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("W1ABC").unwrap(),
        )));
        let full_res = backend
            .connect(
                TransportConfig::Cms {
                    mode: CmsTransport::Telnet,
                },
                None,
            )
            .await;
        if let Err(e) = full_res {
            assert!(
                !matches!(e, BackendError::TacticalNotCmsRegistered { .. }),
                "FULL identity must never be CMS-gated, got {e:?}"
            );
        }
    }

    // Phase 5 (tuxlink-tseu) Task 5: the CMS gate is structurally unreachable for
    // non-CMS transports. A tactical session whose CMS registration is Unknown
    // (which WOULD be refused on the CMS path) attempts a PACKET (RF/P2P) connect;
    // it must proceed to the KISS link open and fail TransportFailed (closed
    // loopback port), NEVER BackendError::TacticalNotCmsRegistered. Pins the spec's
    // "P2P / RF unrestricted" invariant.
    #[tokio::test]
    async fn tactical_packet_p2p_is_never_cms_gated() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening → connection refused

        let backend = NativeBackend::new(offline_config_with_callsign(), tempdir().unwrap().path());
        // Active = tactical EOC-3 under N7CPZ; CMS state defaults to Unknown (no
        // store seeded) — a CMS connect would fail-close, but packet/RF is never gated.
        backend.set_active_identity(
            crate::identity::SessionIdentity::tactical(
                crate::identity::IdentityHandle::for_test(
                    crate::identity::Callsign::parse("N7CPZ").unwrap(),
                ),
                "EOC-3".into(),
            )
            .unwrap(),
        );
        let err = backend
            .connect(
                TransportConfig::Packet {
                    link: KissLinkConfig::Tcp {
                        host: addr.ip().to_string(),
                        port: addr.port(),
                    },
                    ssid: 7,
                    role: PacketRole::DialTo {
                        call: "W7AUX".into(),
                        path: vec![],
                    },
                    intent: SessionIntent::Cms,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            !matches!(err, BackendError::TacticalNotCmsRegistered { .. }),
            "packet/RF must never be CMS-gated; got {err:?}"
        );
        assert!(
            matches!(err, BackendError::TransportFailed { .. }),
            "tactical packet connect should reach link-open and fail TransportFailed; got {err:?}"
        );
    }

    // =========================================================================
    // Task 4b (tuxlink-bsiy): selecting-connect integration over a 127.0.0.1
    // loopback. Proves the FULL wiring: native_connect, given a CmsSelectionContext
    // (which `cms_connect` supplies iff the fresh on-disk review-inbound preference
    // is on), builds the SELECTING decider (not accept-all) — REGARDLESS of the
    // backend's in-memory `live_config` flag, which `cfg` below sets to false to
    // pin the connect-path staleness fix. The decider emits InboundProposalsOffered
    // through the threaded sink, parks on the registry, and the operator's resolved
    // selection lands the chosen message in the Inbox.
    //
    // The fake server is the Answer-role master that OFFERS one proposal (the
    // mirror of `two_native_backends_exchange_with_attachment`, which offers from
    // the client side). The operator answer is delivered from a separate thread
    // that spin-waits for the registry slot — exactly the Task 3 decider-test
    // rendezvous, scaled to a real socket.
    //
    // RADIO-1: 127.0.0.1 loopback only. Nothing is transmitted. `#[serial]`
    // because it sets the process-global TUXLINK_CMS_PORT to point native_connect
    // at the ephemeral loopback listener (serial_test gates env-mutating tests).
    //
    // Hang-safety: this test runs `native_connect` synchronously on the main
    // thread (it borrows `client_mailbox`, asserted after the call, so a bounded
    // off-thread join would force the mailbox to move out and back). A protocol
    // stall therefore cannot run away — the client connect is bounded by telnet
    // `CONNECT_TIMEOUT` (15s) and the read/write `TIMEOUT` (60s), so a wedged
    // server surfaces as an `Err` from `native_connect` rather than a hang; the
    // answerer thread's slot wait is itself bounded (400×5ms spin then panic).
    // =========================================================================
    #[test]
    #[serial_test::serial]
    fn selecting_connect_emits_offer_and_files_selected_message_into_inbox() {
        use crate::winlink::b2f_events::{AttemptId, B2fEvent, B2fEventSink};
        use crate::winlink::inbound_selection::{
            resolve_selection, InboundSelection, SelectionRegistry, UnselectedDisposition,
        };
        use crate::winlink::session::{
            run_exchange_with_role, ExchangeConfig, ExchangeRole,
            OutboundMessage as SessionOutbound, SessionIntent,
        };
        use crate::winlink::telnet::CMS_TARGET_CALL;
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Mutex as StdMutex;

        // A recording sink that captures every emitted B2fEvent so the test can
        // assert the InboundProposalsOffered event fired with the redacted DTO.
        struct RecordingSink {
            events: Arc<StdMutex<Vec<B2fEvent>>>,
        }
        impl B2fEventSink for RecordingSink {
            fn push(&self, event: B2fEvent) {
                self.events.lock().unwrap().push(event);
            }
        }

        // -------------------------------------------------------------------
        // Step 1: the server composes the ONE message it will offer the client.
        // A plain alphanumeric generated MID is redaction-stable, so the DTO MID
        // the operator sees equals the real proposal MID the decider matches on.
        // -------------------------------------------------------------------
        let offered = compose_message(
            "W7AUX",
            &["N7CPZ"],
            &[],
            "Selecting-path inbound",
            "pick me",
            1_716_300_000,
        );
        let offered_mid = offered
            .header("Mid")
            .expect("composed message has a Mid")
            .to_string();
        let (proposal, compressed) = offered.to_proposal().expect("offered message → proposal");
        let server_outbound = vec![SessionOutbound {
            proposal,
            title: "Selecting-path inbound".to_string(),
            compressed,
        }];

        // -------------------------------------------------------------------
        // Step 2: spawn the fake Answer-role server that offers the proposal.
        // -------------------------------------------------------------------
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let listen_port = listener.local_addr().expect("listener addr").port();

        let server = std::thread::spawn(move || {
            let (sock, _) = listener.accept().expect("accept");
            let mut writer = sock.try_clone().expect("clone for write");
            // Telnet login prompts, then read+discard the client's callsign+password.
            writer
                .write_all(b"Callsign :\rPassword :\r")
                .expect("write login prompts");
            let mut reader = BufReader::new(sock);
            for _ in 0..2 {
                let mut line = Vec::new();
                reader
                    .read_until(b'\r', &mut line)
                    .expect("read login response");
            }
            let server_config = ExchangeConfig {
                mycall: "W7AUX".into(),
                targetcall: CMS_TARGET_CALL.to_string(),
                locator: "CN87".into(),
                password: None,
                intent: SessionIntent::Cms,
            };
            run_exchange_with_role(
                &mut reader,
                &mut writer,
                ExchangeRole::Answer,
                &server_config,
                server_outbound, // the server OFFERS this message
                |proposals, _manifest| {
                    Ok(proposals
                        .iter()
                        .map(|_| Answer::Accept { resume_offset: 0 })
                        .collect())
                },
                None,
            )
            .expect("server-side Answer exchange must succeed");
        });

        // -------------------------------------------------------------------
        // Step 3: build the selection context + spawn the operator-answer thread.
        // The answer thread spin-waits for the decider to register its slot, reads
        // the offered (redaction-stable) MID off the emitted event, then resolves
        // the selection — modelling the Tauri resolve command.
        // -------------------------------------------------------------------
        let registry: SelectionRegistry = Arc::new(Mutex::new(None));
        let aborting = Arc::new(AtomicBool::new(false));
        let attempt_id = AttemptId(4242);
        let events = Arc::new(StdMutex::new(Vec::<B2fEvent>::new()));
        let sink: Arc<dyn B2fEventSink> = Arc::new(RecordingSink {
            events: events.clone(),
        });

        let answerer = {
            let registry = registry.clone();
            let want_mid = offered_mid.clone();
            std::thread::spawn(move || {
                // Spin-wait (bounded) for the slot the decider registers.
                let (slot_attempt, slot_req) = {
                    let mut found = None;
                    for _ in 0..400 {
                        if let Some(s) = registry.lock().unwrap().as_ref() {
                            found = Some((s.attempt_id, s.request_id));
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    found.expect("decider never registered a selection slot")
                };
                let delivered = resolve_selection(
                    &registry,
                    slot_attempt,
                    slot_req,
                    InboundSelection {
                        selected_mids: vec![want_mid],
                        disposition: UnselectedDisposition::Hold,
                    },
                );
                assert!(delivered, "resolve_selection should match the live slot");
            })
        };

        // -------------------------------------------------------------------
        // Step 4: run native_connect (the REAL decider-building path). Config has
        // a callsign, an empty outbox (so the client offers nothing on its first
        // turn), the review preference ON, and host=127.0.0.1. The ephemeral port
        // is handed in via TUXLINK_CMS_PORT (Plaintext, so no TLS).
        // -------------------------------------------------------------------
        let client_dir = tempdir().unwrap();
        let client_mailbox = Mailbox::new(client_dir.path());
        let mut cfg = config_with_call("N7CPZ");
        cfg.connect.host = "127.0.0.1".to_string();
        // tuxlink-bsiy connect-path staleness regression: the live-config flag is
        // intentionally FALSE here. The decider must be selected by the PRESENCE of
        // the CmsSelectionContext (which `cms_connect` builds only when the FRESH
        // on-disk preference is on), NOT by this in-memory `live_config` snapshot —
        // which lags the toggle when the preference is enabled before the backend
        // finishes installing. Before the fix, `(false, Some(ctx))` took the
        // accept-all arm and no offer fired; this assertion caught that.
        cfg.review_inbound_before_download = false;

        // SAFETY: edition-2021 set_var; `#[serial]` ensures no concurrent test
        // reads TUXLINK_CMS_PORT while it is set. The RAII guard clears it on drop
        // so a panic between here and the assertions cannot leak the override into
        // a later test in the same process.
        struct CmsPortGuard;
        impl Drop for CmsPortGuard {
            fn drop(&mut self) {
                std::env::remove_var("TUXLINK_CMS_PORT");
            }
        }
        std::env::set_var("TUXLINK_CMS_PORT", listen_port.to_string());
        let _cms_port_guard = CmsPortGuard;

        let abort_handle: Mutex<Option<TcpStream>> = Mutex::new(None);
        let ctx = CmsSelectionContext {
            sink,
            attempt_id,
            registry: registry.clone(),
        };
        // tuxlink-0063 (Phase 3): native_connect now takes the session. The on-air
        // callsign comes from this session (N7CPZ), matching the prior config-derived
        // callsign so the exchange behavior is unchanged.
        let session_id =
            crate::identity::SessionIdentity::full(crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ));
        let result = native_connect(
            &cfg,
            &session_id,
            &client_mailbox,
            CmsTransport::Telnet,
            &|_| {},
            &|_| {},
            &|| {},
            &abort_handle,
            aborting,
            None,
            Some(ctx),
        );
        // TUXLINK_CMS_PORT is cleared by `_cms_port_guard` on scope exit (incl. panic).

        result.expect("selecting connect must complete");
        answerer.join().expect("operator-answer thread panicked");
        server.join().expect("server thread panicked");

        // -------------------------------------------------------------------
        // Assertion (a): the InboundProposalsOffered event fired with the offered
        // (redacted) proposal under the threaded attempt_id.
        // -------------------------------------------------------------------
        let log = events.lock().unwrap();
        let offer = log.iter().find_map(|e| match e {
            B2fEvent::InboundProposalsOffered {
                proposals,
                attempt_id: a,
                ..
            } => Some((proposals.clone(), *a)),
            _ => None,
        });
        let (dtos, evt_attempt) =
            offer.expect("an InboundProposalsOffered event must have been emitted");
        assert_eq!(
            evt_attempt, attempt_id,
            "the event must carry the threaded attempt_id"
        );
        assert_eq!(dtos.len(), 1, "exactly one proposal was offered");
        assert_eq!(
            dtos[0].mid, offered_mid,
            "the redacted DTO MID must match the offered MID"
        );

        // -------------------------------------------------------------------
        // Assertion (b): the selected message landed in the client's Inbox.
        // -------------------------------------------------------------------
        let inbox = client_mailbox
            .list(MailboxFolder::Inbox)
            .expect("list inbox");
        assert_eq!(
            inbox.len(),
            1,
            "the selected message must be in the Inbox; got {inbox:?}"
        );
    }

    // =========================================================================
    // Task 3.1 (tuxlink-0063): native_connect derives the on-air station ID
    // (the telnet-login callsign → ExchangeConfig.mycall) from the SessionIdentity
    // it is handed, NOT from `config.identity.active_full`. The config carries
    // W7AUX; the active session authenticates N7CPZ; the callsign the client sends
    // at the `Callsign :` prompt MUST be N7CPZ.
    //
    // The fake server captures the first non-prompt line the client writes after
    // the `Callsign :` prompt (the login callsign) and then closes the socket. The
    // client's exchange therefore errors after login — irrelevant here: the only
    // load-bearing assertion is which callsign went on the wire. `#[serial]`
    // because it sets the process-global TUXLINK_CMS_PORT to point native_connect
    // at the ephemeral loopback listener.
    // =========================================================================
    #[test]
    #[serial_test::serial]
    fn native_connect_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Mutex as StdMutex;

        // The fake server records the login callsign the client sends.
        let captured: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let listen_port = listener.local_addr().expect("listener addr").port();

        let server = {
            let captured = captured.clone();
            std::thread::spawn(move || {
                let (sock, _) = listener.accept().expect("accept");
                let mut writer = sock.try_clone().expect("clone for write");
                // Prompt for the callsign, read the client's reply, capture it, close.
                writer
                    .write_all(b"Callsign :\r")
                    .expect("write callsign prompt");
                let mut reader = BufReader::new(sock);
                let mut line = Vec::new();
                reader
                    .read_until(b'\r', &mut line)
                    .expect("read callsign reply");
                let call = String::from_utf8_lossy(&line)
                    .trim_end_matches('\r')
                    .to_string();
                *captured.lock().unwrap() = Some(call);
                // Drop the socket: the client's login/exchange then errors out.
            })
        };

        // Config callsign is W7AUX; the active session authenticates N7CPZ.
        let client_dir = tempdir().unwrap();
        let client_mailbox = Mailbox::new(client_dir.path());
        let mut cfg = config_with_call("W7AUX");
        cfg.connect.host = "127.0.0.1".to_string();
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));

        // RAII guard clears TUXLINK_CMS_PORT on drop, even on panic.
        struct CmsPortGuard;
        impl Drop for CmsPortGuard {
            fn drop(&mut self) {
                std::env::remove_var("TUXLINK_CMS_PORT");
            }
        }
        std::env::set_var("TUXLINK_CMS_PORT", listen_port.to_string());
        let _cms_port_guard = CmsPortGuard;

        let abort_handle: Mutex<Option<TcpStream>> = Mutex::new(None);
        let aborting = Arc::new(AtomicBool::new(false));
        // The connect is expected to error (server closes after login); we only
        // care about which callsign the client put on the wire.
        let _ = native_connect(
            &cfg,
            &session_id,
            &client_mailbox,
            CmsTransport::Telnet,
            &|_| {},
            &|_| {},
            &|| {},
            &abort_handle,
            aborting,
            None,
            None,
        );

        server.join().expect("server thread panicked");
        let observed = captured.lock().unwrap().clone();
        assert_eq!(
            observed.as_deref(),
            Some("N7CPZ"),
            "the on-air login callsign must be the session mycall (N7CPZ), not the config callsign (W7AUX)"
        );
    }

    // =========================================================================
    // Task 3.2 (tuxlink-0063): cms_connect_test derives the on-air station ID
    // (the telnet-login callsign → ExchangeConfig.mycall) from the
    // SessionIdentity stored in the backend, NOT from
    // `config.identity.active_full`. The config carries W7AUX; the active
    // session authenticates N7CPZ; the callsign the client sends at the
    // `Callsign :` prompt MUST be N7CPZ.
    //
    // The fake server captures the first non-prompt line after the
    // `Callsign :` prompt then closes the socket (mirrors Task 3.1's
    // native_connect_mycall_comes_from_session_not_config). The exchange
    // errors after login — irrelevant: the only load-bearing assertion is
    // which callsign went on the wire. `#[serial]` because it sets the
    // process-global TUXLINK_CMS_PORT.
    // =========================================================================
    #[tokio::test]
    #[serial_test::serial]
    async fn cms_connect_test_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use crate::winlink::b2f_events::{AttemptId, B2fEvent, B2fEventSink};
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::Mutex as StdMutex;

        // A no-op event sink: the test only cares about the wire callsign, not events.
        struct NoopSink;
        impl B2fEventSink for NoopSink {
            fn push(&self, _: B2fEvent) {}
        }

        // The fake server records the login callsign the client sends.
        let captured: Arc<StdMutex<Option<String>>> = Arc::new(StdMutex::new(None));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let listen_port = listener.local_addr().expect("listener addr").port();

        let server = {
            let captured = captured.clone();
            std::thread::spawn(move || {
                let (sock, _) = listener.accept().expect("accept");
                let mut writer = sock.try_clone().expect("clone for write");
                // Prompt for the callsign, read the client's reply, capture it, close.
                writer
                    .write_all(b"Callsign :\r")
                    .expect("write callsign prompt");
                let mut reader = BufReader::new(sock);
                let mut line = Vec::new();
                reader
                    .read_until(b'\r', &mut line)
                    .expect("read callsign reply");
                let call = String::from_utf8_lossy(&line)
                    .trim_end_matches('\r')
                    .to_string();
                *captured.lock().unwrap() = Some(call);
                // Drop the socket: the client's login/exchange then errors out.
            })
        };

        // Config callsign is W7AUX; the active session authenticates N7CPZ.
        let mut cfg = config_with_call("W7AUX");
        cfg.connect.host = "127.0.0.1".to_string();
        let dir = tempfile::tempdir().unwrap();
        let backend = NativeBackend::new(cfg.clone(), dir.path());
        // Refresh the backend's live config to pick up the host override.
        backend.set_config(cfg);
        // Set the active session identity to N7CPZ (different from config callsign).
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));
        backend.set_active_identity(session_id);

        // RAII guard clears TUXLINK_CMS_PORT on drop, even on panic.
        struct CmsPortGuard;
        impl Drop for CmsPortGuard {
            fn drop(&mut self) {
                std::env::remove_var("TUXLINK_CMS_PORT");
            }
        }
        std::env::set_var("TUXLINK_CMS_PORT", listen_port.to_string());
        let _cms_port_guard = CmsPortGuard;

        let events: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink> =
            std::sync::Arc::new(NoopSink);
        let attempt_id = AttemptId::fresh();

        // The call is expected to error (server closes after login); we only
        // care about which callsign the client put on the wire.
        let _ = backend.cms_connect_test(events, attempt_id).await;

        server.join().expect("server thread panicked");
        let observed = captured.lock().unwrap().clone();
        assert_eq!(
            observed.as_deref(),
            Some("N7CPZ"),
            "cms_connect_test must use the session mycall (N7CPZ), not the config callsign (W7AUX)"
        );
    }

    // Phase 5 (tuxlink-tseu) — adversarial-review B1: cms_connect_test is a second
    // CMS-Telnet entry point and MUST be gated like NativeBackend::connect. A
    // tactical session with an unverified address is refused BEFORE any dial (no
    // fake server needed — the gate returns before the socket opens).
    #[tokio::test]
    async fn cms_connect_test_refuses_unverified_tactical_without_dialing() {
        use crate::identity::{
            Callsign, FullIdentity, IdentityHandle, IdentityStore, SessionIdentity,
            TacticalCmsState, TacticalIdentity, TacticalRegistrationVerifier,
        };
        use crate::winlink::b2f_events::{AttemptId, B2fEvent, B2fEventSink};

        struct NoopSink;
        impl B2fEventSink for NoopSink {
            fn push(&self, _: B2fEvent) {}
        }

        let tmp = tempfile::tempdir().unwrap();
        let store_path = tmp.path().join("identities.json");
        let mut store = IdentityStore::load(&store_path).unwrap();
        store
            .add_full(FullIdentity {
                callsign: Callsign::parse("W1ABC").unwrap(),
                label: None,
                has_cms_account: true,
                cms_registered: true,
            })
            .unwrap();
        store
            .add_tactical(TacticalIdentity {
                label: "EOC-3".into(),
                parent: Callsign::parse("W1ABC").unwrap(),
                cms: TacticalCmsState::Unknown,
            })
            .unwrap();
        store.save().unwrap();

        let backend = NativeBackend::new(offline_config_with_callsign(), tmp.path().join("mbox"))
            .with_tactical_gate(
                TacticalRegistrationVerifier::with_base_url(
                    "http://127.0.0.1:1/".into(),
                    "K".into(),
                ),
                store_path.clone(),
                /*online=*/ false,
            );
        backend.set_active_identity(
            SessionIdentity::tactical(
                IdentityHandle::for_test(Callsign::parse("W1ABC").unwrap()),
                "EOC-3".into(),
            )
            .unwrap(),
        );

        let events: std::sync::Arc<dyn crate::winlink::b2f_events::B2fEventSink> =
            std::sync::Arc::new(NoopSink);
        let err = backend
            .cms_connect_test(events, AttemptId::fresh())
            .await
            .unwrap_err();
        assert!(
            matches!(err, BackendError::TacticalNotCmsRegistered { .. }),
            "a tactical session must be refused by the CMS password test too; got {err:?}"
        );
    }

    // =========================================================================
    // Task 4: resolve_packet_endpoint tests (spec §4.4 identity split)
    // =========================================================================

    #[test]
    fn resolve_packet_endpoint_dial_builds_ssidd_link_addr_and_base_b2f_call() {
        // Identity split (spec §4.4): the AX.25 link addr carries the SSID; the B2F
        // identity is the BASE call. Dial role → ExchangeRole::Dial + a target.
        let resolved = resolve_packet_endpoint(
            "N7CPZ",
            7,
            PacketRole::DialTo {
                call: "W7AUX".into(),
                path: vec!["RELAY-1".into()],
            },
            SessionIntent::Cms,
        )
        .unwrap();
        assert_eq!(
            resolved.link_mycall,
            Address {
                call: "N7CPZ".into(),
                ssid: 7
            }
        );
        assert_eq!(resolved.base_mycall, "N7CPZ");
        assert_eq!(resolved.role, ExchangeRole::Dial);
        let (target, digis) = resolved.dial.unwrap();
        assert_eq!(
            target,
            Address {
                call: "W7AUX".into(),
                ssid: 0
            }
        );
        assert_eq!(
            digis,
            vec![Address {
                call: "RELAY".into(),
                ssid: 1
            }]
        );
    }

    #[test]
    fn resolve_packet_endpoint_listen_yields_answer_role_and_no_target() {
        let resolved =
            resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen, SessionIntent::P2p).unwrap();
        assert_eq!(
            resolved.link_mycall,
            Address {
                call: "N7CPZ".into(),
                ssid: 7
            }
        );
        assert_eq!(resolved.base_mycall, "N7CPZ");
        assert_eq!(resolved.role, ExchangeRole::Answer);
        assert!(resolved.dial.is_none());
        assert_eq!(resolved.intent, SessionIntent::P2p);
    }

    #[test]
    fn resolve_packet_endpoint_rejects_more_than_two_digipeaters() {
        let err = resolve_packet_endpoint(
            "N7CPZ",
            0,
            PacketRole::DialTo {
                call: "W7AUX".into(),
                path: vec!["A-1".into(), "B-2".into(), "C-3".into()],
            },
            SessionIntent::Cms,
        )
        .unwrap_err();
        assert!(matches!(err, BackendError::NotConfigured(_)));
    }

    #[test]
    fn resolve_packet_endpoint_rejects_malformed_ax25_target_and_via() {
        // FIX-3 [P3]: a malformed target or via hop is rejected BEFORE any
        // Address is built, so a bad byte never reaches the RF address field.
        // Malformed TARGET forms.
        for bad_target in ["w7aux-16", "TOOLONGCALL", "W7:AUX", "W7 AUX", ""] {
            let err = resolve_packet_endpoint(
                "N7CPZ",
                0,
                PacketRole::DialTo {
                    call: bad_target.into(),
                    path: vec![],
                },
                SessionIntent::Cms,
            )
            .unwrap_err();
            assert!(
                matches!(err, BackendError::NotConfigured(_)),
                "malformed target {bad_target:?} must be rejected"
            );
        }
        // A malformed VIA hop (valid target) is rejected too.
        for bad_hop in ["relay-16", "TOOLONGHOP", "A B"] {
            let err = resolve_packet_endpoint(
                "N7CPZ",
                0,
                PacketRole::DialTo {
                    call: "W7AUX".into(),
                    path: vec![bad_hop.into()],
                },
                SessionIntent::Cms,
            )
            .unwrap_err();
            assert!(
                matches!(err, BackendError::NotConfigured(_)),
                "malformed via hop {bad_hop:?} must be rejected"
            );
        }
        // A well-formed target + hops still resolves (no false rejection).
        let ok = resolve_packet_endpoint(
            "N7CPZ",
            0,
            PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec!["RELAY".into(), "WIDE2-1".into()],
            },
            SessionIntent::Cms,
        );
        assert!(ok.is_ok(), "valid AX.25 target + hops must not be rejected");
    }

    // =========================================================================
    // Task 3.8 (tuxlink-0063): packet base call is the session call, not config
    // =========================================================================

    /// Guard test (documents the contract): resolve_packet_endpoint fed with the
    /// session mycall yields base_mycall == that callsign and the SSID'd link
    /// address. This test passes immediately (resolve_packet_endpoint already
    /// takes &str); its purpose is to document that the base now comes from the
    /// session, not config, and to catch any future regression that changes the
    /// source of the &str passed to resolve_packet_endpoint.
    #[test]
    fn packet_base_call_is_session_call() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};

        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));
        let resolved = resolve_packet_endpoint(
            session_id.mycall().as_str(),
            7,
            PacketRole::DialTo {
                call: "W7AUX".into(),
                path: vec![],
            },
            SessionIntent::Cms,
        )
        .unwrap();
        assert_eq!(
            resolved.base_mycall, "N7CPZ",
            "base_mycall must equal the session callsign"
        );
        assert_eq!(
            resolved.link_mycall,
            Address {
                call: "N7CPZ".into(),
                ssid: 7
            },
            "link address must carry the session call + SSID"
        );
    }

    /// Behavior test (the real one): proves packet_connect_inner derives its base
    /// call from the SESSION identity, not config.identity.active_full. The config
    /// carries W7AUX; the active session authenticates N7CPZ. Both peers need an
    /// active identity for packet_connect_inner to succeed; the dialer's identity
    /// is N7CPZ and the answerer's is W7AUX. The observable is the From: header
    /// in the message the answerer receives: it must say N7CPZ (the session call),
    /// not W7AUX (the config call).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn packet_connect_inner_base_call_is_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};

        let wire = spawn_kiss_wire();

        let dialer_dir = tempdir().unwrap();
        let answerer_dir = tempdir().unwrap();

        // Seed one outbound message from the dialer so the B2F exchange has
        // something to transfer (proves the exchange completes, not just the
        // handshake).
        let seed = Mailbox::new(dialer_dir.path());
        let raw = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "Session-call test",
            "base call from session",
            1_716_200_000,
        )
        .to_bytes();
        seed.store(MailboxFolder::Outbox, &raw).unwrap();

        // Dialer: config says W7AUX but the active session authenticates N7CPZ.
        // This is the split that the test exercises: session beats config.
        let mut dialer_cfg = config_with_call("W7AUX");
        dialer_cfg.connect.host = "127.0.0.1".to_string();
        let dialer = NativeBackend::new(dialer_cfg, dialer_dir.path());
        dialer.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("N7CPZ").unwrap(),
        )));

        // Answerer: config and session both say W7AUX (no identity split here).
        let answerer = NativeBackend::new(config_with_call("W7AUX"), answerer_dir.path())
            .with_packet_allowlist(
                crate::winlink::listener::AllowedStations::new().with_allow_all(true),
            );
        answerer.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("W7AUX").unwrap(),
        )));

        let listen = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::Listen,
            intent: SessionIntent::Cms,
        };
        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec![],
            },
            intent: SessionIntent::Cms,
        };

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            tokio::join!(answerer.connect(listen, None), dialer.connect(dial, None))
        })
        .await;

        let (ans_res, dial_res) =
            outcome.expect("packet identity test timed out (connect/handshake deadlock?)");
        ans_res.expect("answerer connect+exchange failed");
        dial_res.expect("dialer connect+exchange failed");

        // The message in the answerer's inbox must have From: N7CPZ — the session
        // callsign — NOT W7AUX (the dialer's config callsign).
        let inbox = Mailbox::new(answerer_dir.path())
            .list(MailboxFolder::Inbox)
            .unwrap();
        assert_eq!(
            inbox.len(),
            1,
            "answerer inbox must hold exactly one message; got {inbox:?}"
        );
        assert_eq!(
            inbox[0].from.trim().to_uppercase(),
            "N7CPZ",
            "packet base call must be the SESSION call (N7CPZ), not the config call (W7AUX)"
        );
    }

    // =========================================================================
    // Task 12 (tuxlink-c39af): packet SessionIntent plumbing
    // =========================================================================

    #[test]
    fn packet_dial_default_intent_is_cms_and_p2p_is_not_cms() {
        // [R5-3] pins both directions of the contract: existing callers are
        // untouched (Cms default), and a P2P packet session is not
        // classified as CMS.
        //
        // Assert on `cms` BEFORE building `p2p` via struct-update (`..cms`):
        // `PacketConnectCtx::password` is `Option<String>` (not `Copy`), so
        // `..cms` partially moves `cms` and a later `&cms` would not compile.
        let cms = PacketConnectCtx {
            base_mycall: "W6ABC",
            targetcall: "N0DAJ-10",
            password: None,
            role: ExchangeRole::Dial,
            locator: "CN87",
            intent: SessionIntent::Cms,
        };
        assert_eq!(exchange_config_for_packet(&cms).intent, SessionIntent::Cms);
        let p2p = PacketConnectCtx {
            intent: SessionIntent::P2p,
            ..cms
        };
        assert_eq!(exchange_config_for_packet(&p2p).intent, SessionIntent::P2p);
    }

    // =========================================================================
    // Task 5: native_packet_exchange tests
    // FakeAx25Stream: reads from inbound Cursor, writes into a shared Vec.
    // =========================================================================

    struct FakeAx25Stream {
        inbound: std::io::Cursor<Vec<u8>>,
        outbound: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }
    impl std::io::Read for FakeAx25Stream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.inbound.read(buf)
        }
    }
    impl std::io::Write for FakeAx25Stream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.outbound
                .lock()
                .expect("fake outbound")
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn native_packet_exchange_dials_a_gateway_with_secure_login() {
        use crate::winlink::secure::secure_login_response;
        // A scripted gateway: speaks first, challenges, then quits (empty mailbox).
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\r;PQ: 12345678\rCMS>\r");
        server.extend_from_slice(b"FF\r");
        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let mailbox = Mailbox::new(tempdir().unwrap().path());
        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ", // base B2F call (NO ssid)
                targetcall: "W7AUX",  // target call (gateway)
                password: Some("MYPASS".into()),
                role: ExchangeRole::Dial,
                locator: "CN87", // controller directive: pass cms_locator
                intent: SessionIntent::Cms,
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );
        assert!(result.is_ok(), "gateway dial must succeed, got {result:?}");

        // The secure-login token must appear in the written bytes.
        let token = secure_login_response("12345678", "MYPASS");
        let written = outbound_spy.lock().unwrap();
        assert!(
            written.windows(token.len()).any(|w| w == token.as_bytes()),
            "the secure-login token must appear in our handshake; wrote {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    #[test]
    fn native_packet_exchange_answers_a_peer_and_receives_a_message() {
        use crate::winlink::message::Message as WMessage;
        use crate::winlink::proposal::batch_checksum_line;
        use crate::winlink::transfer;

        let mut peer = Vec::new();
        peer.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\rW7AUX>\r");
        let mut msg = WMessage::new();
        msg.set_header("Mid", "PEERMSG00009");
        msg.set_header("Subject", "P2P");
        msg.set_body(b"hello from the field\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();
        peer.extend_from_slice(proposal.line().as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        peer.push(b'\r');
        peer.extend_from_slice(&transfer::frame_block("P2P", 0, &compressed));
        peer.extend_from_slice(b"FQ\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(peer),
            outbound: outbound_spy.clone(),
        };

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Answer,
                locator: "CN87",
                intent: SessionIntent::Cms,
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );
        assert!(
            result.is_ok(),
            "answer exchange must succeed, got {result:?}"
        );

        // The received peer message was filed into the inbox.
        let inbox = mailbox.list(MailboxFolder::Inbox).unwrap();
        assert!(
            inbox.iter().any(|m| m.id.0 == "PEERMSG00009"),
            "PEERMSG00009 must be in the inbox; got {inbox:?}"
        );
    }

    // =========================================================================
    // Task 4.3: FS-reject MIDs map to BackendError::MessageRejected
    // =========================================================================

    /// When the CMS sends `FS N` for our proposal, `ExchangeResult.rejected`
    /// contains the MID. The caller (`native_packet_exchange`) must convert that
    /// into `BackendError::MessageRejected` instead of silently succeeding.
    #[test]
    fn fs_reject_for_our_mid_maps_to_message_rejected_error() {
        use crate::winlink::message::Message as WMessage;

        // Build an outbox message so native_packet_exchange has something to
        // propose to the gateway.
        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());
        let mut msg = WMessage::new();
        msg.set_header("Mid", "REJECTME0001");
        msg.set_header("Subject", "FS-reject test");
        msg.set_body(b"Should be rejected by the gateway.\r\n".to_vec());
        mailbox
            .store(MailboxFolder::Outbox, &msg.to_bytes())
            .expect("store to outbox");

        // Scripted gateway (Dial role: gateway speaks first, no challenge):
        //   1. CMS handshake
        //   2. FS N  — reject our one proposal
        //   3. FF    — gateway has nothing to offer us
        // After FS N our remaining queue is empty and remote_no_messages=true,
        // so our next send_turn emits FQ and breaks the loop.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        server.extend_from_slice(b"FS N\r");
        server.extend_from_slice(b"FF\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Dial,
                locator: "CN87",
                intent: SessionIntent::Cms,
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );

        match result {
            Err(BackendError::MessageRejected(msg)) => {
                assert!(
                    msg.contains("REJECTME0001"),
                    "MessageRejected must contain the MID; got: {msg:?}"
                );
            }
            other => panic!("expected BackendError::MessageRejected, got {other:?}"),
        }
    }

    /// P1.4 (Codex post-impl review): in a mixed FS batch where one MID is
    /// accepted (FS Y) and another is rejected (FS N), the accepted MID must be
    /// moved to the Sent folder BEFORE `BackendError::MessageRejected` is returned.
    /// Without the fix, the early-return left accepted messages in the Outbox and
    /// they would be re-offered on the next connection (duplicate send).
    ///
    /// `fs::read_dir` enumeration order is not guaranteed, so we cannot assume
    /// which MID lands in `sent` vs `rejected`. Instead, the test asserts:
    ///   - exactly one MID ends up in `result.rejected` (the MessageRejected error)
    ///   - exactly one MID ends up in `Sent`
    ///   - they are different MIDs
    ///   - neither the sent MID nor the rejected MID remains in `Outbox`
    #[test]
    fn mixed_fs_batch_moves_accepted_mid_to_sent_before_returning_rejection_error() {
        use crate::winlink::message::Message as WMessage;

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // Two outbox messages. FS YN accepts whichever is enumerated first and
        // rejects whichever is enumerated second. We don't control the order.
        let mut msg1 = WMessage::new();
        msg1.set_header("Mid", "MIXED00000A");
        msg1.set_header("Subject", "Msg A");
        msg1.set_body(b"Body A.\r\n".to_vec());
        mailbox
            .store(MailboxFolder::Outbox, &msg1.to_bytes())
            .expect("store msg A");

        let mut msg2 = WMessage::new();
        msg2.set_header("Mid", "MIXED00000B");
        msg2.set_header("Subject", "Msg B");
        msg2.set_body(b"Body B.\r\n".to_vec());
        mailbox
            .store(MailboxFolder::Outbox, &msg2.to_bytes())
            .expect("store msg B");

        // Scripted gateway (Dial role): `FS YN` — first proposal accepted, second rejected.
        // Filesystem enumeration order determines which MID is "first"; both orderings
        // are valid inputs for this test — the property we check holds regardless.
        let mut server = Vec::new();
        server.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        server.extend_from_slice(b"FS YN\r");
        server.extend_from_slice(b"FF\r");

        let outbound_spy = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = FakeAx25Stream {
            inbound: std::io::Cursor::new(server),
            outbound: outbound_spy.clone(),
        };

        let result = native_packet_exchange(
            stream,
            PacketConnectCtx {
                base_mycall: "N7CPZ",
                targetcall: "W7AUX",
                password: None,
                role: ExchangeRole::Dial,
                locator: "CN87",
                intent: SessionIntent::Cms,
            },
            &mailbox,
            &|_| {},
            &|_| {},
        );

        // Must return Err(MessageRejected) containing exactly one MID.
        let rejected_mid = match result {
            Err(BackendError::MessageRejected(ref msg)) => {
                // Extract the one rejected MID from the error string.
                let candidates = ["MIXED00000A", "MIXED00000B"];
                let found: Vec<&str> = candidates
                    .iter()
                    .copied()
                    .filter(|m| msg.contains(m))
                    .collect();
                assert_eq!(
                    found.len(),
                    1,
                    "MessageRejected must name exactly one of our two MIDs; got: {msg:?}"
                );
                found[0].to_string()
            }
            other => panic!("expected BackendError::MessageRejected, got {other:?}"),
        };

        let accepted_mid = if rejected_mid == "MIXED00000A" {
            "MIXED00000B"
        } else {
            "MIXED00000A"
        };

        // The accepted MID must be in Sent — NOT left in Outbox.
        let sent = mailbox.list(MailboxFolder::Sent).unwrap();
        assert!(
            sent.iter().any(|m| m.id.0 == accepted_mid),
            "accepted MID ({accepted_mid}) must be in Sent folder; sent: {sent:?}"
        );
        let outbox = mailbox.list(MailboxFolder::Outbox).unwrap();
        assert!(
            !outbox.iter().any(|m| m.id.0 == accepted_mid),
            "accepted MID ({accepted_mid}) must NOT remain in Outbox; outbox: {outbox:?}"
        );

        // The rejected MID must NOT be in Sent.
        assert!(
            !sent.iter().any(|m| m.id.0 == rejected_mid),
            "rejected MID ({rejected_mid}) must NOT be in Sent folder; sent: {sent:?}"
        );
    }

    // =========================================================================
    // Task 6: packet lifecycle branch selection + no-link fast-fail
    // =========================================================================

    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn offline_config_with_callsign() -> Config {
        Config {
            elmer: crate::config::ElmerConfig::default(),
            p2p_limits: crate::contacts::limiter::P2pLimitsConfig::default(),
            ft8: crate::config::Ft8Config::default(),
            wwv_offair: None,
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::Telnet,
                host: crate::config::default_cms_host(),
            },
            identity: IdentityConfig {
                active_full: Some("N7CPZ".into()),
                identifier: None,
                grid: None,
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::Off,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
            active_connection: None,
            onboarding: Some(crate::config::OnboardingConfig::default()),
        }
    }

    #[tokio::test]
    async fn connect_packet_with_no_reachable_link_is_transport_failed() {
        // A NativeBackend with a callsign set but a KISS link that no listener is on.
        // connect_link fails fast (connection refused) → TransportFailed.
        // Per RADIO-1: we use a definitely-closed loopback port (bind then drop).
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening → connection refused

        let backend = NativeBackend::new(offline_config_with_callsign(), tempdir().unwrap().path());
        // Task 3.8: packet_connect_inner now gates on active_identity(); set one so
        // the test reaches the KISS connection-refused path it was designed to cover.
        backend.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ),
        ));
        let err = backend
            .connect(
                TransportConfig::Packet {
                    link: KissLinkConfig::Tcp {
                        host: addr.ip().to_string(),
                        port: addr.port(),
                    },
                    ssid: 7,
                    role: PacketRole::DialTo {
                        call: "W7AUX".into(),
                        path: vec![],
                    },
                    intent: SessionIntent::Cms,
                },
                None,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, BackendError::TransportFailed { .. }),
            "expected TransportFailed, got {err:?}"
        );
    }

    // tuxlink-ka7 / tuxlink-p5u regression guard (reduced scope after Task 3.8).
    //
    // Original coverage: proved that `set_config` refreshed the live config so the
    // NEXT packet connect honored the updated params without an app restart (the
    // stale-snapshot bug). Task 3.8 moved the packet path's base-call source from
    // `live_config().identity.active_full` to `active_identity()` (SessionIdentity),
    // and the `set_config(config_with_call(...))` call in this test became inert —
    // the updated identity.active_full no longer reaches the packet path, so the
    // stale-snapshot observable was lost.
    //
    // The live-config-refresh property for packet AX.25 params (`config.packet.params`,
    // read by `native_packet_connect`) cannot be exercised through a connection-refused
    // test because those params are only used AFTER the KISS link opens successfully.
    // Restoring that coverage requires a real (or faked) open KISS link to reach
    // `native_packet_connect`; a dedicated test for that is needed (tuxlink-0063 Phase 3
    // follow-up). For now this test is renamed to its actual reduced scope: proving
    // that `packet_connect_inner` requires an active SessionIdentity and reaches
    // link-open (TransportFailed) when one is set.
    #[tokio::test]
    async fn connect_requires_active_identity_before_packet_link_open() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener); // nothing listening → connection refused

        let backend = NativeBackend::new(offline_config(), tempdir().unwrap().path());
        // An active SessionIdentity is required; packet_connect_inner gates on it
        // before touching any KISS/TNC state.
        backend.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ),
        ));

        let err = backend
            .connect(
                TransportConfig::Packet {
                    link: KissLinkConfig::Tcp {
                        host: addr.ip().to_string(),
                        port: addr.port(),
                    },
                    ssid: 7,
                    role: PacketRole::DialTo {
                        call: "W7AUX".into(),
                        path: vec![],
                    },
                    intent: SessionIntent::Cms,
                },
                None,
            )
            .await
            .unwrap_err();

        assert!(
            !matches!(&err, BackendError::NoActiveIdentity),
            "connect must pass the identity gate (session was set); got {err:?}"
        );
        assert!(
            matches!(err, BackendError::TransportFailed { .. }),
            "with a live session, connect should reach link-open and fail \
             TransportFailed; got {err:?}"
        );
    }

    #[test]
    fn packet_dial_selects_dial_role_and_listen_selects_answer_role() {
        assert_eq!(
            resolve_packet_endpoint(
                "N7CPZ",
                7,
                PacketRole::DialTo {
                    call: "W7AUX".into(),
                    path: vec![]
                },
                SessionIntent::Cms,
            )
            .unwrap()
            .role,
            ExchangeRole::Dial
        );
        assert_eq!(
            resolve_packet_endpoint("N7CPZ", 7, PacketRole::Listen, SessionIntent::P2p)
                .unwrap()
                .role,
            ExchangeRole::Answer
        );
    }

    // tuxlink-orj: arming Listen must report Listening (armed, waiting for an
    // inbound call), NOT Connecting (which implies an active dial). This is the
    // honest-state fix — the prior code set Connecting for both roles, so the UI
    // refused to trust it and hard-coded "not connected".
    #[test]
    fn listen_role_initial_status_is_listening_not_connecting() {
        assert!(matches!(
            initial_packet_status(&PacketRole::Listen, 7),
            BackendStatus::Listening { transport } if transport == "Packet-7"
        ));
    }

    #[test]
    fn dial_role_initial_status_is_connecting() {
        assert!(matches!(
            initial_packet_status(
                &PacketRole::DialTo { call: "W7AUX".into(), path: vec![] },
                3
            ),
            BackendStatus::Connecting { transport } if transport == "Packet-3"
        ));
    }

    // =========================================================================
    // tuxlink-3wh: REAL end-to-end integration chain (no mocks, no RF).
    //
    // Two production NativeBackend instances connect to EACH OTHER over a real
    // TCP socket pair. One runs Listen (Answer role = FBB master), the other
    // DialTo (Dial role = slave/dialer). Every layer is the shipping code:
    // connect_link (real TcpStream) -> KISS framing -> AX.25 SABM/UA connect ->
    // Ax25Stream ARQ -> B2F run_exchange_with_role. The only non-tuxlink piece
    // is `kiss_wire`, a transparent byte relay that stands in for the
    // TNC->RF->TNC path (the TNC is transparent to AX.25 frames above the KISS
    // boundary, and RADIO-1 bars us from running the RF PHY anyway). 127.0.0.1
    // only; nothing is transmitted.
    // =========================================================================

    /// A transparent KISS byte-wire: accepts the two backends' TCP connections
    /// and cross-pipes their bytes, exactly as a TNC+RF+TNC link would carry the
    /// AX.25 frames between two hosts. Returns the address both peers dial.
    fn spawn_kiss_wire() -> std::net::SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let peer_a = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => return,
            };
            let peer_b = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => return,
            };
            let a_rd = peer_a.try_clone().unwrap();
            let mut a_wr = peer_a;
            let b_rd = peer_b.try_clone().unwrap();
            let mut b_wr = peer_b;
            let t1 = std::thread::spawn(move || {
                let mut r = a_rd;
                let _ = std::io::copy(&mut r, &mut b_wr);
            });
            let t2 = std::thread::spawn(move || {
                let mut r = b_rd;
                let _ = std::io::copy(&mut r, &mut a_wr);
            });
            let _ = t1.join();
            let _ = t2.join();
        });
        addr
    }

    fn config_with_call(call: &str) -> Config {
        let mut cfg = offline_config();
        cfg.identity.active_full = Some(call.to_string());
        cfg.identity.grid = Some("CN87".to_string());
        cfg
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn packet_two_real_peers_complete_a_connect_and_b2f_over_tcp_kiss() {
        let wire = spawn_kiss_wire();

        // Dialer (N7CPZ-7) has one outbound message; answerer (W7AUX-7) listens.
        let dialer_dir = tempdir().unwrap();
        let answerer_dir = tempdir().unwrap();
        let seed = Mailbox::new(dialer_dir.path());
        let raw = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "AX25-E2E",
            "hello over packet",
            1_716_200_000,
        )
        .to_bytes();
        seed.store(MailboxFolder::Outbox, &raw).unwrap();

        let dialer = NativeBackend::new(config_with_call("N7CPZ"), dialer_dir.path());
        // Task 3.8: packet_connect_inner now derives the base call from the active
        // SessionIdentity; both peers need one set or they return NoActiveIdentity.
        dialer.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ),
        ));
        // The answerer's listener gate (tuxlink-inde) defaults to "reject all"
        // — fresh tuxlink with no operator-curated allowlist rejects every
        // inbound peer. For this happy-path E2E test we inject an
        // allow_all=TRUE list so the dialer's N7CPZ-7 SABM is accepted.
        let answerer = NativeBackend::new(config_with_call("W7AUX"), answerer_dir.path())
            .with_packet_allowlist(
                crate::winlink::listener::AllowedStations::new().with_allow_all(true),
            );
        answerer.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("W7AUX").unwrap(),
            ),
        ));

        let listen = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::Listen,
            intent: SessionIntent::Cms,
        };
        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec![],
            },
            intent: SessionIntent::Cms,
        };

        // Watchdog: a handshake/connect deadlock must fail the test, not hang cargo.
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            tokio::join!(answerer.connect(listen, None), dialer.connect(dial, None))
        })
        .await;

        let (ans_res, dial_res) =
            outcome.expect("end-to-end packet exchange timed out (connect/handshake deadlock?)");
        ans_res.expect("answerer (Listen/Answer role) connect+exchange failed");
        dial_res.expect("dialer (DialTo/Dial role) connect+exchange failed");

        // The dialer's outbound message must have crossed the real TCP+KISS+AX.25
        // wire into the answerer's inbox (proves the full chain ran).
        let inbox = Mailbox::new(answerer_dir.path())
            .list(MailboxFolder::Inbox)
            .unwrap();
        assert_eq!(
            inbox.len(),
            1,
            "answerer inbox should hold the one message that crossed the wire; got {inbox:?}"
        );
        // ...and the dialer must have filed it as Sent (proves the proposal was acked).
        let sent = Mailbox::new(dialer_dir.path())
            .list(MailboxFolder::Sent)
            .unwrap();
        assert_eq!(
            sent.len(),
            1,
            "dialer Sent should hold the acked message; got {sent:?}"
        );
    }

    // A reader that mimics Ax25Stream's defect-J behaviour: it returns Ok(0) for
    // "no data yet" `idle` times (link open), then delivers `payload` once, then
    // reports closed so a further Ok(0) is a genuine EOF.
    struct IdleThenData {
        idle_left: usize,
        payload: Vec<u8>,
        delivered: bool,
    }
    impl std::io::Read for IdleThenData {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.idle_left > 0 {
                self.idle_left -= 1;
                return Ok(0); // no data yet — link still open
            }
            if !self.delivered {
                self.delivered = true;
                let n = buf.len().min(self.payload.len());
                buf[..n].copy_from_slice(&self.payload[..n]);
                return Ok(n);
            }
            Ok(0) // no more data; is_closed() is now true → genuine EOF
        }
    }
    impl MaybeClosed for IdleThenData {
        fn is_closed(&self) -> bool {
            self.delivered
        }
    }

    #[test]
    fn blocking_b2f_stream_loops_past_transient_ok0_then_reports_eof() {
        // Regression for the Codex 2026-05-22 BLOCKER: a transient Ok(0) from
        // Ax25Stream (no data yet, link open) must NOT be read as EOF by the B2F
        // BufReader; the adapter loops until real data, then surfaces a closed-link
        // Ok(0) as a genuine EOF.
        let mut s = BlockingB2fStream(IdleThenData {
            idle_left: 3,
            payload: b"FF\r".to_vec(),
            delivered: false,
        });
        let mut buf = [0u8; 8];
        let n = std::io::Read::read(&mut s, &mut buf).unwrap();
        assert_eq!(
            &buf[..n],
            b"FF\r",
            "must block through transient Ok(0), not EOF early"
        );
        let n2 = std::io::Read::read(&mut s, &mut buf).unwrap();
        assert_eq!(
            n2, 0,
            "Ok(0) while the link is closed must surface as a real EOF"
        );
    }

    #[test]
    fn active_identity_slot_starts_empty_and_round_trips() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        let backend = NativeBackend::new(offline_config(), tempfile::tempdir().unwrap().path());
        // Empty at construction -> NoActiveIdentity.
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));
        let handle: IdentityHandle = IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap());
        backend.set_active_identity(SessionIdentity::full(handle));
        let active = backend.active_identity().expect("active set");
        assert_eq!(active.mycall().as_str(), "N7CPZ");
    }

    #[test]
    fn captured_identity_is_immune_to_later_active_switch() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        let backend = NativeBackend::new(offline_config(), tempfile::tempdir().unwrap().path());
        backend.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("W1AAA").unwrap(),
        )));
        // Simulate a listener capturing the active identity at arm time (Clone).
        let captured = backend.active_identity().expect("active set").clone();
        // Operator switches the active identity (default for NEW ops).
        backend.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("W2BBB").unwrap(),
        )));
        assert_eq!(
            backend.active_identity().unwrap().mycall().as_str(),
            "W2BBB",
            "active switched"
        );
        assert_eq!(
            captured.mycall().as_str(),
            "W1AAA",
            "a captured identity is immune to active switches"
        );
        // And clear() restores the re-auth requirement.
        backend.clear_active_identity();
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));
    }

    #[test]
    fn session_identity_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<crate::identity::SessionIdentity>();
    }

    // =========================================================================
    // Task 3.6 (tuxlink-0063): run_ardop_b2f_exchange derives the on-air
    // station ID (ExchangeConfig.mycall → the ;FW: callsign sent on the data
    // stream) from the SessionIdentity it is handed, NOT from
    // `config.identity.active_full`.
    //
    // Config callsign: W7AUX. Active session: N7CPZ.
    // The ;FW: line in the client's handshake MUST carry N7CPZ, not W7AUX.
    //
    // A minimal ScriptedDuplex implements ModemTransport + ReadWrite so the
    // B2F engine can run end-to-end in memory (no TCP, no ardopcf).
    // =========================================================================
    #[test]
    fn ardop_b2f_exchange_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use crate::native_mailbox::Mailbox;
        use crate::winlink::modem::ardop::session::{ConnectInfo, InitConfig, SessionError};
        use crate::winlink::modem::{ModemTransport, ReadWrite};
        use std::io::{Cursor, Read, Write};
        use std::time::Duration;

        // ── scripted duplex: reads return a minimal CMS handshake; writes are
        //    captured so we can inspect the callsign the B2F engine announced.
        struct ScriptedDuplex {
            reader: Cursor<Vec<u8>>,
            captured: Vec<u8>,
        }
        impl Read for ScriptedDuplex {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.reader.read(buf)
            }
        }
        impl Write for ScriptedDuplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.captured.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        struct ScriptedTransport {
            duplex: ScriptedDuplex,
        }
        impl ModemTransport for ScriptedTransport {
            fn init(&mut self, _: &InitConfig) -> Result<(), SessionError> {
                Ok(())
            }
            fn connect_arq(
                &mut self,
                _: &str,
                _: u32,
                _: Option<Duration>,
            ) -> Result<ConnectInfo, SessionError> {
                Ok(ConnectInfo {
                    peer_call: "W7RMS-10".into(),
                    bandwidth_hz: 500,
                })
            }
            fn disconnect(&mut self, _: Duration) -> Result<(), SessionError> {
                Ok(())
            }
            fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
                Ok(&mut self.duplex)
            }
        }

        // Scripted "CMS" server: empty handshake + no messages.
        let mut script = Vec::new();
        script.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        script.extend_from_slice(b"FF\r"); // remote has nothing

        let mut transport = ScriptedTransport {
            duplex: ScriptedDuplex {
                reader: Cursor::new(script),
                captured: Vec::new(),
            },
        };

        // Config callsign W7AUX; active session authenticates N7CPZ.
        let mut cfg = offline_config();
        cfg.identity.active_full = Some("W7AUX".into());
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // Call run_ardop_b2f_exchange — after the Task 3.6 signature change,
        // session_id is the parameter immediately after config (5th positional).
        let _ = run_ardop_b2f_exchange(
            &mut transport,
            "W7RMS-10",
            crate::winlink::session::SessionIntent::Cms,
            &cfg,
            &session_id,
            &mailbox,
            None,
            None,
        );

        // The B2F slave's opening handshake sends ";FW: <mycall>\r".
        // It MUST be N7CPZ (from the session), NOT W7AUX (from the config).
        let written = String::from_utf8_lossy(&transport.duplex.captured);
        assert!(
            written.contains(";FW: N7CPZ"),
            "run_ardop_b2f_exchange must use the session mycall N7CPZ in ;FW:; got:\n{written}"
        );
        assert!(
            !written.contains(";FW: W7AUX"),
            "run_ardop_b2f_exchange must NOT use the config callsign W7AUX in ;FW:; got:\n{written}"
        );
    }

    // =========================================================================
    // Task 3.6 (tuxlink-0063): run_ardop_b2f_answer derives the on-air station
    // ID from the SessionIdentity, NOT from `config.identity.active_full`.
    //
    // The answerer is the B2F MASTER (ISS): it speaks FIRST, sending the master
    // handshake which carries `;FW: {mycall}`. That handshake is written to the
    // captured byte stream BEFORE any data from the scripted peer is consumed, so
    // the assertion is straightforward.
    //
    // Config callsign: W7AUX. Active session: N7CPZ.
    // The `;FW:` line in the master handshake MUST carry N7CPZ, not W7AUX.
    // =========================================================================
    #[test]
    fn run_ardop_b2f_answer_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use crate::native_mailbox::Mailbox;
        use crate::winlink::modem::ardop::session::{ConnectInfo, InitConfig, SessionError};
        use crate::winlink::modem::{ModemTransport, ReadWrite};
        use std::io::{Cursor, Read, Write};
        use std::time::Duration;

        // ── scripted duplex: reads return a minimal slave (peer/dialer) handshake
        //    followed by FF (no messages); writes are captured to inspect the
        //    callsign the answerer (master) announced.
        struct ScriptedDuplex {
            reader: Cursor<Vec<u8>>,
            captured: Vec<u8>,
        }
        impl Read for ScriptedDuplex {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.reader.read(buf)
            }
        }
        impl Write for ScriptedDuplex {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.captured.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        struct ScriptedTransport {
            duplex: ScriptedDuplex,
        }
        impl ModemTransport for ScriptedTransport {
            fn init(&mut self, _: &InitConfig) -> Result<(), SessionError> {
                Ok(())
            }
            fn connect_arq(
                &mut self,
                _: &str,
                _: u32,
                _: Option<Duration>,
            ) -> Result<ConnectInfo, SessionError> {
                Ok(ConnectInfo {
                    peer_call: "W7AUX".into(),
                    bandwidth_hz: 500,
                })
            }
            fn disconnect(&mut self, _: Duration) -> Result<(), SessionError> {
                Ok(())
            }
            fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
                Ok(&mut self.duplex)
            }
        }

        // Scripted peer (slave/dialer): its handshake reply + empty turn.
        // The peer is "W7AUX"; the answerer (us) is N7CPZ.
        // Slave handshake: forwarding line + SID + DE line (no `>` prompt).
        let mut script = Vec::new();
        script.extend_from_slice(b";FW: W7AUX\r[RMS-1.0-B2FHM$]\r; N7CPZ DE W7AUX (CN87)\r");
        // Slave takes first message turn with nothing to send.
        script.extend_from_slice(b"FF\r");

        let mut transport = ScriptedTransport {
            duplex: ScriptedDuplex {
                reader: Cursor::new(script),
                captured: Vec::new(),
            },
        };

        // Config callsign W7AUX; active session authenticates N7CPZ.
        let mut cfg = offline_config();
        cfg.identity.active_full = Some("W7AUX".into());
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // Call run_ardop_b2f_answer — session_id is the parameter immediately
        // after config (4th positional), immediately after peer_callsign.
        let _ = run_ardop_b2f_answer(
            &mut transport,
            "W7AUX",
            &cfg,
            &session_id,
            &mailbox,
            None,
            None,
        );

        // The B2F master's opening handshake sends ";FW: <mycall>\r" as the
        // FIRST bytes on the wire (before reading anything from the peer).
        // It MUST be N7CPZ (from the session), NOT W7AUX (from the config).
        let written = String::from_utf8_lossy(&transport.duplex.captured);
        assert!(
            written.contains(";FW: N7CPZ"),
            "run_ardop_b2f_answer must use the session mycall N7CPZ in ;FW:; got:\n{written}"
        );
        assert!(
            !written.contains(";FW: W7AUX"),
            "run_ardop_b2f_answer must NOT use the config callsign W7AUX in ;FW:; got:\n{written}"
        );
    }

    // =========================================================================
    // Task 3.7 (tuxlink-0063): run_vara_b2f_exchange derives the on-air station
    // ID (ExchangeConfig.mycall → the ;FW: callsign sent on the data stream)
    // from the SessionIdentity it is handed, NOT from `config.identity.active_full`.
    //
    // Config callsign: W7AUX. Active session: N7CPZ.
    // The ;FW: line in the client's B2F handshake MUST carry N7CPZ, not W7AUX.
    //
    // VARA uses real TcpStreams (try_clone() requires OS handles), so we spin a
    // real loopback listener pair. The server thread on the data port plays the
    // CMS slave role: it sends a minimal B2F master handshake and awaits the
    // client's ;FW: line. The cmd port acceptor just holds the connection.
    // Both captured bytes (written by the client) and the session assert are
    // confirmed.
    // =========================================================================
    #[test]
    fn run_vara_b2f_exchange_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use crate::native_mailbox::Mailbox;
        use crate::winlink::modem::vara::transport::{VaraConfig, VaraTransport};
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        // ── Bind loopback listeners for cmd + data ports.
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        // ── Capture bytes the client writes to the data socket.
        let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        // ── cmd acceptor: just holds the connection alive for the transport.
        let cmd_handle = thread::spawn(move || {
            let (_sock, _) = cmd_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });

        // ── data acceptor: acts as B2F slave (IRS/responder).
        //    Sends a minimal WL2K-5.0 header so the client handshake proceeds;
        //    then records everything the client writes before closing.
        let data_handle = {
            let captured = captured.clone();
            thread::spawn(move || {
                let (sock, _) = data_l.accept().unwrap();
                // Per-read timeout is short, but we DRAIN ACROSS TIMEOUTS up to a
                // generous total deadline. Under full-suite CPU contention the
                // client's handshake (the ;FW: line we assert on) can arrive well
                // after connect; a break-on-first-timeout drain races and flakes
                // (observed in the tuxlink-0063 Phase 3 full-suite run). We stop as
                // soon as a complete ;FW: line is captured, on real EOF, or at the
                // deadline backstop.
                sock.set_read_timeout(Some(Duration::from_millis(200))).ok();
                let mut writer = sock.try_clone().unwrap();
                // Send the WL2K-5.0 server greeting so B2F exchange starts.
                writer.write_all(b"[WL2K-5.0-B2FHM$]\rCMS>\r").unwrap();
                writer.write_all(b"FF\r").unwrap();

                // Drain client output into captured.
                let mut reader = BufReader::new(sock);
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                loop {
                    let mut line = Vec::new();
                    match reader.read_until(b'\r', &mut line) {
                        Ok(0) => break, // client closed the socket
                        Ok(_) => {
                            let got_fw = line.windows(4).any(|w| w == b";FW:");
                            captured.lock().unwrap().extend_from_slice(&line);
                            // The complete ;FW: line is captured — that is all the
                            // assertion needs; stop deterministically.
                            if got_fw {
                                break;
                            }
                        }
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            if std::time::Instant::now() >= deadline {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        // ── Connect a VaraTransport to our scripted listeners.
        let cfg_vara = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(1000)),
            data_read_timeout: Some(Duration::from_millis(1000)),
        };
        let mut transport = VaraTransport::connect(cfg_vara).expect("loopback connect");

        // ── Config callsign W7AUX; active session authenticates N7CPZ.
        let mut cfg = offline_config();
        cfg.identity.active_full = Some("W7AUX".into());
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // ── Call run_vara_b2f_exchange — the Task 3.7 signature change adds
        //    session_id immediately after config (5th positional arg).
        let _ = run_vara_b2f_exchange(
            &mut transport,
            "W7RMS-10",
            crate::winlink::session::SessionIntent::Cms,
            &cfg,
            &session_id,
            &mailbox,
            None,
            None,
        );

        cmd_handle.join().ok();
        data_handle.join().ok();

        // ── Assert: ;FW: must carry N7CPZ (session call), not W7AUX (config call).
        let binding = captured.lock().unwrap();
        let written = String::from_utf8_lossy(&binding);
        assert!(
            written.contains(";FW: N7CPZ"),
            "run_vara_b2f_exchange must use the session mycall N7CPZ in ;FW:; got:\n{written}"
        );
        assert!(
            !written.contains(";FW: W7AUX"),
            "run_vara_b2f_exchange must NOT use the config callsign W7AUX in ;FW:; got:\n{written}"
        );
    }

    // =========================================================================
    // Task 3.7 (tuxlink-0063): run_vara_b2f_answer derives the on-air station
    // ID from the SessionIdentity, NOT from `config.identity.active_full`.
    //
    // The answerer is the B2F MASTER (ISS): it speaks FIRST, sending the master
    // handshake which carries `;FW: {mycall}`. That handshake is written to the
    // data socket BEFORE any data from the scripted peer is consumed, so the
    // assertion is straightforward.
    //
    // Config callsign: W7AUX. Active session: N7CPZ.
    // The `;FW:` line in the master handshake MUST carry N7CPZ, not W7AUX.
    // =========================================================================
    #[test]
    fn run_vara_b2f_answer_mycall_comes_from_session_not_config() {
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};
        use crate::native_mailbox::Mailbox;
        use crate::winlink::modem::vara::transport::{VaraConfig, VaraTransport};
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        // ── Bind loopback listeners for cmd + data ports.
        let cmd_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let cmd_port = cmd_l.local_addr().unwrap().port();
        let data_l = TcpListener::bind("127.0.0.1:0").unwrap();
        let data_port = data_l.local_addr().unwrap().port();

        // ── Capture bytes the answerer (master/ISS) writes to the data socket.
        let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        // ── cmd acceptor: holds the connection for the transport lifetime.
        let cmd_handle = thread::spawn(move || {
            let (_sock, _) = cmd_l.accept().unwrap();
            thread::sleep(Duration::from_millis(500));
        });

        // ── data acceptor: acts as B2F slave (IRS/dialer) in the exchange.
        //    Waits for the master's ;FW: greeting, replies with minimal slave
        //    handshake, then sends FF (nothing to send). All bytes written BY
        //    the client (the answerer/master) are captured for assertion.
        let data_handle = {
            let captured = captured.clone();
            thread::spawn(move || {
                let (sock, _) = data_l.accept().unwrap();
                // Drain across read timeouts up to a generous deadline — under
                // full-suite CPU contention the master's preamble lines can arrive
                // slower than a single short read window, so breaking on the first
                // timeout would race (same latent flake the exchange test hit).
                sock.set_read_timeout(Some(Duration::from_millis(200))).ok();
                let mut writer = sock.try_clone().unwrap();

                // Capture everything the master writes, and reply with the
                // slave handshake once the preamble is in so the master can proceed.
                let mut reader = BufReader::new(sock);
                let mut line_count = 0usize;
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                loop {
                    let mut line = Vec::new();
                    match reader.read_until(b'\r', &mut line) {
                        Ok(0) => break, // client closed the socket
                        Ok(_) => {
                            captured.lock().unwrap().extend_from_slice(&line);
                            line_count += 1;
                            // After the master's 3-line preamble (;FW:, SID, DE line)
                            // send the slave greeting so the exchange can conclude.
                            if line_count == 3 {
                                // Slave: ;FW: peer + SID + DE greeting + first FF turn.
                                let _ = writer.write_all(
                                    b";FW: W7AUX\r[RMS-1.0-B2FHM$]\r; N7CPZ DE W7AUX\rFF\r",
                                );
                                let _ = writer.flush();
                                // The master's ;FW: (line 1) is captured and the slave
                                // reply is sent; give the master a beat to consume it,
                                // then stop deterministically.
                                thread::sleep(Duration::from_millis(100));
                                break;
                            }
                        }
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            if std::time::Instant::now() >= deadline {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        // ── Connect a VaraTransport to our scripted listeners.
        let cfg_vara = VaraConfig {
            host: "127.0.0.1".into(),
            cmd_port,
            data_port,
            connect_timeout: Duration::from_secs(2),
            read_timeout: Some(Duration::from_millis(1000)),
            data_read_timeout: Some(Duration::from_millis(1000)),
        };
        let mut transport = VaraTransport::connect(cfg_vara).expect("loopback connect");

        // ── Config callsign W7AUX; active session authenticates N7CPZ.
        let mut cfg = offline_config();
        cfg.identity.active_full = Some("W7AUX".into());
        let session_id =
            SessionIdentity::full(IdentityHandle::for_test(Callsign::parse("N7CPZ").unwrap()));

        let dir = tempdir().unwrap();
        let mailbox = Mailbox::new(dir.path());

        // ── Call run_vara_b2f_answer — session_id is the parameter immediately
        //    after config (4th positional), immediately before mailbox.
        let _ = run_vara_b2f_answer(
            &mut transport,
            "W7AUX",
            &cfg,
            &session_id,
            &mailbox,
            None,
            None,
        );

        cmd_handle.join().ok();
        data_handle.join().ok();

        // ── The B2F master's opening handshake sends ";FW: <mycall>\r" as the
        //    FIRST bytes on the wire. It MUST be N7CPZ (session), not W7AUX (config).
        let binding = captured.lock().unwrap();
        let written = String::from_utf8_lossy(&binding);
        assert!(
            written.contains(";FW: N7CPZ"),
            "run_vara_b2f_answer must use the session mycall N7CPZ in ;FW:; got:\n{written}"
        );
        assert!(
            !written.contains(";FW: W7AUX"),
            "run_vara_b2f_answer must NOT use the config callsign W7AUX in ;FW:; got:\n{written}"
        );
    }

    // =========================================================================
    // Task 15 (tuxlink-c39af): packet record sites (dial + answer)
    // =========================================================================

    /// A KISS TCP stand-in that accepts the connection and IMMEDIATELY closes
    /// it. The client's TCP open succeeds (so `connect_link_with_abort` — which
    /// runs BEFORE the record guard arms — passes and the guard IS armed), then
    /// the very next link read inside `datalink::connect`'s poll loop sees the
    /// FIN: `recv_frame` maps a 0-byte read to `Err(ConnectionAborted)`
    /// (datalink.rs "link closed" arm), which `connect`'s `recv_frame()?`
    /// propagates on the FIRST poll iteration. That makes the AX.25 connect
    /// failure DETERMINISTIC and sub-second — no reliance on retry timing.
    ///
    /// Why not a silent (accept-and-sink) wire: `datalink::connect` listens for
    /// a slow UA until `params.connect_timeout`, and
    /// `Ax25ParamsConfig::into_params` hard-codes that ceiling to the 25 s
    /// `Ax25Params::default()` (a RADIO-1 safety default, deliberately not
    /// config-tunable) — `t1_ms`/`n2_retries` only pace the ≤2 SABMs, they do
    /// NOT end the wait. A silent wire therefore always costs the full 25 s,
    /// which blew past the test timeout on a loaded CI runner
    /// (run 29129290795, both arches).
    fn spawn_closing_kiss_wire() -> std::net::SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((sock, _)) = listener.accept() {
                // Drop immediately → FIN to the client. Nothing is ever read
                // or written on this side.
                drop(sock);
            }
        });
        addr
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn packet_answer_p2p_intent_records_incoming_accepted_observation() {
        // [CDX-7]: drives the REAL `native_packet_connect` answer path — via
        // `NativeBackend::connect` → `packet_connect_inner` → `spawn_blocking` —
        // over a genuine TCP+KISS+AX.25 loopback wire (mirrors the Task 12
        // `packet_two_real_peers_complete_a_connect_and_b2f_over_tcp_kiss`
        // fixture), never a hand-constructed `ObservationGuard`. #[serial]:
        // the recorder sink is process-global.
        use crate::identity::{Callsign, IdentityHandle, SessionIdentity};

        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }

        let wire = spawn_kiss_wire();

        let dialer_dir = tempdir().unwrap();
        let answerer_dir = tempdir().unwrap();
        let seed = Mailbox::new(dialer_dir.path());
        let raw = compose_message(
            "N7CPZ",
            &["W7AUX"],
            &[],
            "P2P record site",
            "task 15 observation",
            1_716_200_000,
        )
        .to_bytes();
        seed.store(MailboxFolder::Outbox, &raw).unwrap();

        let dialer = NativeBackend::new(config_with_call("N7CPZ"), dialer_dir.path());
        dialer.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("N7CPZ").unwrap(),
        )));
        let answerer = NativeBackend::new(config_with_call("W7AUX"), answerer_dir.path())
            .with_packet_allowlist(
                crate::winlink::listener::AllowedStations::new().with_allow_all(true),
            );
        answerer.set_active_identity(SessionIdentity::full(IdentityHandle::for_test(
            Callsign::parse("W7AUX").unwrap(),
        )));

        let listen = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::Listen,
            intent: SessionIntent::P2p,
        };
        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec![],
            },
            intent: SessionIntent::P2p,
        };

        // 60 s: the full loopback exchange completes in ~1-2 s locally; the
        // margin covers spawn_blocking scheduling on a loaded 2-core CI runner
        // (the dial-fail twins' original 10 s wrapper proved too tight there).
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            tokio::join!(answerer.connect(listen, None), dialer.connect(dial, None))
        })
        .await;
        let (ans_res, dial_res) =
            outcome.expect("packet P2P answer record-site test timed out");
        ans_res.expect("answerer (Listen/Answer role) connect+exchange failed");
        dial_res.expect("dialer (DialTo/Dial role) connect+exchange failed");

        let obs = seen.lock().unwrap();
        let incoming: Vec<_> = obs
            .iter()
            .filter(|o| o.direction == crate::contacts::reachability::Direction::Incoming)
            .collect();
        assert_eq!(
            incoming.len(),
            1,
            "exactly one Incoming observation from the answer arm; got {obs:?}"
        );
        let o = incoming[0];
        match &o.path {
            crate::contacts::observation::ObservedPath::Rf { transport, .. } => {
                assert_eq!(*transport, crate::contacts::reachability::ChannelTransport::Packet);
            }
            other => panic!("expected an Rf path for the packet answer site; got {other:?}"),
        }
        assert_eq!(
            o.presented_target, "N7CPZ",
            "the answerer's observation must present the dialer's base call"
        );
        assert_eq!(
            crate::contacts::observation::classify(o.phase),
            crate::contacts::observation::Classified::Ok,
            "a clean accepted exchange must classify Ok; got phase {:?}",
            o.phase
        );
        drop(obs);

        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn packet_dial_connect_fail_p2p_intent_records_one_fail_observation() {
        // [CDX-7] [R3-11]: drives the REAL `native_packet_connect` dial path
        // (via `NativeBackend::connect`) against a KISS wire that accepts the
        // TCP link and immediately closes it — the FIN surfaces as
        // `ConnectionAborted` on `datalink::connect`'s first poll, the dial
        // arm's `?` fires, and the guard armed immediately before that call
        // drops recording `DialAttempted` → Fail. Deterministic sub-second
        // failure (see `spawn_closing_kiss_wire` for why a silent wire is NOT
        // usable: the 25 s non-configurable `connect_timeout` governs that
        // path). #[serial]: the recorder sink is process-global.
        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }

        let wire = spawn_closing_kiss_wire();

        let dir = tempdir().unwrap();
        let dialer = NativeBackend::new(config_with_call("N7CPZ"), dir.path());
        dialer.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ),
        ));

        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec![],
            },
            intent: SessionIntent::P2p,
        };

        // Expected wall clock is sub-second (FIN on the first poll); 60 s gives
        // order-of-magnitude headroom for a loaded 2-core CI runner and would
        // even survive a regression back to the 25 s connect_timeout ceiling.
        let result = tokio::time::timeout(std::time::Duration::from_secs(60), dialer.connect(dial, None))
            .await
            .expect("packet dial-fail record-site test timed out");
        assert!(
            result.is_err(),
            "a dial into an immediately-closed KISS wire must fail the AX.25 connect, got {result:?}"
        );

        let obs = seen.lock().unwrap();
        assert_eq!(
            obs.len(),
            1,
            "exactly one observation for the failed P2P dial; got {obs:?}"
        );
        assert_eq!(obs[0].direction, crate::contacts::reachability::Direction::Outgoing);
        assert_eq!(obs[0].presented_target, "W7AUX");
        assert_eq!(
            crate::contacts::observation::classify(obs[0].phase),
            crate::contacts::observation::Classified::Fail
        );
        drop(obs);

        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn packet_dial_connect_fail_cms_intent_records_nothing_even_with_sink_installed() {
        // [spec §3]: the packet dial gate mirrors the VARA/ARDOP dial gates —
        // only `SessionIntent::P2p` dials resolve the global sink. A CMS dial
        // that fails its AX.25 connect must not touch the peer roster even
        // though the global sink IS installed. Same deterministic
        // accept-then-close wire as the P2p twin (sub-second failure).
        let seen: Arc<std::sync::Mutex<Vec<crate::contacts::observation::PeerObservation>>> =
            Arc::default();
        {
            let seen = seen.clone();
            crate::contacts::observation::install_observation_sink(Arc::new(move |o| {
                seen.lock().unwrap().push(o)
            }));
        }

        let wire = spawn_closing_kiss_wire();

        let dir = tempdir().unwrap();
        let dialer = NativeBackend::new(config_with_call("N7CPZ"), dir.path());
        dialer.set_active_identity(crate::identity::SessionIdentity::full(
            crate::identity::IdentityHandle::for_test(
                crate::identity::Callsign::parse("N7CPZ").unwrap(),
            ),
        ));

        let dial = TransportConfig::Packet {
            link: KissLinkConfig::Tcp {
                host: wire.ip().to_string(),
                port: wire.port(),
            },
            ssid: 7,
            role: PacketRole::DialTo {
                call: "W7AUX-7".into(),
                path: vec![],
            },
            intent: SessionIntent::Cms,
        };

        // Same 60 s order-of-magnitude margin as the P2p twin.
        let result = tokio::time::timeout(std::time::Duration::from_secs(60), dialer.connect(dial, None))
            .await
            .expect("packet dial-fail CMS-intent test timed out");
        assert!(result.is_err(), "expected the AX.25 connect to fail, got {result:?}");

        assert!(
            seen.lock().unwrap().is_empty(),
            "a CMS packet dial must not record even with the global sink installed"
        );

        crate::contacts::observation::install_observation_sink(Arc::new(|_| {})); // reset
    }
}
