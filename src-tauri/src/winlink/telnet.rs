//! The telnet transport: connect to a Winlink CMS over TCP and run an exchange.
//!
//! This is thin glue. The protocol work lives in [`super::session`]; here we
//! open the connection — plaintext ([`Transport::Plaintext`]) or TLS-wrapped
//! ([`Transport::Tls`], the default, like Winlink Express) — present it to the
//! exchange driver as a read half and a write half, and run the CMS telnet
//! login before the B2F handshake.
//!
//! **Transmission policy.** Calling [`connect_and_exchange`] against the real
//! CMS connects to the live Winlink network under the station's call sign. Per
//! `docs/live-cms-testing-policy.md` and the RADIO-1 pitfall, that is an
//! operator-run, per-run-consented action — automation, tests, and agents must
//! not initiate it. The driver itself is verified with in-memory streams in
//! [`super::session`]; the loopback test below exercises only the socket
//! plumbing against a local mock on `127.0.0.1` (no live network, no RF).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
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

/// How to wrap the CMS connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Plaintext telnet (Winlink CMS port 8772).
    Plaintext,
    /// TLS-wrapped telnet (port 8773) — the default, matching Winlink Express.
    Tls,
}

/// A connection we can read, write, and move across threads.
trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

/// The connection, shared between a read half and a write half. The B2F exchange
/// is strictly turn-based (it never reads and writes at the same instant), so
/// locking per operation is contention-free — and it lets a TLS stream, which
/// cannot be cloned into independent halves the way a `TcpStream` can, back both
/// halves.
type Shared = Arc<Mutex<Box<dyn ReadWrite>>>;

struct ReadHalf(Shared);
struct WriteHalf(Shared);

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.lock().expect("connection lock").read(buf)
    }
}

impl Write for WriteHalf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("connection lock").write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().expect("connection lock").flush()
    }
}

/// Connect to `host:port` over the chosen transport and run a full message
/// exchange.
///
/// Operator-run when `host` is the live CMS — telnet (incl. TLS) to the CMS is
/// authorized dev testing; RADIO-1 covers RF transmission, not this.
pub fn connect_and_exchange<F>(
    host: &str,
    port: u16,
    transport: Transport,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, TelnetError>
where
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let shared: Shared = Arc::new(Mutex::new(connect_stream(host, port, transport)?));
    let mut reader = BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared);

    // The CMS telnet "post office" greets with a callsign/password login that
    // precedes the B2F handshake; clear it first.
    telnet_login(&mut reader, &mut writer, &config.mycall)?;

    session::run_exchange(&mut reader, &mut writer, config, outbound, decide)
        .map_err(TelnetError::Exchange)
}

/// Open the TCP connection and, for [`Transport::Tls`], complete the TLS
/// handshake (verifying the server certificate against `host`).
fn connect_stream(
    host: &str,
    port: u16,
    transport: Transport,
) -> Result<Box<dyn ReadWrite>, TelnetError> {
    let tcp = TcpStream::connect((host, port)).map_err(TelnetError::Connect)?;
    tcp.set_read_timeout(Some(TIMEOUT)).ok();
    tcp.set_write_timeout(Some(TIMEOUT)).ok();
    match transport {
        Transport::Plaintext => Ok(Box::new(tcp)),
        Transport::Tls => {
            let connector =
                native_tls::TlsConnector::new().map_err(|e| TelnetError::Tls(e.to_string()))?;
            let tls = connector
                .connect(host, tcp)
                .map_err(|e| TelnetError::Tls(e.to_string()))?;
            Ok(Box::new(tls))
        }
    }
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
    /// The TCP connection could not be opened.
    Connect(std::io::Error),
    /// The TLS handshake failed.
    Tls(String),
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
        let result = connect_and_exchange(
            &addr.ip().to_string(),
            addr.port(),
            Transport::Plaintext,
            &config,
            vec![],
            |_| vec![],
        )
        .unwrap();

        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
    }
}
