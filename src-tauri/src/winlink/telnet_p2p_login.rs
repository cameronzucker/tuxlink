//! Dialer-side telnet-login wrapper for WLE-compat P2P sessions.
//!
//! Runs BEFORE the B2F handshake. The peer (listener) emits a `CALLSIGN :`
//! prompt; we answer with our callsign. The peer then either emits a
//! `Password :` prompt (if it has a station password configured) OR begins
//! emitting the B2F handshake `[NAME-VERSION-CODES]`. We handle both cases
//! without losing bytes.
//!
//! Wire reference: `dev/scratch/winlink-re/findings/p2p-telnet.md`
//! (WLE decompile `TelnetP2PSession.cs:1252-1340`).

use std::io::{self, BufRead, Write};

/// Outcome of the dialer-side login.
#[derive(Debug, PartialEq, Eq)]
pub enum DialerLoginOutcome {
    /// Login completed; the next line on the wire is the B2F handshake start.
    Done,
    /// Login completed AND we already consumed (but did not forward) the first
    /// line of the B2F handshake. The caller MUST prepend `pushback` to its
    /// reader before invoking `run_exchange`. Carries the raw line including
    /// the trailing newline byte that triggered our look-ahead.
    DoneWithPushback { pushback: Vec<u8> },
}

#[derive(Debug, thiserror::Error)]
pub enum DialerLoginError {
    #[error("io error during login: {0}")]
    Io(#[from] io::Error),
    #[error("peer closed connection before CALLSIGN prompt")]
    EofBeforeCallsignPrompt,
    #[error("peer asked for password but none was configured for this peer")]
    PasswordPromptedButNotConfigured,
    #[error("peer sent unexpected line during login: {line:?}")]
    UnexpectedLine { line: String },
}

/// Read one line terminated by `\r`, `\n`, or `\r\n`. Returns the bytes
/// INCLUDING the terminator(s) — for `\r\n`, both are consumed and included.
/// Returns `None` on EOF before any byte was read.
fn read_line_with_eol<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match reader.read(&mut byte) {
            Ok(0) => {
                return Ok(if buf.is_empty() { None } else { Some(buf) });
            }
            Ok(_) => {
                buf.push(byte[0]);
                if byte[0] == b'\n' {
                    return Ok(Some(buf));
                }
                if byte[0] == b'\r' {
                    // Peek for a paired \n — consume if present, leave alone if not.
                    // BufRead::fill_buf is the non-consuming peek primitive.
                    let peek = reader.fill_buf()?;
                    if peek.first() == Some(&b'\n') {
                        buf.push(b'\n');
                        reader.consume(1);
                    }
                    return Ok(Some(buf));
                }
            }
            Err(e) => return Err(e),
        }
    }
}

fn trimmed_str(line: &[u8]) -> String {
    String::from_utf8_lossy(line).trim().to_string()
}

/// Run the WLE-compat telnet-login wrapper as the dialer.
///
/// Sequence:
///   peer  → us:   "CALLSIGN :\r"
///   us    → peer: "<our_callsign>\r"
///   peer  → us:   EITHER "Password :\r" (then we send password) OR the first
///                 line of the B2F handshake (which we hand back via pushback)
pub fn dialer_login<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    our_callsign: &str,
    password: Option<&str>,
) -> Result<DialerLoginOutcome, DialerLoginError> {
    // Phase 1: wait for the CALLSIGN: prompt.
    let line = read_line_with_eol(reader)?.ok_or(DialerLoginError::EofBeforeCallsignPrompt)?;
    let trimmed = trimmed_str(&line);
    if !trimmed.eq_ignore_ascii_case("CALLSIGN :") && !trimmed.eq_ignore_ascii_case("CALLSIGN:") {
        return Err(DialerLoginError::UnexpectedLine { line: trimmed });
    }

    // Send our callsign.
    write!(writer, "{}\r", our_callsign)?;
    writer.flush()?;

    // Phase 2: read the next line. Either Password: prompt or B2F handshake.
    let next = match read_line_with_eol(reader)? {
        Some(l) => l,
        None => return Ok(DialerLoginOutcome::Done), // Peer closed; no B2F coming.
    };
    let next_trimmed = trimmed_str(&next);

    if next_trimmed.eq_ignore_ascii_case("PASSWORD :")
        || next_trimmed.eq_ignore_ascii_case("PASSWORD:")
    {
        // Password prompt. We need a configured password.
        let pw = password.ok_or(DialerLoginError::PasswordPromptedButNotConfigured)?;
        write!(writer, "{}\r", pw)?;
        writer.flush()?;
        // After the password exchange the peer immediately starts the B2F
        // handshake; read and push back the first line so the session driver
        // sees it in its reader (same pattern as the no-password case).
        return match read_line_with_eol(reader)? {
            Some(b2f_line) => Ok(DialerLoginOutcome::DoneWithPushback { pushback: b2f_line }),
            None => Ok(DialerLoginOutcome::Done),
        };
    }

    // Not a password prompt — this is the B2F handshake. Push it back.
    Ok(DialerLoginOutcome::DoneWithPushback { pushback: next })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn run(
        peer_script: &[u8],
        password: Option<&str>,
    ) -> (Result<DialerLoginOutcome, DialerLoginError>, Vec<u8>) {
        let mut reader = std::io::BufReader::new(Cursor::new(peer_script.to_vec()));
        let mut writer: Vec<u8> = Vec::new();
        let outcome = dialer_login(&mut reader, &mut writer, "N0CALL", password);
        (outcome, writer)
    }

    #[test]
    fn answers_callsign_prompt_then_sees_b2f_handshake() {
        let peer = b"CALLSIGN :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r".to_vec());
            }
            other => panic!("expected DoneWithPushback, got {:?}", other),
        }
    }

    #[test]
    fn answers_password_prompt_when_present_and_password_provided() {
        let peer = b"CALLSIGN :\rPassword :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, Some("hunter2"));
        assert_eq!(sent, b"N0CALL\rhunter2\r".to_vec());
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r".to_vec());
            }
            other => panic!("expected DoneWithPushback, got {:?}", other),
        }
    }

    #[test]
    fn errors_if_password_prompted_but_none_provided() {
        let peer = b"CALLSIGN :\rPassword :\r";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        assert!(matches!(
            outcome,
            Err(DialerLoginError::PasswordPromptedButNotConfigured)
        ));
    }

    #[test]
    fn tolerates_crlf_line_endings_in_both_callsign_prompt_and_b2f_opener() {
        // WLE on Windows sends \r\n. Both terminators must be consumed as one
        // unit so the next read sees the B2F handshake start, not a stranded \n.
        let peer = b"CALLSIGN :\r\n[RMS-EXPRESS-1.7.31.0-B2FHM$]\r\n";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                // Pushback must be the B2F line including its \r\n, NOT a stranded \n.
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r\n".to_vec());
            }
            other => panic!("expected DoneWithPushback with B2F line, got {:?}", other),
        }
    }

    #[test]
    fn errors_on_eof_before_callsign_prompt() {
        let peer = b"";
        let (outcome, _) = run(peer, None);
        assert!(matches!(
            outcome,
            Err(DialerLoginError::EofBeforeCallsignPrompt)
        ));
    }

    #[test]
    fn unexpected_first_line_yields_error_not_silent_pass() {
        let peer = b"WELCOME TO SOMETHING ELSE\r";
        let (outcome, _) = run(peer, None);
        assert!(matches!(
            outcome,
            Err(DialerLoginError::UnexpectedLine { .. })
        ));
    }
}
