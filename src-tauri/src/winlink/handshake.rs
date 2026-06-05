//! The B2F handshake: who we are, what we support, and the secure-login answer.
//!
//! When a client connects to a Winlink CMS over telnet, the server speaks first
//! — it sends its identifier line `[NAME-VERSION-CODES]`, an optional password
//! challenge `;PQ: <challenge>`, and a prompt ending in `>`. The client then
//! replies with its forwarding line `;FW: <call>`, its own identifier line, an
//! optional secure-login response `;PR: <token>`, and a closing
//! `; <target> DE <mycall> (<locator>)` line.
//!
//! Mirrors `wl2k-go/fbb/handshake.go`. The SID codes we advertise are `B2FHM$`:
//! B2 compressed forwarding, FBB basic, hierarchical locators, message-id, and
//! BID support (the `$` must come last).

use std::io::BufRead;

use super::wire;

/// The application name we put in our identifier line. Must contain no dash.
const APP_NAME: &str = "tuxlink";
/// The protocol features we advertise (B2 + basic + locators + message-id +
/// BID). The `$` must be last.
const SID_CODES: &str = "B2FHM$";

/// What the server told us during the handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteHandshake {
    /// The feature codes from the server's identifier line, e.g. `B2FWIHJM$`.
    pub sid: String,
    /// Call signs the server forwards on behalf of (often empty for a CMS).
    pub forwarders: Vec<String>,
    /// The password challenge, if the server asked for a secure login.
    pub challenge: Option<String>,
}

/// Build the client's half of the handshake (the bytes to send after reading
/// the server's). Pass the secure-login response if the server sent a challenge.
pub fn build_handshake(
    mycall: &str,
    targetcall: &str,
    locator: &str,
    secure_response: Option<&str>,
) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&format!(";FW: {mycall}\r"));
    out.push_str(&format!(
        "[{APP_NAME}-{}-{SID_CODES}]\r",
        env!("CARGO_PKG_VERSION")
    ));
    if let Some(response) = secure_response {
        out.push_str(&format!(";PR: {response}\r"));
    }
    out.push_str(&format!("; {targetcall} DE {mycall} ({locator})\r"));
    out.into_bytes()
}

/// Build the *master's* half of the handshake (the answering/listening station).
///
/// Mirrors `wl2k-go/fbb/handshake.go` `sendHandshake` in its master branch: the
/// master's handshake is identical to the client's EXCEPT it appends the FBB
/// prompt `>` to the final `DE` line — that prompt is what tells the dialing
/// slave the master's handshake is complete. A P2P master never challenges
/// (`;PQ`) and never answers one (`;PR`), so neither appears here. Without the
/// trailing `>`, a dialing peer's `read_remote_handshake` never terminates
/// (the bug a two-real-peer end-to-end test surfaced — tuxlink-3wh).
pub fn build_master_handshake(mycall: &str, targetcall: &str, locator: &str) -> Vec<u8> {
    let mut out = String::new();
    out.push_str(&format!(";FW: {mycall}\r"));
    out.push_str(&format!(
        "[{APP_NAME}-{}-{SID_CODES}]\r",
        env!("CARGO_PKG_VERSION")
    ));
    out.push_str(&format!("; {targetcall} DE {mycall} ({locator})>\r"));
    out.into_bytes()
}

/// Read the server/master's handshake lines until its prompt, gathering the
/// identifier, any forwarders, and any password challenge. Used by the **slave**
/// (dialer) role, which always sees a prompt-terminated master handshake.
pub fn read_remote_handshake<R: BufRead>(
    reader: &mut R,
) -> Result<RemoteHandshake, HandshakeError> {
    read_handshake(reader, false)
}

/// Read the **slave's** (dialer's) handshake — used by the master/answerer role.
///
/// A slave's handshake carries no `>` prompt (see [`build_handshake`]); per
/// `wl2k-go/fbb/handshake.go` `readHandshake`, the master detects the end of the
/// slave's handshake by peeking the next line: when it begins with `F` the
/// slave's *message turn* has started (`FA`/`FB`/`FC`/`FF`/`FQ`), so the
/// handshake is done and that line is left unconsumed for the turn loop. A `>`
/// prompt still terminates too, for peers that send one.
pub fn read_slave_handshake<R: BufRead>(
    reader: &mut R,
) -> Result<RemoteHandshake, HandshakeError> {
    read_handshake(reader, true)
}

fn read_handshake<R: BufRead>(
    reader: &mut R,
    master: bool,
) -> Result<RemoteHandshake, HandshakeError> {
    let mut sid: Option<String> = None;
    let mut forwarders = Vec::new();
    let mut challenge = None;

    loop {
        if master {
            // Peek (don't consume): an `F`-prefixed line is the slave's first
            // protocol command — its handshake is over (wl2k-go readHandshake).
            // B2F over packet is CR-only, but tolerate a stray LF left by a CRLF
            // link so framing residue can't mask the `F` peek (Codex 2026-05-22).
            match reader.fill_buf() {
                Ok([]) => return Err(HandshakeError::ConnectionClosed),
                Ok(buf) if buf[0] == b'\n' => {
                    reader.consume(1);
                    continue;
                }
                Ok(buf) if buf[0] == b'F' => break,
                Ok(_) => {}
                Err(_) => return Err(HandshakeError::ConnectionClosed),
            }
        }
        let line = wire::read_line(reader).map_err(|_| HandshakeError::ConnectionClosed)?;

        if let Some(rest) = line.strip_prefix("***") {
            let raw = rest.trim().to_string();
            let scrubbed = super::redaction::redact_freeform(&raw).into_owned();
            return Err(HandshakeError::RemoteError(scrubbed));
        }

        if is_identifier(&line) {
            let codes = parse_sid(&line)?;
            if !codes.contains("B2") {
                return Err(HandshakeError::NoB2Support);
            }
            sid = Some(codes);
        } else if let Some(rest) = line.strip_prefix(";FW:") {
            forwarders = rest
                .split_whitespace()
                .map(|field| field.split('|').next().unwrap_or(field).to_string())
                .collect();
        } else if let Some(rest) = line.strip_prefix(";PQ:") {
            challenge = Some(rest.trim().to_string());
        } else if line.ends_with('>') {
            // The prompt marks the end of the handshake.
            break;
        }
        // Anything else (comments, message-of-the-day, "*** ..." lines) is
        // ignored during the handshake.
    }

    match sid {
        Some(sid) => Ok(RemoteHandshake {
            sid,
            forwarders,
            challenge,
        }),
        None => Err(HandshakeError::NoSid),
    }
}

fn is_identifier(line: &str) -> bool {
    line.starts_with('[') && line.ends_with(']')
}

/// Pull the feature codes out of an identifier line `[NAME-VERSION-CODES]` — the
/// part after the last dash — and upper-case them.
fn parse_sid(line: &str) -> Result<String, HandshakeError> {
    let inner = line
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(HandshakeError::BadSid)?;
    if !inner.contains('-') {
        return Err(HandshakeError::BadSid);
    }
    let codes = inner.rsplit('-').next().ok_or(HandshakeError::BadSid)?;
    Ok(codes.to_uppercase())
}

/// Why the handshake could not be completed.
#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeError {
    /// The server never sent an identifier line.
    NoSid,
    /// The server's identifier line was not in `[NAME-VERSION-CODES]` form.
    BadSid,
    /// The server does not speak the B2 compressed forwarding protocol.
    NoB2Support,
    /// The connection closed before the handshake finished.
    ConnectionClosed,
    /// The CMS sent a `*** ...` error line during the handshake (e.g.,
    /// callsign not authorized, secure login failed before our reply).
    /// Payload is pre-redacted by `redaction::redact_freeform` to avoid
    /// any echoed credential leakage. Takes precedence over NoSid /
    /// ConnectionClosed.
    RemoteError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn builds_the_client_handshake_with_a_secure_response() {
        let bytes = build_handshake("N7CPZ", "SERVICE", "CN87", Some("72768415"));
        let expected = format!(
            ";FW: N7CPZ\r[tuxlink-{}-B2FHM$]\r;PR: 72768415\r; SERVICE DE N7CPZ (CN87)\r",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    #[test]
    fn builds_the_client_handshake_without_a_secure_response() {
        let bytes = build_handshake("N7CPZ", "SERVICE", "CN87", None);
        let expected = format!(
            ";FW: N7CPZ\r[tuxlink-{}-B2FHM$]\r; SERVICE DE N7CPZ (CN87)\r",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    #[test]
    fn reads_a_cms_handshake_with_sid_and_challenge() {
        let data = b"[WL2K-5.0-B2FWIHJM$]\r;PQ: 12345678\rCMS>\r";
        let mut cursor = Cursor::new(&data[..]);
        let hs = read_remote_handshake(&mut cursor).unwrap();
        assert_eq!(hs.sid, "B2FWIHJM$");
        assert_eq!(hs.challenge.as_deref(), Some("12345678"));
    }

    #[test]
    fn reads_a_handshake_with_no_password_challenge() {
        let data = b";FW: RELAY\r[WL2K-5.0-B2FHM$]\rRELAY>\r";
        let mut cursor = Cursor::new(&data[..]);
        let hs = read_remote_handshake(&mut cursor).unwrap();
        assert_eq!(hs.sid, "B2FHM$");
        assert_eq!(hs.challenge, None);
        assert_eq!(hs.forwarders, vec!["RELAY".to_string()]);
    }

    #[test]
    fn rejects_a_server_that_does_not_support_b2() {
        let data = b"[OLDBBS-1.0-FA$]\rBBS>\r";
        let mut cursor = Cursor::new(&data[..]);
        assert_eq!(
            read_remote_handshake(&mut cursor),
            Err(HandshakeError::NoB2Support)
        );
    }

    #[test]
    fn rejects_a_handshake_with_no_identifier_line() {
        let data = b";FW: SOMECALL\rPROMPT>\r";
        let mut cursor = Cursor::new(&data[..]);
        assert_eq!(read_remote_handshake(&mut cursor), Err(HandshakeError::NoSid));
    }

    #[test]
    fn builds_a_master_handshake_ending_in_the_fbb_prompt() {
        let bytes = build_master_handshake("W7AUX", "N7CPZ", "CN87");
        let expected = format!(
            ";FW: W7AUX\r[tuxlink-{}-B2FHM$]\r; N7CPZ DE W7AUX (CN87)>\r",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    #[test]
    fn master_reads_a_slave_handshake_terminated_by_the_message_turn() {
        // A real dialing slave sends NO `>` prompt; its handshake ends and its message
        // turn begins with an `F` line. read_slave_handshake must stop there and leave
        // the `F` line unconsumed for the turn loop (wl2k-go readHandshake semantics).
        let data = b";FW: N7CPZ\r[tuxlink-1.0-B2FHM$]\r; W7AUX DE N7CPZ (CN87)\rFF\r";
        let mut cursor = Cursor::new(&data[..]);
        let hs = read_slave_handshake(&mut cursor).unwrap();
        assert_eq!(hs.sid, "B2FHM$");
        assert_eq!(hs.forwarders, vec!["N7CPZ".to_string()]);
        // The `FF` turn line must remain for the exchange loop.
        assert_eq!(super::wire::read_line(&mut cursor).unwrap(), "FF");
    }

    #[test]
    fn master_still_terminates_on_a_prompt_if_the_peer_sends_one() {
        // A peer that DOES end with a prompt (older/other impls) must still work.
        let data = b";FW: N7CPZ\r[tuxlink-1.0-B2FHM$]\rN7CPZ>\r";
        let mut cursor = Cursor::new(&data[..]);
        let hs = read_slave_handshake(&mut cursor).unwrap();
        assert_eq!(hs.sid, "B2FHM$");
    }

    #[test]
    fn handshake_surfaces_remote_error_taking_precedence_over_no_sid() {
        // R3 #3: today's read_remote_handshake silently drops *** lines.
        // A CMS rejection sent BEFORE the SID line was previously
        // mis-classified as NoSid; the new HandshakeError::RemoteError
        // variant captures it correctly.
        let data = b"*** Callsign not authorized - Disconnecting\r";
        let mut cursor = std::io::Cursor::new(&data[..]);
        let result = read_remote_handshake(&mut cursor);
        match result {
            Err(HandshakeError::RemoteError(payload)) => {
                assert!(payload.contains("Callsign not authorized"), "got: {payload}");
            }
            other => panic!("expected RemoteError, got {other:?}"),
        }
    }

    #[test]
    fn handshake_remote_error_payload_is_redacted() {
        // Defense in depth: if a misbehaving CMS reflects credentials
        // back in an error line, the handshake-error payload must be
        // scrubbed by redaction::redact_freeform before construction.
        let data = b"*** Rejected ;PR: 72768415 (debug echo)\r";
        let mut cursor = std::io::Cursor::new(&data[..]);
        let result = read_remote_handshake(&mut cursor);
        match result {
            Err(HandshakeError::RemoteError(payload)) => {
                assert!(!payload.contains("72768415"), "got: {payload}");
            }
            other => panic!("expected RemoteError, got {other:?}"),
        }
    }
}
