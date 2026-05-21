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

use std::io::{BufRead, Write};

use super::message::{self, Message};
use super::proposal::{self, Answer, Proposal};
use super::{handshake, lzhuf, secure, transfer, wire};

/// At most this many proposals are offered in a single batch.
const MAX_BATCH: usize = 5;

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
#[derive(Debug, Clone)]
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
}

/// The result of a whole exchange.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExchangeResult {
    pub received: Vec<Message>,
    pub sent: Vec<String>,
    pub rejected: Vec<String>,
    pub deferred: Vec<String>,
}

/// Run a full exchange over an already-connected transport: read the server's
/// handshake, answer it (with a secure-login token if challenged), then
/// alternate turns until either side quits.
///
/// The transport is split into a reader and a writer so this is exercised with
/// scripted in-memory streams; the telnet layer supplies a TCP socket. The
/// client speaks second in the handshake but takes the first message turn, which
/// is why an empty mailbox first sends `FF` and only later `FQ`.
pub fn run_exchange<R, W, F>(
    reader: &mut R,
    writer: &mut W,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, ExchangeError>
where
    R: BufRead,
    W: Write,
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    // The server speaks first.
    let remote = handshake::read_remote_handshake(reader).map_err(ExchangeError::Handshake)?;
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
    write_bytes(writer, &our_handshake)?;

    let mut result = ExchangeResult::default();
    let mut remaining = outbound;
    let mut remote_no_messages = false;
    let mut my_turn = true; // the client takes the first message turn

    loop {
        if my_turn {
            let outcome = send_turn(reader, writer, &remaining, remote_no_messages)?;
            result.sent.extend(outcome.sent);
            result.rejected.extend(outcome.rejected);
            result.deferred.extend(outcome.deferred);
            remaining.clear(); // each message is offered once
            if outcome.quit_sent {
                break;
            }
        } else {
            let outcome = receive_turn(reader, writer, &decide)?;
            result.received.extend(outcome.messages);
            remote_no_messages = outcome.remote_no_messages;
            if outcome.remote_quit {
                break;
            }
        }
        my_turn = !my_turn;
    }
    Ok(result)
}

/// Our turn: offer the pending messages, read the answers, send the accepted
/// bodies. With nothing to send, signal "no more" (or "quit" if the other side
/// was also done).
pub fn send_turn<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    outbound: &[OutboundMessage],
    remote_no_messages: bool,
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
        write_bytes(writer, proposal.line().as_bytes())?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::message::Message;
    use crate::winlink::proposal::{batch_checksum_line, Answer};
    use crate::winlink::transfer;
    use std::io::Cursor;

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
        let outcome = send_turn(&mut reader, &mut writer, &[], false).unwrap();
        assert_eq!(writer, b"FF\r");
        assert!(!outcome.quit_sent);
    }

    #[test]
    fn with_nothing_to_send_and_the_other_side_done_we_quit() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        let mut writer = Vec::<u8>::new();
        let outcome = send_turn(&mut reader, &mut writer, &[], true).unwrap();
        assert_eq!(writer, b"FQ\r");
        assert!(outcome.quit_sent);
    }

    #[test]
    fn an_accepted_proposal_is_offered_then_its_body_is_sent() {
        let (out, compressed) = outbound_message("OUTBOUND0001", "Test", b"hello");
        let proposal = out.proposal.clone();

        let mut reader = Cursor::new(b"FS Y\r".to_vec());
        let mut writer = Vec::new();
        let outcome = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false).unwrap();

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
        let outcome = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false).unwrap();

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
        };
        let result = run_exchange(&mut reader, &mut writer, &config, vec![], |_| vec![]).unwrap();

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
        };
        let result = run_exchange(&mut reader, &mut writer, &config, vec![], |_| {
            vec![Answer::Accept { resume_offset: 0 }]
        })
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
        };
        assert_eq!(
            run_exchange(&mut reader, &mut writer, &config, vec![], |_| vec![]),
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
        let result = send_turn(&mut reader, &mut writer, std::slice::from_ref(&out), false);
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
}
