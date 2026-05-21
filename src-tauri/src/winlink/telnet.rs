//! The telnet transport: connect to a Winlink CMS over TCP and run an exchange.
//!
//! This is thin glue. The protocol work lives in [`super::session`]; here we
//! just open the socket, split it into a buffered reader and a writer (a TCP
//! stream can be cloned so both halves share one connection), and hand them to
//! the exchange driver.
//!
//! **Transmission policy.** Calling [`connect_and_exchange`] against the real
//! CMS connects to the live Winlink network under the station's call sign. Per
//! `docs/live-cms-testing-policy.md` and the RADIO-1 pitfall, that is an
//! operator-run, per-run-consented action — automation, tests, and agents must
//! not initiate it. The driver itself is verified with in-memory streams in
//! [`super::session`]; the loopback test below exercises only the socket
//! plumbing against a local mock on `127.0.0.1` (no live network, no RF).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, OutboundMessage};
use super::wire;

/// How long to wait on a single read or write before giving up.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The fixed, public password the CMS telnet "post office" login expects. It is
/// NOT the station's Winlink password — that one answers the B2F secure-login
/// challenge later. (wl2k-go's `telnet.CMSPassword`.)
const CMS_TELNET_PASSWORD: &str = "CMSTelnet";

/// The call sign the CMS identifies as in the B2F handshake target field
/// (wl2k-go's `telnet.CMSTargetCall`).
pub const CMS_TARGET_CALL: &str = "wl2k";

/// Connect to `host:port` and run a full message exchange.
///
/// Operator-run only when `host` is the live CMS — see the module note.
pub fn connect_and_exchange<F>(
    host: &str,
    port: u16,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, TelnetError>
where
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let stream = TcpStream::connect((host, port)).map_err(TelnetError::Connect)?;
    stream.set_read_timeout(Some(TIMEOUT)).ok();
    stream.set_write_timeout(Some(TIMEOUT)).ok();

    // One half reads, the other writes; both refer to the same connection.
    let read_half = stream.try_clone().map_err(TelnetError::Connect)?;
    let mut reader = BufReader::new(read_half);
    let mut writer = stream;

    // The CMS telnet "post office" greets with a callsign/password login that
    // precedes the B2F handshake; clear it first.
    telnet_login(&mut reader, &mut writer, &config.mycall)?;

    session::run_exchange(&mut reader, &mut writer, config, outbound, decide)
        .map_err(TelnetError::Exchange)
}

/// Answer the CMS telnet login: send the call sign at the `Callsign :` prompt
/// and the fixed telnet password at the `Password :` prompt. Returns once the
/// password has been sent (the B2F handshake follows).
pub fn telnet_login<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    mycall: &str,
) -> Result<(), TelnetError> {
    loop {
        let line = wire::read_line(reader).map_err(TelnetError::Connect)?;
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("callsign") {
            writer
                .write_all(format!("{mycall}\r").as_bytes())
                .map_err(TelnetError::Connect)?;
        } else if lower.starts_with("password") {
            writer
                .write_all(format!("{CMS_TELNET_PASSWORD}\r").as_bytes())
                .map_err(TelnetError::Connect)?;
            return Ok(());
        }
    }
}

/// Why a telnet exchange failed.
#[derive(Debug)]
pub enum TelnetError {
    /// The TCP connection could not be opened (or cloned).
    Connect(std::io::Error),
    /// The exchange itself failed once connected.
    Exchange(ExchangeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn telnet_login_answers_the_callsign_and_password_prompts() {
        // The real greeting starts with a stray CR, then the two prompts.
        let mut reader = Cursor::new(b"\rCallsign :\rPassword :\r".to_vec());
        let mut writer = Vec::new();
        telnet_login(&mut reader, &mut writer, "N7CPZ").unwrap();
        assert_eq!(writer, b"N7CPZ\rCMSTelnet\r");
    }

    #[test]
    fn connects_to_a_local_mock_and_runs_an_exchange() {
        // A local fake server on 127.0.0.1 — not the live CMS, not RF. It runs the
        // telnet login, sends a handshake, then immediately quits (FQ), so the
        // client logs in, answers the handshake, signals it has nothing (FF), and
        // reads the quit.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(b"Callsign :\rPassword :\r[WL2K-5.0-B2FHM$]\rCMS>\rFQ\r")
                .unwrap();
            // Drain the client's writes until it closes (EOF), so we never close
            // the socket out from under the client mid-exchange.
            let mut buf = [0u8; 256];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
            }
        });

        let config = ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "SERVICE".into(),
            locator: "CN87".into(),
            password: None,
        };
        let result =
            connect_and_exchange(&addr.ip().to_string(), addr.port(), &config, vec![], |_| vec![])
                .unwrap();

        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
    }
}
