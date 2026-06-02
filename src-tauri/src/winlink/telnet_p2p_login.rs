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
    #[error("peer closed connection before Callsign prompt")]
    EofBeforeCallsignPrompt,
    #[error("peer sent unexpected line during login: {line:?}")]
    UnexpectedLine { line: String },
}

/// Read one line terminated by `\r` OR `\n`. Returns the bytes including the
/// terminator. Returns `None` on EOF before any byte was read.
///
/// Does NOT peek across `\r\n` pairs — that would require `BufRead::fill_buf`
/// which BLOCKS a TCP socket waiting for the next byte. WLE's wire protocol
/// uses bare `\r`, so paired `\r\n` is only a concern for hypothetical
/// non-WLE peers; the next `read_line_with_eol` call will yield a single-byte
/// `\n` line which the caller treats as an empty line and skips.
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
                if byte[0] == b'\r' || byte[0] == b'\n' {
                    return Ok(Some(buf));
                }
            }
            Err(e) => return Err(e),
        }
    }
}

/// Read a non-empty trimmed line from `reader`. Skips lines whose trimmed
/// content is empty (e.g., a stranded `\n` left over from a `\r\n` previous
/// line, or a peer that sends a blank line as padding).
fn read_non_empty_line<R: BufRead>(reader: &mut R) -> io::Result<Option<(Vec<u8>, String)>> {
    loop {
        let raw = match read_line_with_eol(reader)? {
            Some(l) => l,
            None => return Ok(None),
        };
        let trimmed = trimmed_str(&raw);
        if !trimmed.is_empty() {
            return Ok(Some((raw, trimmed)));
        }
        // Empty trimmed line — likely a stranded \n. Loop and try again.
    }
}

/// Default password to send when a peer prompts but the operator has not
/// configured one. Matches WLE-as-dialer behavior (`TelnetP2PSession.cs:1341`).
/// WLE-as-listener with empty `strStationPassword` accepts any incoming
/// password including this default, so this works against unconfigured peers.
const DEFAULT_PEER_PASSWORD: &str = "CMSTelnet";

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
    // Phase 1: wait for the Callsign: prompt (WLE sends "Callsign :\r"; this is
    // case-insensitive to tolerate variants).
    let (_raw, trimmed) = read_non_empty_line(reader)?
        .ok_or(DialerLoginError::EofBeforeCallsignPrompt)?;
    if !trimmed.eq_ignore_ascii_case("CALLSIGN :") && !trimmed.eq_ignore_ascii_case("CALLSIGN:") {
        return Err(DialerLoginError::UnexpectedLine { line: trimmed });
    }

    // Send our callsign.
    write!(writer, "{}\r", our_callsign)?;
    writer.flush()?;

    // Phase 2: read the next line. Either Password: prompt or B2F handshake.
    let (next, next_trimmed) = match read_non_empty_line(reader)? {
        Some(pair) => pair,
        None => return Ok(DialerLoginOutcome::Done), // Peer closed; no B2F coming.
    };

    if next_trimmed.eq_ignore_ascii_case("PASSWORD :")
        || next_trimmed.eq_ignore_ascii_case("PASSWORD:")
    {
        // Password prompt. Send the configured password, or "CMSTelnet" as
        // the WLE-compat default (WLE-as-listener with empty station password
        // accepts anything; WLE-as-dialer sends "CMSTelnet" when its
        // favorites entry has no remote password — see TelnetP2PSession.cs:1341).
        let pw = password.unwrap_or(DEFAULT_PEER_PASSWORD);
        write!(writer, "{}\r", pw)?;
        writer.flush()?;
        // After the password exchange the peer immediately starts the B2F
        // handshake; read and push back the first line so the session driver
        // sees it in its reader (same pattern as the no-password case).
        return match read_non_empty_line(reader)? {
            Some((b2f_line, _)) => Ok(DialerLoginOutcome::DoneWithPushback { pushback: b2f_line }),
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
    fn sends_cmstelnet_default_password_when_none_configured() {
        // WLE always prompts for password even when its station-password is
        // empty (and accepts anything in that case). WLE-as-dialer sends
        // "CMSTelnet" as its default. Match that for parity + interop.
        let peer = b"CALLSIGN :\rPassword :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\rCMSTelnet\r".to_vec());
        assert!(matches!(
            outcome,
            Ok(DialerLoginOutcome::DoneWithPushback { .. })
        ));
    }

    #[test]
    fn tolerates_lf_alone_terminator_between_prompts() {
        // After the Task 2 CRLF "fix" introduced a fill_buf deadlock on real
        // sockets (the peek blocked forever waiting for bytes the peer would
        // not send until WE sent our callsign), the loop reverted to stopping
        // on either \r or \n. CRLF tolerance is now provided by skipping empty
        // trimmed lines (read_non_empty_line). This test pins that the stranded
        // \n between two \r-terminated lines is correctly skipped.
        let peer = b"CALLSIGN :\r\n[RMS-EXPRESS-1.7.31.0-B2FHM$]\r\n";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                // The trimmed pushback must be the B2F line.
                assert_eq!(trimmed_str(&pushback), "[RMS-EXPRESS-1.7.31.0-B2FHM$]");
            }
            other => panic!("expected DoneWithPushback with B2F line, got {:?}", other),
        }
    }

    #[test]
    fn lowercase_callsign_prompt_from_wle_is_accepted() {
        // WLE actually sends "Callsign :\r" (capital C, rest lowercase) — see
        // TelnetP2PSession.cs:2371. The case-insensitive match handles it.
        let peer = b"Callsign :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, sent) = run(peer, None);
        assert_eq!(sent, b"N0CALL\r".to_vec());
        assert!(matches!(
            outcome,
            Ok(DialerLoginOutcome::DoneWithPushback { .. })
        ));
    }

    #[test]
    fn bare_cr_terminator_does_not_block_peeking_for_lf() {
        // Regression: the prior fill_buf peek for paired \n would block a TCP
        // socket forever when the peer sent bare-\r line endings (which WLE
        // does — every DataToSend call uses "\r"). read_line_with_eol must
        // return immediately on \r without attempting to peek.
        let peer = b"CALLSIGN :\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r";
        let (outcome, _) = run(peer, None);
        match outcome {
            Ok(DialerLoginOutcome::DoneWithPushback { pushback }) => {
                // Pushback must be the B2F line WITHOUT a stranded leading byte
                // from the previous read's terminator.
                assert_eq!(pushback, b"[RMS-EXPRESS-1.7.31.0-B2FHM$]\r".to_vec());
            }
            other => panic!("expected DoneWithPushback, got {:?}", other),
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
