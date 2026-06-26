//! B2F exchange over an ARDOP transport.
//!
//! tuxlink-ytg: bridges [`crate::winlink::modem::ModemTransport`] (already
//! connected via `connect_arq`) into the generic
//! [`crate::winlink::session::run_exchange_with_role`] B2F engine, so a real
//! Winlink mail exchange flows over ardopcf's data socket.
//!
//! # Design
//!
//! `ArdopTransport::data_stream()` returns a single `&mut dyn ReadWrite` — one
//! borrow that has to be both the read half and the write half. The B2F engine
//! takes `R: BufRead` and `W: Write` as separate arguments, so the mutable
//! borrow has to be split somehow. This module uses the same `Arc<Mutex<>>`
//! shared-handle pattern as `winlink/telnet.rs` and the AX.25 packet path in
//! `winlink_backend::native_packet_exchange`. Rationale:
//!
//! - B2F is strictly turn-based — only one side is ever reading or writing at
//!   any instant — so a single mutex around the duplex stream is
//!   contention-free.
//! - The `DataSocket` underneath is a TCP socket whose two halves we *could*
//!   independently `try_clone`, but that would require either (a) reaching
//!   past the trait-object boundary or (b) duplicating the ARQ frame decoder
//!   on both halves. Mutex-sharing avoids both.
//!
//! # EOF coordination
//!
//! The data socket emits `Ok(0)` from `DataSocket::read` on real TCP EOF, which
//! the B2F engine maps to [`crate::winlink::session::ExchangeError::ConnectionClosed`].
//! ardopcf closes the data socket when the ARQ peer DISCs, so this is
//! automatic — no explicit cmd→data EOF wire is added here. (If a future TNC
//! variant keeps the data socket open across DISC, that's a separate
//! coordination bug to file then.)
//!
//! # ARQ-state gate
//!
//! Writes to the data socket before `connect_arq` succeeds would currently go
//! through to a closed/un-attached TCP fd and fail with a plain `BrokenPipe`.
//! The structural protection — `modem_ardop_connect_post_consume_with_factory`
//! only installs the transport into `ModemSession` after `connect_arq` returns
//! `Ok` — keeps this path unreachable from the operator flow today. A logical
//! "ARQ not connected" check inside `DataSocket::write` is a nice-to-have but
//! not load-bearing for v0.2; filed as a follow-up if a tighter gate is wanted.

use std::io::{BufReader, Read, Write};
use std::sync::{Arc, Mutex};

use crate::winlink::modem::{ModemTransport, ReadWrite};
use crate::winlink::proposal::{Answer, PendingMessage, Proposal};
use crate::winlink::session::{
    self, ExchangeConfig, ExchangeError, ExchangeResult, ExchangeRole, OutboundMessage,
};

/// Run one B2F exchange over an already-`connect_arq`'d ARDOP transport.
///
/// The caller has already:
/// - consumed a RADIO-1 consent token (per-invocation Part 97 gate),
/// - spawned ardopcf via [`crate::winlink::modem::ardop::transport::ArdopTransport::with_managed_modem`],
/// - run `init` + `connect_arq` to bring the ARQ link up.
///
/// `role` is [`ExchangeRole::Dial`] for an outbound CMS / peer dial (the
/// default for "send & receive" — the slave/IRS), or [`ExchangeRole::Answer`]
/// for a P2P listen (the master/ISS). Today's flow only uses Dial.
///
/// On return — success or failure — the transport is still owned by the
/// caller, who is responsible for `transport.disconnect(...)` plus drop. This
/// function does NOT swallow the transport, because the caller frequently
/// wants to call `transport.disconnect()` after — see
/// `winlink_backend::native_ardop_connect`.
pub fn run_b2f_exchange<F>(
    transport: &mut dyn ModemTransport,
    role: ExchangeRole,
    config: &ExchangeConfig,
    outbound: Vec<OutboundMessage>,
    decide: F,
) -> Result<ExchangeResult, B2fOverArdopError>
where
    F: Fn(&[Proposal], &[PendingMessage]) -> Result<Vec<Answer>, ExchangeError>,
{
    let data: &mut dyn ReadWrite = transport
        .data_stream()
        .map_err(|e| B2fOverArdopError::DataStream(e.to_string()))?;

    // The shared duplex handle. `&mut dyn ReadWrite: Send` (`ReadWrite` is
    // `Read + Write + Send`), and `Mutex<...>` is Sync, so the same pattern
    // telnet.rs / native_packet_exchange use applies.
    let shared: Arc<Mutex<&mut dyn ReadWrite>> = Arc::new(Mutex::new(data));

    struct ReadHalf<'a>(Arc<Mutex<&'a mut dyn ReadWrite>>);
    struct WriteHalf<'a>(Arc<Mutex<&'a mut dyn ReadWrite>>);

    impl<'a> Read for ReadHalf<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ardop b2f read lock").read(buf)
        }
    }
    impl<'a> Write for WriteHalf<'a> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("ardop b2f write lock").write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().expect("ardop b2f flush lock").flush()
        }
    }

    let mut reader = BufReader::new(ReadHalf(shared.clone()));
    let mut writer = WriteHalf(shared);

    session::run_exchange_with_role(
        &mut reader, &mut writer, role, config, outbound, decide, None,
    )
    .map_err(B2fOverArdopError::Exchange)
}

/// Why a B2F-over-ARDOP exchange failed.
#[derive(Debug)]
pub enum B2fOverArdopError {
    /// `transport.data_stream()` returned an error (init wasn't run, or
    /// connect_arq hasn't succeeded yet).
    DataStream(String),
    /// The B2F protocol exchange itself failed.
    Exchange(ExchangeError),
}

impl std::fmt::Display for B2fOverArdopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            B2fOverArdopError::DataStream(e) => write!(f, "ARDOP data stream not ready: {e}"),
            B2fOverArdopError::Exchange(e) => write!(f, "B2F exchange failed: {e:?}"),
        }
    }
}

impl std::error::Error for B2fOverArdopError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winlink::message::Message;
    use crate::winlink::modem::ardop::session::{ConnectInfo, InitConfig, SessionError};
    use crate::winlink::proposal::{batch_checksum_line, Answer};
    use crate::winlink::session::SessionIntent;
    use crate::winlink::transfer;
    use std::io::Cursor;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    /// A stub `ModemTransport` whose `data_stream()` yields a scripted byte
    /// stream — the bytes the CMS / peer would have sent — and records what
    /// the client writes.
    ///
    /// Bridges the B2F engine to an in-memory duplex so the wiring is
    /// unit-testable end-to-end without TCP, ardopcf, or RF.
    struct ScriptedTransport {
        // The Read+Write surface returned by `data_stream`.
        // The `Cursor` provides the scripted read side; `Vec` captures writes.
        duplex: Duplex,
    }

    struct Duplex {
        reader: Cursor<Vec<u8>>,
        writer: Vec<u8>,
        // Mark-set by `disconnect` so the test can assert the caller cleanly
        // disconnected after the exchange completed.
        disconnected: bool,
    }

    impl Read for Duplex {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reader.read(buf)
        }
    }
    impl Write for Duplex {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writer.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.writer.flush()
        }
    }

    impl ModemTransport for ScriptedTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            _deadline: Option<Duration>,
        ) -> Result<ConnectInfo, SessionError> {
            Ok(ConnectInfo {
                peer_call: "W7RMS-10".into(),
                bandwidth_hz: 500,
            })
        }
        fn disconnect(&mut self, _deadline: Duration) -> Result<(), SessionError> {
            self.duplex.disconnected = true;
            Ok(())
        }
        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            Ok(&mut self.duplex)
        }
    }

    /// A no-data transport whose `data_stream` returns an error — the exact
    /// shape an unconnected ARDOP transport surfaces.
    struct UnreadyTransport;
    impl ModemTransport for UnreadyTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            _deadline: Option<Duration>,
        ) -> Result<ConnectInfo, SessionError> {
            Err(SessionError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stub never connects",
            )))
        }
        fn disconnect(&mut self, _deadline: Duration) -> Result<(), SessionError> {
            Ok(())
        }
        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "init was not run",
            ))
        }
    }

    fn dial_config() -> ExchangeConfig {
        ExchangeConfig {
            mycall: "N7CPZ".into(),
            targetcall: "wl2k".into(),
            locator: "CN87".into(),
            password: None,
            intent: SessionIntent::Cms,
        }
    }

    #[test]
    fn data_stream_error_surfaces_as_data_stream_variant() {
        // tuxlink-ytg: if the transport isn't ready, the B2F driver must surface
        // a clean error rather than panic. The caller (a Tauri command) maps
        // this to a user-visible "ARDOP not connected" message.
        let mut transport = UnreadyTransport;
        let result = run_b2f_exchange(
            &mut transport,
            ExchangeRole::Dial,
            &dial_config(),
            vec![],
            |_, _| Ok(vec![]),
        );
        assert!(
            matches!(result, Err(B2fOverArdopError::DataStream(_))),
            "expected DataStream error, got {result:?}"
        );
    }

    #[test]
    fn empty_session_handshakes_then_quits_via_dial_role() {
        // tuxlink-ytg: the canonical no-traffic exchange — the slave-role dial
        // (operator pressing Send/Receive with an empty outbox + nothing on the
        // CMS) handshakes, signals FF (no more), reads FQ from the remote, and
        // returns a clean empty result.
        //
        // This is structurally identical to
        // `session::tests::a_session_with_no_traffic_handshakes_then_quits`,
        // but routes through `run_b2f_exchange` so the wiring (transport →
        // data_stream → Arc<Mutex> split → run_exchange_with_role) is
        // exercised end-to-end.
        let mut script = Vec::new();
        script.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r"); // no challenge
        script.extend_from_slice(b"FF\r"); // remote: no messages

        let mut transport = ScriptedTransport {
            duplex: Duplex {
                reader: Cursor::new(script),
                writer: Vec::new(),
                disconnected: false,
            },
        };

        let result = run_b2f_exchange(
            &mut transport,
            ExchangeRole::Dial,
            &dial_config(),
            vec![],
            |_, _| Ok(vec![]),
        )
        .expect("empty exchange must succeed");
        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());

        // The exchange wrote our handshake then FF then FQ. We don't compare
        // bytes exactly (handshake content is exercised in session.rs); we
        // assert the tail says "we had nothing to offer (FF), then quit (FQ)".
        let written = &transport.duplex.writer;
        assert!(
            written.ends_with(b"FF\rFQ\r"),
            "expected the exchange to end with FF\\rFQ\\r; got {:?}",
            String::from_utf8_lossy(written)
        );
    }

    #[test]
    fn a_received_message_round_trips_through_the_b2f_driver() {
        // tuxlink-ytg: an offered message that the decide-closure accepts must
        // come back parsed in `result.received`. The wire bytes are produced
        // by `to_proposal` + `frame_block`, exactly as the CMS would send
        // them, so this is the smallest possible reality check that the
        // ARDOP-routed bytes reach the B2F engine intact.
        let mut msg = Message::new();
        msg.set_header("Mid", "ARDOPMSG0001");
        msg.set_header("Subject", "Net check-in");
        msg.set_header("From", "W7RMS-10");
        msg.set_body(b"Net is active.\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();

        let mut script = Vec::new();
        script.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        script.extend_from_slice(proposal.line().as_bytes());
        script.push(b'\r');
        script.extend_from_slice(batch_checksum_line(&[proposal]).as_bytes());
        script.push(b'\r');
        script.extend_from_slice(&transfer::frame_block("Net check-in", 0, &compressed));
        script.extend_from_slice(b"FF\r");

        let mut transport = ScriptedTransport {
            duplex: Duplex {
                reader: Cursor::new(script),
                writer: Vec::new(),
                disconnected: false,
            },
        };

        let decide_calls = StdMutex::new(0usize);
        let result = run_b2f_exchange(
            &mut transport,
            ExchangeRole::Dial,
            &dial_config(),
            vec![],
            |proposals, _manifest| {
                *decide_calls.lock().unwrap() += 1;
                Ok(proposals
                    .iter()
                    .map(|_| Answer::Accept { resume_offset: 0 })
                    .collect())
            },
        )
        .expect("exchange must succeed");

        assert_eq!(result.received.len(), 1);
        assert_eq!(
            result.received[0].header("Mid"),
            Some("ARDOPMSG0001"),
            "the received message's MID must propagate through B2F"
        );
        assert_eq!(result.received[0].body(), b"Net is active.\r\n");
        assert_eq!(
            *decide_calls.lock().unwrap(),
            1,
            "decide closure must fire once for the one offered batch"
        );
    }

    #[test]
    fn an_outbound_message_is_sent_via_the_b2f_driver() {
        // tuxlink-ytg: the symmetric case — operator has a message queued; the
        // CMS accepts it (`FS Y\r`); the framed body must reach the wire.
        let mut msg = Message::new();
        msg.set_header("Mid", "OUTBOUND0001");
        msg.set_header("Subject", "Status report");
        msg.set_body(b"Ops normal.\r\n".to_vec());
        let (proposal, compressed) = msg.to_proposal().unwrap();

        let outbound = vec![OutboundMessage {
            proposal,
            title: "Status report".to_string(),
            compressed,
        }];

        // Server scripts: handshake (no challenge), `FS Y` accepts our one
        // proposal, then `FQ` ends. The CMS would also send a final FF to
        // bookend its own turn, but because we sent the body and we are also
        // out of messages (remaining.clear()), the next turn we go to
        // send-with-nothing and write FF; the server's FQ ends the loop.
        let mut script = Vec::new();
        script.extend_from_slice(b"[WL2K-5.0-B2FHM$]\rCMS>\r");
        script.extend_from_slice(b"FS Y\r"); // accept our proposal
        script.extend_from_slice(b"FQ\r"); // remote quits

        let mut transport = ScriptedTransport {
            duplex: Duplex {
                reader: Cursor::new(script),
                writer: Vec::new(),
                disconnected: false,
            },
        };

        let result = run_b2f_exchange(
            &mut transport,
            ExchangeRole::Dial,
            &dial_config(),
            outbound,
            |_, _| Ok(vec![]),
        )
        .expect("outbound exchange must succeed");

        assert_eq!(result.sent, vec!["OUTBOUND0001".to_string()]);
        assert!(result.received.is_empty());
        assert!(result.rejected.is_empty());
        assert!(result.deferred.is_empty());

        // The wire must contain the proposal line and the framed block. The
        // exact byte-level format is exercised in session.rs tests; here we
        // just confirm something more than the handshake hit the wire.
        let written = &transport.duplex.writer;
        assert!(
            written.len() > 64,
            "expected the wire to carry proposal + body; got {} bytes",
            written.len()
        );
    }

    #[test]
    fn disconnect_can_be_called_after_exchange_returns() {
        // tuxlink-ytg: per the design, `disconnect()` runs AFTER
        // `run_b2f_exchange` returns (success OR failure) and BEFORE
        // `reset_to_stopped`. This test confirms the borrow-split releases the
        // mutable borrow on the transport so the caller can still call
        // `disconnect` afterwards.
        let mut transport = ScriptedTransport {
            duplex: Duplex {
                reader: Cursor::new(b"[WL2K-5.0-B2FHM$]\rCMS>\rFF\r".to_vec()),
                writer: Vec::new(),
                disconnected: false,
            },
        };

        let _ = run_b2f_exchange(
            &mut transport,
            ExchangeRole::Dial,
            &dial_config(),
            vec![],
            |_, _| Ok(vec![]),
        )
        .expect("exchange must succeed");

        // The borrow's lifetime has ended; we can now call disconnect on the
        // transport without a borrow-checker error. (The test's load-bearing
        // assertion is that this compiles + runs.)
        ModemTransport::disconnect(&mut transport, Duration::from_secs(1)).unwrap();
        assert!(
            transport.duplex.disconnected,
            "caller's disconnect must run after exchange"
        );
    }
}
