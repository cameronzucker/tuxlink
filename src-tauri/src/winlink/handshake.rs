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

/// Read the server's handshake lines until its prompt, gathering the identifier,
/// any forwarders, and any password challenge.
pub fn read_remote_handshake<R: BufRead>(
    reader: &mut R,
) -> Result<RemoteHandshake, HandshakeError> {
    let mut sid: Option<String> = None;
    let mut forwarders = Vec::new();
    let mut challenge = None;

    loop {
        let line = wire::read_line(reader).map_err(|_| HandshakeError::ConnectionClosed)?;

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
}
