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

use std::io::BufReader;
use std::net::TcpStream;
use std::time::Duration;

use super::proposal::{Answer, Proposal};
use super::session::{self, ExchangeConfig, ExchangeError, ExchangeResult, OutboundMessage};

/// How long to wait on a single read or write before giving up.
const TIMEOUT: Duration = Duration::from_secs(60);

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

    session::run_exchange(&mut reader, &mut writer, config, outbound, decide)
        .map_err(TelnetError::Exchange)
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn connects_to_a_local_mock_and_runs_an_exchange() {
        // A local fake server on 127.0.0.1 — not the live CMS, not RF. It sends a
        // handshake then immediately quits (FQ), so the client connects, answers
        // the handshake, signals it has nothing (FF), and reads the quit.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            sock.write_all(b"[WL2K-5.0-B2FHM$]\rCMS>\rFQ\r").unwrap();
            // Drain the client's writes so it never blocks, then let the socket
            // close as the thread ends.
            let mut buf = [0u8; 256];
            let _ = sock.read(&mut buf);
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
