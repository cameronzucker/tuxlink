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
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, OutboundMessage};
use super::wire;

/// How long to wait on a single read or write before giving up.
const TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for the TCP connect (per resolved address) before giving up.
/// Without this, `TcpStream::connect` rides the OS SYN-retry default (~75-130s on
/// Linux), so a filtered/black-holed CMS endpoint reads as a silent stall rather
/// than a fast, legible failure (tuxlink-gqo: cms-z exposes no TLS on 8773, so a
/// CmsSsl connect there hung ~75-130s before ETIMEDOUT).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

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
    progress: &dyn Fn(&str),
    register_socket: &dyn Fn(&TcpStream),
    decide: F,
) -> Result<ExchangeResult, TelnetError>
where
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let shared: Shared = Arc::new(Mutex::new(connect_stream(
        host,
        port,
        transport,
        progress,
        register_socket,
    )?));
    let mut reader = BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared);

    // The CMS telnet "post office" greets with a callsign/password login that
    // precedes the B2F handshake; clear it first.
    telnet_login(&mut reader, &mut writer, &config.mycall)?;
    progress("CMS login complete.");

    progress("Negotiating messages…");
    session::run_exchange(&mut reader, &mut writer, config, outbound, decide)
        .map_err(TelnetError::Exchange)
}

/// Open the TCP connection and, for [`Transport::Tls`], complete the TLS
/// handshake (verifying the server certificate against `host`).
fn connect_stream(
    host: &str,
    port: u16,
    transport: Transport,
    progress: &dyn Fn(&str),
    register_socket: &dyn Fn(&TcpStream),
) -> Result<Box<dyn ReadWrite>, TelnetError> {
    let addrs = (host, port).to_socket_addrs().map_err(TelnetError::Connect)?;
    let tcp = connect_with_timeout(addrs, CONNECT_TIMEOUT).map_err(TelnetError::Connect)?;
    // Hand the freshly-connected socket to the caller BEFORE TLS wrapping moves it,
    // so an abort can `.shutdown()` it to unblock a slow TLS/login/exchange phase
    // (tuxlink-9z2). The initial connect itself is bounded by CONNECT_TIMEOUT above.
    register_socket(&tcp);
    progress("TCP connection established.");
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
            progress("TLS handshake complete.");
            Ok(Box::new(tls))
        }
    }
}

/// Connect to the first reachable address, bounding each attempt by `timeout` so a
/// filtered/black-holed endpoint fails fast instead of riding the OS SYN-retry
/// default (the "silent stall", tuxlink-gqo). Tries each resolved address in turn;
/// returns the first success, or the last error if all fail, or a `NotFound` error
/// if `addrs` is empty (host resolved to nothing).
fn connect_with_timeout(
    addrs: impl Iterator<Item = SocketAddr>,
    timeout: Duration,
) -> std::io::Result<TcpStream> {
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no addresses resolved for host")
    }))
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
            &|_| {},
            &|_| {},
            |_| vec![],
        )
        .unwrap();

        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
    }

    #[test]
    fn connect_with_timeout_errors_when_no_addresses() {
        // Host resolved to nothing → a clean error, never a hang.
        let err = connect_with_timeout(std::iter::empty(), Duration::from_secs(1)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn connect_with_timeout_fails_fast_on_a_refused_port() {
        // Bind to claim a free port, then drop the listener so nothing is
        // listening; connecting is then refused (RST) — fast, deterministic,
        // loopback-only (no external network, per testing-pitfalls).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err =
            connect_with_timeout(std::iter::once(addr), Duration::from_secs(5)).unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "expected refused on a dead port, got {err:?}"
        );
    }

    #[test]
    fn connect_with_timeout_connects_to_a_live_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = connect_with_timeout(std::iter::once(addr), Duration::from_secs(5)).unwrap();
        assert_eq!(stream.peer_addr().unwrap(), addr);
    }

    #[test]
    fn connect_and_exchange_reports_connection_progress() {
        // A local mock that completes the telnet login + an empty exchange while a
        // recording progress sink captures the per-step phase lines. Loopback
        // only — no live network, no RF. The progress callback fires synchronously
        // on this thread, so a RefCell recorder (no Arc/Mutex) is sufficient.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(b"Callsign :\rPassword :\r[WL2K-5.0-B2FHM$]\rCMS>\rFQ\r")
                .unwrap();
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
        let recorded = std::cell::RefCell::new(Vec::<String>::new());
        connect_and_exchange(
            &addr.ip().to_string(),
            addr.port(),
            Transport::Plaintext,
            &config,
            vec![],
            &|msg: &str| recorded.borrow_mut().push(msg.to_string()),
            &|_| {},
            |_| vec![],
        )
        .unwrap();
        server.join().unwrap();

        let lines = recorded.into_inner();
        assert!(
            lines.iter().any(|l| l.contains("TCP")),
            "expected a TCP-connected progress line, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("login")),
            "expected a login-complete progress line, got {lines:?}"
        );
    }
}
