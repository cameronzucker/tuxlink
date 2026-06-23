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
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, OutboundMessage};
use super::wire;

/// How long to wait on a single read or write before giving up.
const TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for the TCP connect to a *single* resolved address before
/// giving up. Without this, `TcpStream::connect` rides the OS SYN-retry default
/// (~75-130s on Linux), so a filtered/black-holed CMS endpoint reads as a silent
/// stall rather than a fast, legible failure (tuxlink-gqo: cms-z exposes no TLS on
/// 8773, so a CmsSsl connect there hung ~75-130s before ETIMEDOUT).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Total wall-clock budget for the whole connect sweep across *all* resolved
/// addresses (tuxlink-lbg #4). Each address still gets up to [`CONNECT_TIMEOUT`],
/// but the sum is bounded here so a host that resolves to many black-holed A/AAAA
/// records can't stack to N×[`CONNECT_TIMEOUT`]. Larger than one [`CONNECT_TIMEOUT`]
/// so a dual-stack host whose first family is dead can still fail over to a working
/// second address.
const CONNECT_TOTAL_DEADLINE: Duration = Duration::from_secs(30);

/// How long to wait for DNS resolution before giving up (tuxlink-lbg #3).
/// `ToSocketAddrs` is synchronous and otherwise unbounded; a hung/black-holed
/// resolver would reintroduce the silent stall gqo fixed for connect — and worse,
/// no socket exists yet, so an operator Abort can't unblock it. Resolution runs on
/// a worker thread bounded by this timeout.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

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

/// A read/write tap that mirrors each `\r`-terminated protocol line to a sink
/// (tuxlink-nki), so the session log can surface the raw B2F wire dialogue —
/// `[WL2K-...]`, `;PQ`, `;FW`, the client SID, `FF`/`FQ`, etc. — under the
/// "Raw output" view, instead of only the human progress summary. The CMS speaks
/// `\r`-terminated lines (see [`wire::read_line`]); we frame on `\r` to match, and
/// reuse [`wire::clean_line`] so logged lines match what the parser sees.
struct WireTap<'a, T> {
    inner: T,
    sink: &'a dyn Fn(&str),
    /// Direction marker prefixed to each logged line: `'<'` received, `'>'` sent.
    dir: char,
    line: Vec<u8>,
}

impl<'a, T> WireTap<'a, T> {
    fn new(inner: T, sink: &'a dyn Fn(&str), dir: char) -> Self {
        Self { inner, sink, dir, line: Vec::new() }
    }

    /// Accumulate observed bytes; emit each completed (`\r`-terminated) line.
    fn observe(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\r' {
                self.flush();
            } else {
                self.line.push(b);
            }
        }
    }

    /// Emit the buffered line. B2F protocol lines are ASCII and logged verbatim;
    /// a chunk containing non-ASCII bytes is a binary payload (e.g. an
    /// LZHUF-compressed message body) and is summarized as a byte count — never
    /// dumped as mojibake (tuxlink-nki re-smoke finding).
    ///
    /// Lines are forwarded to the sink RAW, including the secure-login
    /// `;PQ:`/`;PR:` exchange. The sink is the operator's in-memory session-log
    /// ring (`LogSource::Wire`), which the operator reads live to diagnose
    /// connection problems (tuxlink-6726, operator decision 2026-06-22). Wire
    /// bytes are never handed to a tracing macro, so they never reach the
    /// `.jsonl` disk sink; the issue-report upload re-redacts the ring at its own
    /// boundary (`logging::export::clean_operator_session_message_inner`). Redacting
    /// here instead blanked the operator's own diagnostic window — the regression
    /// this method's prior `sanitize_wire_line` call introduced.
    fn flush(&mut self) {
        let raw = std::mem::take(&mut self.line);
        if raw.is_empty() {
            return;
        }
        let is_ascii_text = raw
            .iter()
            .all(|&b| b == b'\t' || b == b'\n' || (0x20..=0x7e).contains(&b));
        if is_ascii_text {
            let text = wire::clean_line(&String::from_utf8_lossy(&raw)).to_string();
            if !text.is_empty() {
                (self.sink)(&format!("{} {}", self.dir, text));
            }
        } else {
            (self.sink)(&format!("{} <{} bytes binary>", self.dir, raw.len()));
        }
    }
}

impl<'a, T: Read> Read for WireTap<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.observe(&buf[..n]);
        Ok(n)
    }
}

impl<'a, T: Write> Write for WireTap<'a, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.observe(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Connect to `host:port` over the chosen transport and run a full message
/// exchange.
///
/// Operator-run when `host` is the live CMS — telnet (incl. TLS) to the CMS is
/// authorized dev testing; RADIO-1 covers RF transmission, not this.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_exchange<F>(
    host: &str,
    port: u16,
    transport: Transport,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    register_socket: &dyn Fn(&TcpStream),
    decide: F,
) -> Result<ExchangeResult, TelnetError>
where
    F: Fn(&[Proposal]) -> Result<Vec<Answer>, ExchangeError>,
{
    let shared: Shared = Arc::new(Mutex::new(connect_stream(
        host,
        port,
        transport,
        progress,
        register_socket,
    )?));
    // Tee both directions of the wire to `wire_log` so the raw B2F dialogue
    // (`[WL2K-...]`, `;PQ`, `;FW`, the client SID, `FF`/`FQ`) is visible in the
    // session log's Raw output, not just the human progress summary (tuxlink-nki).
    //
    // Tee both directions RAW into `wire_log` so the operator's scrolling log
    // shows the full B2F dialogue, including the secure-login `;PQ`/`;PR`
    // exchange (tuxlink-6726). Safe: wire bytes feed only the in-memory
    // session-log ring, never the `.jsonl` tracing sink; the issue-report upload
    // re-redacts the ring at its own boundary.
    let mut reader = BufReader::new(WireTap::new(ReadHalf(shared.clone()), wire_log, '<'));
    let mut writer = WireTap::new(WriteHalf(shared), wire_log, '>');

    // The CMS telnet "post office" greets with a callsign/password login that
    // precedes the B2F handshake; clear it first.
    telnet_login(&mut reader, &mut writer, &config.mycall)?;
    progress("CMS login complete.");

    progress("Negotiating messages…");
    session::run_exchange(&mut reader, &mut writer, config, outbound, decide, None)
        .map_err(TelnetError::Exchange)
}

/// Auth-only connection: connect + telnet login + B2F handshake + quit. Does NOT
/// run any inbound proposal reading or outbound message sending. Sends `FF` + `FQ`
/// on successful auth to signal "nothing to exchange" and quit cleanly.
///
/// Emits the full [`super::b2f_events::B2fEvent`] stream via `events` when
/// `Some`: `RemoteSidReceived`, `SecureChallengeReceived`, `SecureResponseSent`,
/// `PostAuthExchangeStarted` (Mode 5 discriminator), `RemoteErrorReceived`, and
/// `ConnectionClosed`. The caller emits `AuthClassified` after this returns.
///
/// Used by `ui_commands::cms_connect_test` per spec §4.3 (iii).
///
/// RADIO-1 GUARDRAIL: CMS-TELNET ONLY. Any RF-transport extension requires
/// fresh RADIO-1 review + separate command name per spec §2 out-of-scope + §4.3 (iii).
#[allow(clippy::too_many_arguments)]
pub(crate) fn connect_and_auth_test(
    host: &str,
    port: u16,
    transport: Transport,
    config: &ExchangeConfig,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    register_socket: &dyn Fn(&TcpStream),
    events: Option<&dyn super::b2f_events::B2fEventSink>,
    attempt_id: super::b2f_events::AttemptId,
) -> Result<ExchangeResult, TelnetError> {
    let shared: Shared = Arc::new(Mutex::new(connect_stream(
        host,
        port,
        transport,
        progress,
        register_socket,
    )?));
    // Tee both directions RAW into `wire_log` (tuxlink-6726) — see the sibling
    // `connect_and_exchange` above for why the operator window gets the
    // unredacted B2F dialogue while the `.jsonl` + upload sinks stay redacted.
    let mut reader = BufReader::new(WireTap::new(ReadHalf(shared.clone()), wire_log, '<'));
    let mut writer = WireTap::new(WriteHalf(shared), wire_log, '>');

    telnet_login(&mut reader, &mut writer, &config.mycall)?;
    progress("CMS login complete.");

    progress("Checking credentials…");
    session::run_exchange_with_events(
        &mut reader,
        &mut writer,
        config,
        vec![],
        |_| Ok(vec![]),
        None,
        events,
        attempt_id,
    )
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
    // Resolve under a bounded timeout (tuxlink-lbg #3): a hung resolver must fail
    // fast, not silently stall before any socket exists to abort.
    let addrs = resolve_with_timeout(host, port, RESOLVE_TIMEOUT).map_err(TelnetError::Connect)?;
    // Bound each attempt by CONNECT_TIMEOUT AND the whole sweep by a total deadline
    // (tuxlink-lbg #4) so a many-address host can't stack to N×CONNECT_TIMEOUT.
    let deadline = Instant::now() + CONNECT_TOTAL_DEADLINE;
    let tcp = connect_with_deadline(addrs.into_iter(), CONNECT_TIMEOUT, deadline)
        .map_err(TelnetError::Connect)?;
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

/// Resolve `(host, port)` to socket addresses under a bounded `timeout`
/// (tuxlink-lbg #3). `ToSocketAddrs` is synchronous and unbounded; a black-holed
/// resolver would otherwise reintroduce the silent stall — and unlike a TCP
/// connect, no socket exists yet for an operator Abort to shut down. Resolution
/// runs on a worker thread; if it outruns `timeout` we return `TimedOut` and let
/// the now-orphaned worker finish (or die with the process) on its own.
fn resolve_with_timeout(
    host: &str,
    port: u16,
    timeout: Duration,
) -> std::io::Result<Vec<SocketAddr>> {
    let host = host.to_string();
    resolve_with_deadline_inner(
        move || (host.as_str(), port).to_socket_addrs().map(|it| it.collect()),
        timeout,
    )
}

/// The timeout core of [`resolve_with_timeout`], generic over the resolve closure
/// so the timeout path is unit-testable without a real hung resolver.
fn resolve_with_deadline_inner<F>(resolve: F, timeout: Duration) -> std::io::Result<Vec<SocketAddr>>
where
    F: FnOnce() -> std::io::Result<Vec<SocketAddr>> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        // The receiver may already be gone (we timed out); ignore the send error.
        let _ = tx.send(resolve());
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("DNS resolution timed out after {timeout:?}"),
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(std::io::Error::other(
            "DNS resolver thread terminated without a result",
        )),
    }
}

/// Connect to the first reachable address, bounding each attempt by `per_attempt`
/// so a filtered/black-holed endpoint fails fast instead of riding the OS SYN-retry
/// default (the "silent stall", tuxlink-gqo), and bounding the whole sweep by
/// `deadline` so a many-address host can't stack to N×`per_attempt` (tuxlink-lbg
/// #4). Tries each resolved address in turn; returns the first success, a `NotFound`
/// error if `addrs` is empty (host resolved to nothing), or — when every address
/// fails — an error naming EVERY address tried and why (tuxlink-lbg #6), carrying
/// the first failure's `ErrorKind` (a `ConnectionRefused` is more actionable than a
/// later `TimedOut`).
fn connect_with_deadline(
    addrs: impl Iterator<Item = SocketAddr>,
    per_attempt: Duration,
    deadline: Instant,
) -> std::io::Result<TcpStream> {
    let mut errors: Vec<String> = Vec::new();
    let mut first_kind: Option<std::io::ErrorKind> = None;
    let mut tried_any = false;
    for addr in addrs {
        tried_any = true;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            first_kind.get_or_insert(std::io::ErrorKind::TimedOut);
            errors.push(format!("{addr}: total connect deadline exceeded before attempt"));
            break;
        }
        // The last address gets whatever budget is left, never more than per_attempt.
        let timeout = remaining.min(per_attempt);
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                first_kind.get_or_insert(e.kind());
                errors.push(format!("{addr}: {e}"));
            }
        }
    }
    if !tried_any {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no addresses resolved for host",
        ));
    }
    Err(std::io::Error::new(
        first_kind.unwrap_or(std::io::ErrorKind::TimedOut),
        errors.join("; "),
    ))
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

/// Login callsign for an RMS Relay "post office" telnet session, mirroring WLE
/// `GetBaseCallsign` (`Globals.cs:3136-3154`): uppercase, drop any `.`-qualifier
/// then SSID, then (for the local `L` pool) append `-L` — the `-L` suffix is the
/// entire local-vs-global routing discriminator (`TelnetSession.cs:2011-2013`).
/// Network PO passes `local = false` for the full base callsign, no `-L`.
///
/// No >6-char rejection: that check is Pactor-TNC-only (`PactorWL2KSession.cs:2259`);
/// importing it here would be a tuxlink-added safeguard (see memory
/// `feedback_no_tuxlink_added_safeguards`).
pub fn base_callsign_for_post_office(raw: &str, local: bool) -> String {
    let base = raw
        .trim()
        .to_uppercase()
        .split('.')
        .next()
        .unwrap_or("")
        .split('-')
        .next()
        .unwrap_or("")
        .to_string();
    if local { format!("{base}-L") } else { base }
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
    use super::session::SessionIntent;
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
            intent: SessionIntent::Cms,
        };
        let result = connect_and_exchange(
            &addr.ip().to_string(),
            addr.port(),
            Transport::Plaintext,
            &config,
            vec![],
            &|_| {},
            &|_| {},
            &|_| {},
            |_| Ok(vec![]),
        )
        .unwrap();

        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
        server.join().unwrap();
    }

    /// A deadline far enough out that the per-attempt timeout, not the total budget,
    /// governs these single-address tests.
    fn far_deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    #[test]
    fn connect_with_deadline_errors_when_no_addresses() {
        // Host resolved to nothing → a clean error, never a hang.
        let err = connect_with_deadline(std::iter::empty(), Duration::from_secs(1), far_deadline())
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn connect_with_deadline_fails_fast_on_a_refused_port() {
        // Bind to claim a free port, then drop the listener so nothing is
        // listening; connecting is then refused (RST) — fast, deterministic,
        // loopback-only (no external network, per testing-pitfalls).
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = connect_with_deadline(std::iter::once(addr), Duration::from_secs(5), far_deadline())
            .unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "expected refused on a dead port, got {err:?}"
        );
    }

    #[test]
    fn connect_with_deadline_connects_to_a_live_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let stream =
            connect_with_deadline(std::iter::once(addr), Duration::from_secs(5), far_deadline())
                .unwrap();
        assert_eq!(stream.peer_addr().unwrap(), addr);
    }

    #[test]
    fn connect_with_deadline_names_every_failed_address() {
        // tuxlink-lbg #6: a multi-address total failure must surface EVERY address
        // tried, not just the last — and keep the first (most actionable) ErrorKind.
        let l1 = TcpListener::bind("127.0.0.1:0").unwrap();
        let a1 = l1.local_addr().unwrap();
        drop(l1);
        let l2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let a2 = l2.local_addr().unwrap();
        drop(l2);
        let err = connect_with_deadline([a1, a2].into_iter(), Duration::from_secs(5), far_deadline())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&a1.to_string()), "missing first addr in {msg:?}");
        assert!(msg.contains(&a2.to_string()), "missing second addr in {msg:?}");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
    }

    #[test]
    fn connect_with_deadline_stops_when_total_budget_is_spent() {
        // tuxlink-lbg #4: once the total deadline has passed, remaining addresses are
        // skipped rather than each adding another per-attempt timeout. A past deadline
        // exercises this deterministically — no real network wait.
        let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let past = Instant::now() - Duration::from_secs(1);
        let start = Instant::now();
        let err =
            connect_with_deadline(std::iter::once(addr), Duration::from_secs(15), past).unwrap_err();
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "a passed deadline must return promptly, took {:?}",
            start.elapsed()
        );
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(err.to_string().contains("deadline"), "got {err}");
    }

    #[test]
    fn resolve_with_deadline_inner_returns_addresses() {
        // tuxlink-lbg #3: the happy path passes the resolver's result straight through.
        let addr: SocketAddr = "127.0.0.1:8772".parse().unwrap();
        let got = resolve_with_deadline_inner(move || Ok(vec![addr]), Duration::from_secs(5))
            .unwrap();
        assert_eq!(got, vec![addr]);
    }

    #[test]
    fn resolve_with_deadline_inner_times_out_on_a_slow_resolver() {
        // tuxlink-lbg #3: a resolver that outruns the budget yields TimedOut promptly,
        // not a hang — the bug a black-holed DNS server would otherwise cause.
        let start = Instant::now();
        let err = resolve_with_deadline_inner(
            || {
                thread::sleep(Duration::from_secs(3));
                Ok(vec![])
            },
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "timeout must fire well before the slow resolver finishes, took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn resolve_with_timeout_resolves_localhost() {
        // Real resolution of a name guaranteed present locally (no external DNS).
        let addrs = resolve_with_timeout("localhost", 8772, Duration::from_secs(5)).unwrap();
        assert!(!addrs.is_empty(), "localhost should resolve to at least one address");
        assert!(addrs.iter().all(|a| a.port() == 8772));
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
            intent: SessionIntent::Cms,
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
            &|_| {},
            |_| Ok(vec![]),
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

    #[test]
    fn connect_and_exchange_tees_raw_wire_lines() {
        // tuxlink-nki: the wire tap must surface the raw B2F dialogue in BOTH
        // directions so "Raw output" can show the real protocol exchange. Loopback
        // mock, no live network: the CMS sends a banner + CMS> + FQ; the client
        // sends its login + handshake (;FW:). The wire sink fires synchronously on
        // this thread, so a RefCell recorder suffices.
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
            intent: SessionIntent::Cms,
        };
        let wire = std::cell::RefCell::new(Vec::<String>::new());
        connect_and_exchange(
            &addr.ip().to_string(),
            addr.port(),
            Transport::Plaintext,
            &config,
            vec![],
            &|_| {},
            &|line: &str| wire.borrow_mut().push(line.to_string()),
            &|_| {},
            |_| Ok(vec![]),
        )
        .unwrap();
        server.join().unwrap();

        let lines = wire.into_inner();
        // Received lines are prefixed '<': the CMS banner must be captured.
        assert!(
            lines.iter().any(|l| l.starts_with('<') && l.contains("[WL2K-5.0-B2FHM$]")),
            "expected the received CMS banner in the wire log, got {lines:?}"
        );
        // Sent lines are prefixed '>': the client's B2F forward line must be captured.
        assert!(
            lines.iter().any(|l| l.starts_with('>') && l.contains(";FW: N7CPZ")),
            "expected the sent ;FW handshake line in the wire log, got {lines:?}"
        );
    }

    #[test]
    fn wire_tap_summarizes_binary_payloads_instead_of_mojibake() {
        // tuxlink-nki re-smoke: a `\r`-framed chunk with non-ASCII bytes (an
        // LZHUF-compressed message body) must log as a byte-count summary, not
        // garbage — while ASCII protocol lines stay readable.
        let recorded = std::cell::RefCell::new(Vec::<String>::new());
        let sink = |l: &str| recorded.borrow_mut().push(l.to_string());
        let mut tap = WireTap::new(std::io::sink(), &sink, '<');
        tap.observe(b";FW: N7CPZ\r"); // ASCII protocol line
        tap.observe(&[0x00, 0xff, 0x9a, 0x1b, 0xc3, b'\r']); // 5-byte binary payload
        let lines = recorded.into_inner();
        assert!(
            lines.iter().any(|l| l == "< ;FW: N7CPZ"),
            "ASCII protocol line should stay readable, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "< <5 bytes binary>"),
            "binary payload should be summarized, not dumped, got {lines:?}"
        );
    }

    #[test]
    fn wire_tap_emits_secure_login_tokens_raw_to_the_operator_window() {
        // tuxlink-6726: the operator's live scrolling log MUST show the raw B2F
        // dialogue, including the secure-login `;PQ:`/`;PR:` exchange, so alpha
        // testers can diagnose connection problems. (Operator decision, 2026-06-22:
        // "We WANT passwords in the scrolling logs and do NOT want them in the
        // .jsonl disk logs.")
        //
        // This is safe because the WireTap feeds ONLY the in-memory session-log
        // ring (LogSource::Wire); wire bytes are never handed to a tracing macro,
        // so they never reach the `.jsonl` disk sink. The issue-report upload
        // re-redacts the ring at its own boundary
        // (`logging::export::clean_operator_session_message_inner` → `redact_freeform`,
        // covered by `logging::export` tests), so a raw window does not leak
        // credential-equivalent material to disk or to an uploaded report.
        //
        // Replaces the prior `wire_log_redacts_pr_token_per_blocker_fix` test,
        // whose source-level scrubbing blanked the operator's own diagnostic
        // window — the regression this change fixes.
        let recorded = std::cell::RefCell::new(Vec::<String>::new());
        let sink = |l: &str| recorded.borrow_mut().push(l.to_string());
        let mut tap = WireTap::new(std::io::sink(), &sink, '>');
        // wl2k-go vector: challenge "23753528", password "FOOBAR" → response "72768415".
        tap.observe(b";PR: 72768415\r");
        tap.observe(b";PQ: 23753528\r");
        let lines = recorded.into_inner();
        assert!(
            lines.iter().any(|l| l == "> ;PR: 72768415"),
            "operator window must show the raw ;PR response token, got {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "> ;PQ: 23753528"),
            "operator window must show the raw ;PQ challenge token, got {lines:?}"
        );
    }

    #[test]
    fn base_callsign_for_post_office_local_appends_dash_l_after_stripping() {
        // Vector table pins the WLE GetBaseCallsign algorithm: uppercase, split '.'
        // first, then '-', take the first token; append -L for local. NO >6 rejection.
        assert_eq!(base_callsign_for_post_office("n7cpz-10", true), "N7CPZ-L");
        assert_eq!(base_callsign_for_post_office("N7CPZ.P", true), "N7CPZ-L");
        assert_eq!(base_callsign_for_post_office("W7XYZ-10", true), "W7XYZ-L");
        assert_eq!(base_callsign_for_post_office("N7CPZ", true), "N7CPZ-L");
        assert_eq!(base_callsign_for_post_office("RELAY1", true), "RELAY1-L"); // tactical passthrough
        // '.' splits BEFORE '-' (load-bearing order): "w7xyz-5.bbs" -> ".".0="w7xyz-5" -> "-".0="w7xyz"
        assert_eq!(base_callsign_for_post_office("w7xyz-5.bbs", true), "W7XYZ-L");
    }

    #[test]
    fn base_callsign_for_post_office_network_keeps_full_base_no_dash_l() {
        assert_eq!(base_callsign_for_post_office("n7cpz-10", false), "N7CPZ");
        assert_eq!(base_callsign_for_post_office("N7CPZ.P", false), "N7CPZ");
    }
}
