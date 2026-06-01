//! TCP transport for WLE-compat P2P-Telnet sessions.
//!
//! See `docs/design/2026-06-01-tcp-p2p-telnet-design.md` §4.1.
//!
//! This module owns: TCP connect + wire-tap + login wrapper invocation +
//! handoff to `session::run_exchange_with_role(Dial)`. Listener side is in PR 2.

use std::io::{self, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, ExchangeRole, OutboundMessage};
use super::telnet_p2p_login::{self, DialerLoginError, DialerLoginOutcome};

/// How long to wait on a single read or write before giving up.
/// Matches the existing CMS-telnet TIMEOUT for behavioral parity.
const TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait for a single TCP connect before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Why a P2P-Telnet dial attempt failed.
#[derive(Debug)]
pub enum P2pTelnetError {
    /// DNS resolution of the peer address failed.
    Resolve { host: String, port: u16, source: io::Error },
    /// TCP connect to the peer failed.
    Connect { addr: SocketAddr, source: io::Error },
    /// The telnet login wrapper failed.
    Login(DialerLoginError),
    /// The B2F exchange failed once connected and logged in.
    Exchange(ExchangeError),
}

impl std::fmt::Display for P2pTelnetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            P2pTelnetError::Resolve { host, port, source } => {
                write!(f, "DNS resolve failed for {host}:{port}: {source}")
            }
            P2pTelnetError::Connect { addr, source } => {
                write!(f, "TCP connect to {addr} failed: {source}")
            }
            P2pTelnetError::Login(e) => write!(f, "P2P login failed: {e}"),
            P2pTelnetError::Exchange(e) => write!(f, "B2F exchange failed: {e}"),
        }
    }
}

impl std::error::Error for P2pTelnetError {}

impl From<DialerLoginError> for P2pTelnetError {
    fn from(e: DialerLoginError) -> Self {
        P2pTelnetError::Login(e)
    }
}

impl From<ExchangeError> for P2pTelnetError {
    fn from(e: ExchangeError) -> Self {
        P2pTelnetError::Exchange(e)
    }
}

trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}

type Shared = Arc<Mutex<Box<dyn ReadWrite>>>;

struct ReadHalf(Shared);
struct WriteHalf(Shared);

impl Read for ReadHalf {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.lock().expect("p2p connection lock").read(buf)
    }
}
impl Write for WriteHalf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("p2p connection lock").write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().expect("p2p connection lock").flush()
    }
}

/// Reader that prepends a buffer of pushback bytes before yielding from `inner`.
struct PushbackReader<R: Read> {
    pushback: Vec<u8>,
    inner: R,
}
impl<R: Read> Read for PushbackReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.pushback.is_empty() {
            let n = self.pushback.len().min(buf.len());
            buf[..n].copy_from_slice(&self.pushback[..n]);
            self.pushback.drain(..n);
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

fn connect_stream(host: &str, port: u16) -> Result<TcpStream, P2pTelnetError> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|source| P2pTelnetError::Resolve { host: host.to_string(), port, source })?
        .collect();

    let mut last_err: Option<(SocketAddr, io::Error)> = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(TIMEOUT)).ok();
                stream.set_write_timeout(Some(TIMEOUT)).ok();
                return Ok(stream);
            }
            Err(e) => last_err = Some((addr, e)),
        }
    }
    let (addr, source) = last_err.expect("ToSocketAddrs returned non-empty but loop saw no error");
    Err(P2pTelnetError::Connect { addr, source })
}

/// Dial a P2P peer's TCP listener, run the telnet-login wrapper, then a full
/// B2F message exchange in slave role.
#[allow(clippy::too_many_arguments)]
pub fn connect_and_exchange<F>(
    host: &str,
    port: u16,
    peer_callsign: &str,
    peer_password: Option<&str>,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    progress: &dyn Fn(&str),
    wire_log: &dyn Fn(&str),
    decide: F,
) -> Result<ExchangeResult, P2pTelnetError>
where
    F: Fn(&[Proposal]) -> Vec<Answer>,
{
    let _ = peer_callsign; // PR 1 doesn't use this; PR 2 listener-side will.

    progress(&format!("Connecting to {host}:{port} (P2P-Telnet)…"));
    let stream = connect_stream(host, port)?;
    progress("TCP connection open. Running login…");

    let shared: Shared = Arc::new(Mutex::new(Box::new(stream)));
    let read_half = ReadHalf(shared.clone());
    let write_half = WriteHalf(shared);

    let mut reader = BufReader::new(read_half);
    let mut writer = write_half;

    let login_outcome = telnet_p2p_login::dialer_login(
        &mut reader,
        &mut writer,
        &config.mycall,
        peer_password,
    )?;

    progress("Login complete. Negotiating messages…");

    let pushback = match login_outcome {
        DialerLoginOutcome::Done => Vec::new(),
        DialerLoginOutcome::DoneWithPushback { pushback } => pushback,
    };
    let mut pushback_reader = BufReader::new(PushbackReader { pushback, inner: reader });

    session::run_exchange_with_role(
        &mut pushback_reader,
        &mut writer,
        ExchangeRole::Dial,
        config,
        outbound,
        decide,
        Some(wire_log),
    )
    .map_err(P2pTelnetError::Exchange)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Spin up a localhost TCP server that scripts a P2P listener.
    ///
    /// Writes ALL peer-side bytes (prompts + B2F handshake + any extra) to the
    /// socket in one burst before entering the read phase. This avoids the
    /// peek-deadlock in `read_line_with_eol`: when the dialer's BufReader sees
    /// `\r` it tries `fill_buf()` to check for a paired `\n`; if the peer holds
    /// back subsequent bytes until it receives the callsign response, that
    /// `fill_buf()` call blocks on the socket and deadlocks (client waits for
    /// peer; peer waits for client). Writing first puts all peer bytes in the
    /// TCP recv buffer so the peek returns immediately.
    fn scripted_peer(
        prompts: Vec<&'static str>,
        b2f_handshake: &'static str,
        also_send: Option<&'static str>,
    ) -> (u16, thread::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let prompts_owned: Vec<String> = prompts.into_iter().map(|s| s.to_string()).collect();
        let b2f_owned = b2f_handshake.to_string();
        let also_send_owned = also_send.map(|s| s.to_string());
        let handle = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            // Write everything the peer will ever send before blocking on reads.
            // This prevents the fill_buf() peek-deadlock in dialer_login.
            for prompt in &prompts_owned {
                sock.write_all(prompt.as_bytes()).unwrap();
            }
            sock.write_all(b2f_owned.as_bytes()).unwrap();
            if let Some(extra) = &also_send_owned {
                sock.write_all(extra.as_bytes()).unwrap();
            }
            // Now read everything the client sends (until it closes the connection).
            let mut received: Vec<u8> = Vec::new();
            let _ = sock.read_to_end(&mut received);
            received
        });
        (port, handle)
    }

    #[test]
    fn dial_completes_login_then_runs_b2f_exchange() {
        // Peer scripts: send CALLSIGN prompt + full B2F handshake atomically (no
        // wait-for-callsign interlock), then read everything the client sends.
        //
        // Why atomic write: `dialer_login`'s `read_line_with_eol` peeks ahead for
        // `\n` after each `\r` using `fill_buf()`. On a live socket the peek blocks
        // until data is available. If the peer holds back the B2F until after it
        // receives the callsign, the client's peek deadlocks (client waits for peer
        // data; peer waits for client's callsign). Sending all peer-side bytes in
        // one burst puts them in the TCP recv buffer so `fill_buf()` returns
        // immediately, breaking the deadlock without altering the wire protocol.
        //
        // The B2F handshake includes the `>` prompt terminator required by
        // `read_remote_handshake`, followed by `FF\r` (peer has no messages).
        let b2f = ";FW: W7AUX\r[RMS-EXPRESS-1.7.31.0-B2FHM$]\r; N0CALL DE W7AUX (CN87)>\rFF\r";
        let (port, peer_handle) = scripted_peer(vec!["CALLSIGN :\r"], b2f, None);

        let config = ExchangeConfig {
            mycall: "N0CALL".to_string(),
            targetcall: "W7AUX".to_string(),
            locator: "CN87".to_string(),
            password: None,
        };

        let result = connect_and_exchange(
            "127.0.0.1",
            port,
            "W7AUX",
            None,
            &config,
            Vec::new(),
            &|_| {},
            &|_| {},
            |_proposals: &[Proposal]| Vec::new(),
        );

        let _peer_received = peer_handle.join().unwrap();
        let res = result.expect("exchange should succeed");
        assert_eq!(res.sent.len(), 0);
        assert_eq!(res.received.len(), 0);
    }

    #[test]
    fn dial_to_refused_port_returns_connect_error() {
        // 127.0.0.1:1 is privileged + nothing listening → ECONNREFUSED.
        let config = ExchangeConfig {
            mycall: "N0CALL".to_string(),
            targetcall: "W7AUX".to_string(),
            locator: "CN87".to_string(),
            password: None,
        };
        let result = connect_and_exchange(
            "127.0.0.1",
            1,
            "W7AUX",
            None,
            &config,
            Vec::new(),
            &|_| {},
            &|_| {},
            |_proposals: &[Proposal]| Vec::new(),
        );
        assert!(matches!(result, Err(P2pTelnetError::Connect { .. })));
    }
}
