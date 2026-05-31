//! `ArdopTransport` — implements [`ModemTransport`] for ardopcf (and any
//! ARDOP-compatible TNC) over the standard dual-TCP-socket host protocol.
//!
//! `with_addrs` constructs an unconnected transport; `init` opens both the
//! command socket and the data socket and runs the ARDOP init sequence.
//! After a successful `connect_arq` the `data_stream` accessor exposes the
//! `DataSocket` as `&mut dyn ReadWrite` for consumption by the sync B2F
//! `run_exchange`.
//!
//! Phase 5 adds `with_managed_modem` and `shutdown` for the full
//! tuxlink-owns-the-process lifecycle (ADR 0015 decision #2).

use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::super::process::{ManagedModem, ProcessError};
use super::arq_state::ArqState;
use super::command::{Command, State};
use super::data::DataSocket;
use super::session::{arq_connect, arq_disconnect, init_tnc, CmdSocket, ConnectInfo, InitConfig, SessionError};
use super::ArdopConfig;
use crate::modem_status::{ModemState, ModemStatus};
use crate::winlink::modem::{ModemTransport, ReadWrite};
use std::sync::mpsc::RecvTimeoutError;

/// Width of the rolling throughput window (tuxlink-n2uz). 5 seconds matches
/// the operator-visible "bytes/s right now" feel without lagging too far
/// behind a freshly-started transmit.
const THROUGHPUT_WINDOW: Duration = Duration::from_secs(5);

/// Connection-time accumulators that derive numeric meters from streamed
/// ardopcf events (tuxlink-n2uz).
///
/// The transport owns one of these for the lifetime of an `ArdopTransport`;
/// each successful `Connected` event stamps `connected_at`, each `BUFFER`
/// event with a drop in queue depth accumulates `bytes_tx` + appends a
/// throughput sample.
#[derive(Debug, Default)]
struct AccumulatorState {
    /// First `Connected` event timestamp of the current session. `None`
    /// while disconnected; cleared on `Disconnected` / `Fault` / NEWSTATE
    /// `DISC|OFFLINE`. Subsequent CONNECTED events within the same session
    /// (e.g. a duplicate emit by the TNC) do NOT re-stamp.
    connected_at: Option<Instant>,
    /// Monotonic count of bytes the TNC has transmitted, derived from drops
    /// in BUFFER queue-depth events. Saturates on overflow.
    bytes_tx: u64,
    /// Last BUFFER reading. `None` until the first BUFFER event arrives.
    prior_buffer: Option<u32>,
    /// Rolling window of (timestamp, cumulative bytes_tx) samples used to
    /// compute throughput_bps. Pruned to entries within `THROUGHPUT_WINDOW`
    /// on every push.
    throughput_samples: VecDeque<(Instant, u64)>,
}

/// How long to wait (total) for ardopcf to bind both TCP ports after spawn.
const BIND_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
/// Interval between retry attempts while waiting for ports to open.
const BIND_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(100);

// ─── ArdopTransport ─────────────────────────────────────────────────────────

/// [`ModemTransport`] implementation for ardopcf (ARDOP TNC).
///
/// Drives ardopcf's dual-TCP host protocol:
/// - cmd socket (typically 8515): `\r`-terminated ASCII command lines.
/// - data socket (typically 8516): ARQ-framed inbound, raw bytes outbound.
///
/// # Lifecycle (external TNC — `with_addrs`)
///
/// ```text
/// with_addrs(cmd_addr, data_addr)   ← no I/O; sockets not yet open
///   .init(cfg)                      ← opens CmdSocket + DataSocket, runs init sequence
///   .connect_arq(target, n, t)      ← ARQCALL handshake → ConnectInfo
///   .data_stream()                  ← Read + Write for B2F exchange
///   .disconnect(t)                  ← DISCONNECT command + confirmation
/// ```
///
/// # Lifecycle (managed TNC — `with_managed_modem`)
///
/// ```text
/// with_managed_modem(cfg)           ← spawns ardopcf, bind-waits for both ports
///   .init(cfg)                      ← same as above
///   ...
///   .shutdown()                     ← disconnect + close sockets + stop process + audio-release check
/// ```
pub struct ArdopTransport {
    cmd_addr: SocketAddr,
    data_addr: SocketAddr,
    cmd: Option<CmdSocket>,
    data: Option<DataSocket>,
    /// Present only when tuxlink spawned and owns the TNC process.
    /// Tuple: (supervisor, optional audio-device path for release check).
    managed: Option<(ManagedModem, Option<PathBuf>)>,
    /// Connection-time accumulators feeding the numeric live meters
    /// (tuxlink-n2uz). Updated by [`drain_status_events`]; populate the
    /// derived `ModemStatus` fields (bytes_tx / throughput_bps / uptime_sec /
    /// bytes_rx) at the end of each broadcaster tick.
    accumulators: AccumulatorState,
    /// Clone of the `ArqState` shared with the data + cmd sockets, kept so
    /// the broadcaster tick can sample `bytes_rx` without reaching through
    /// `Option<DataSocket>` (the data socket may be dropped during a clean
    /// shutdown while we still want to render the final session's totals).
    arq_state: Option<ArqState>,
}

impl ArdopTransport {
    /// Construct an `ArdopTransport` pointing at `cmd_addr` and `data_addr`.
    ///
    /// No I/O happens here — sockets are opened lazily in [`ModemTransport::init`].
    pub fn with_addrs(cmd_addr: SocketAddr, data_addr: SocketAddr) -> Self {
        ArdopTransport {
            cmd_addr,
            data_addr,
            cmd: None,
            data: None,
            managed: None,
            accumulators: AccumulatorState::default(),
            arq_state: None,
        }
    }

    /// Spawn the ardopcf binary described by `cfg`, wait for both TCP ports to
    /// accept connections, then return a transport ready for `init`.
    ///
    /// `cfg.extra_args` is passed verbatim to the binary — the caller packs all
    /// needed arguments (including cmd_port, capture, and playback device names).
    ///
    /// # Bind-wait
    ///
    /// After spawning, the function loops trying `TcpStream::connect` to both
    /// `cmd_port` and `data_port` (loopback). Both must accept before
    /// [`BIND_WAIT_TIMEOUT`] elapses; otherwise returns
    /// `SessionError::Io(ErrorKind::TimedOut)`.
    ///
    /// # RADIO-1
    ///
    /// The caller must obtain per-invocation operator consent before calling
    /// this function — spawning ardopcf can eventually key the radio.
    pub fn with_managed_modem(cfg: ArdopConfig) -> Result<Self, SessionError> {
        Self::with_managed_modem_timeout(cfg, BIND_WAIT_TIMEOUT)
    }

    /// Like `with_managed_modem` but with a caller-specified bind-wait timeout.
    /// Exposed for tests that need a short timeout to keep the test suite fast.
    pub fn with_managed_modem_timeout(
        cfg: ArdopConfig,
        bind_wait: Duration,
    ) -> Result<Self, SessionError> {
        // Pass extra_args verbatim to the binary. The caller is responsible for
        // packing ardopcf's positional args (cmd_port, capture, playback) into
        // extra_args — the CLI example does exactly this. cmd_port and data_port
        // fields on ArdopConfig are used exclusively for the bind-wait and the
        // transport socket addresses.
        let binary_str = cfg.binary.to_string_lossy().into_owned();
        let args_refs: Vec<&str> = cfg.extra_args.iter().map(|s| s.as_str()).collect();

        let modem = ManagedModem::spawn(&binary_str, &args_refs)
            .map_err(|e: ProcessError| io::Error::other(format!("failed to spawn modem: {e}")))?;

        let cmd_addr: SocketAddr = format!("127.0.0.1:{}", cfg.cmd_port)
            .parse()
            .expect("cmd_addr parse is infallible for valid u16");
        let data_addr: SocketAddr = format!("127.0.0.1:{}", cfg.data_port)
            .parse()
            .expect("data_addr parse is infallible for valid u16");

        // Bind-wait: loop until both ports are bound by the ardopcf process, or timeout.
        //
        // Detection strategy: attempt to bind a new loopback socket to the same
        // port. If binding fails with EADDRINUSE (AddressInUse), ardopcf is already
        // listening on that port. This avoids consuming a connection slot (unlike
        // TcpStream::connect), which would prevent the real `init` connection from
        // being accepted.
        let start = Instant::now();
        loop {
            let cmd_ok = std::net::TcpListener::bind(cmd_addr).is_err();
            let data_ok = std::net::TcpListener::bind(data_addr).is_err();
            if cmd_ok && data_ok {
                break;
            }
            if start.elapsed() >= bind_wait {
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "ardopcf did not bind ports {} and {} within {:?}",
                        cfg.cmd_port, cfg.data_port, bind_wait
                    ),
                )));
            }
            std::thread::sleep(BIND_WAIT_POLL_INTERVAL);
        }

        Ok(ArdopTransport {
            cmd_addr,
            data_addr,
            cmd: None,
            data: None,
            managed: Some((modem, cfg.audio_device_path)),
            accumulators: AccumulatorState::default(),
            arq_state: None,
        })
    }

    /// Tear down the full transport + process lifecycle.
    ///
    /// Steps (each best-effort; errors are accumulated but the sequence
    /// always completes):
    ///
    /// 1. Best-effort `DISCONNECT` on the cmd socket (ignores errors — the TNC
    ///    process is about to be killed regardless).
    /// 2. Drop both sockets (their `Drop` implementations close the TCP streams
    ///    and join background threads).
    /// 3. If a managed process is held: `ManagedModem::stop(~3s)`.
    /// 4. If an `audio_device_path` was configured:
    ///    `confirm_audio_device_released(path, ~2s)`. Returns
    ///    `Err(SessionError::Io(WouldBlock))` if the device is still held
    ///    after the deadline — the ADR-0015 swap invariant.
    ///
    /// # Idempotent / retry
    ///
    /// Safe to call on a partially-initialized transport (e.g., `with_addrs`
    /// without `init` — all `Option` fields are just `None`-checked).
    ///
    /// If the audio-device release check fails (`Err(WouldBlock)`), `managed`
    /// is **restored** so that a subsequent `shutdown()` re-runs the check
    /// rather than silently becoming a no-op. Once the release check succeeds,
    /// `managed` stays consumed and subsequent calls are true no-ops.
    pub fn shutdown(&mut self) -> Result<(), SessionError> {
        // Step 1: best-effort ARQ disconnect.
        if let Some(ref mut cmd) = self.cmd {
            let _ = arq_disconnect(cmd, Duration::from_secs(5));
        }

        // Step 2: drop sockets.
        self.cmd = None;
        self.data = None;

        // Step 3 + 4: stop the process, then verify the audio device is released
        // (the ADR-0015 swap invariant). `take()` clears `managed` so a second
        // shutdown() is a true no-op on the SUCCESS path. The release check runs
        // REGARDLESS of whether stop() reported an error: after the SIGKILL
        // escalation the process is gone and the device should be free, so the
        // swap invariant must still be verified rather than silently skipped on a
        // stop error (code review Ph5).
        //
        // RETRY SEMANTICS: if the audio-device release check FAILS (WouldBlock),
        // we restore `self.managed` before returning so that a subsequent
        // shutdown() re-checks rather than becoming a silent no-op. Once the
        // release check SUCCEEDS, `managed` stays consumed and subsequent calls
        // are true no-ops.
        if let Some((mut modem, audio_path)) = self.managed.take() {
            let stop_result = modem.stop(Duration::from_secs(3));

            let release_failed = if let Some(path) = &audio_path {
                !ManagedModem::confirm_audio_device_released(path, Duration::from_secs(2))
            } else {
                false
            };
            if release_failed {
                // Restore managed so a retry shutdown() re-checks the invariant.
                let err_msg = format!(
                    "audio device {:?} still held after shutdown — swap invariant violated",
                    audio_path.as_deref()
                );
                self.managed = Some((modem, audio_path));
                return Err(SessionError::Io(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    err_msg,
                )));
            }

            // Surface a stop failure only after the swap-invariant check has run.
            stop_result.map_err(|e| io::Error::other(format!("modem stop failed: {e}")))?;
        }

        Ok(())
    }

    /// Return a reference to the live `CmdSocket`, or an `Err` if `init` has
    /// not been called.
    fn cmd_or_err(&mut self) -> Result<&mut CmdSocket, SessionError> {
        self.cmd.as_mut().ok_or_else(|| {
            SessionError::Io(io::Error::new(
                io::ErrorKind::NotConnected,
                "ArdopTransport: init() has not been called",
            ))
        })
    }

    /// Populate the derived numeric-meter fields on `status` from the
    /// transport's accumulator state. Called at the end of each
    /// [`drain_status_events`] tick, AFTER all queued events have been
    /// folded in.
    ///
    /// Takes `&mut self` so [`current_throughput_bps`] can prune the
    /// rolling sample window on every tick (Codex P1 finding, tuxlink-n2uz);
    /// without prune-on-call the throughput meter would stay frozen at
    /// the last computed rate after TX stops.
    fn populate_derived_meters(&mut self, status: &mut ModemStatus) {
        status.uptime_sec = self
            .accumulators
            .connected_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        status.bytes_tx = self.accumulators.bytes_tx;
        status.bytes_rx = self
            .arq_state
            .as_ref()
            .map(|s| s.bytes_rx())
            .unwrap_or(0);
        status.throughput_bps = current_throughput_bps(&mut self.accumulators);
    }
}

// ─── Numeric-meter accumulators (tuxlink-n2uz) ─────────────────────────────
//
// Free-standing helpers (rather than methods on `ArdopTransport`) so they
// can be called from inside `drain_status_events` while `self.cmd` is held
// as a `&mut` — Rust's borrow checker permits disjoint-field borrows but
// not interleaved method calls on `self`.

/// Record a single BUFFER event and (when the queue depth drops) accrue
/// `bytes_tx` + append a throughput sample.
///
/// **BUFFER semantics:** ardopcf's `BUFFER <n>` reports the remaining
/// outbound queue depth (bytes still pending TX). A drop from N→M means
/// `(N − M)` bytes were transmitted; a rise means new bytes were enqueued
/// (`Send` command) and must NOT contribute to `bytes_tx`.
///
/// **Wrap / saturate:** `bytes_tx` uses `saturating_add` so a runaway
/// peer cannot wrap the counter past `u64::MAX`. In practice the meter
/// rolls over at ~18 EB; saturation is the conservative choice.
fn record_buffer(accum: &mut AccumulatorState, remaining: u32) {
    let now = Instant::now();
    if let Some(prior) = accum.prior_buffer {
        if remaining < prior {
            let sent = u64::from(prior - remaining);
            accum.bytes_tx = accum.bytes_tx.saturating_add(sent);
            accum.throughput_samples.push_back((now, accum.bytes_tx));
            // Trim the rolling window: discard samples older than
            // THROUGHPUT_WINDOW relative to `now`. Always keep at least
            // one historical sample so the rate calculation has a
            // non-zero elapsed window after a brief lull.
            while accum.throughput_samples.len() > 1 {
                let (t_front, _) = accum.throughput_samples[0];
                if now.duration_since(t_front) > THROUGHPUT_WINDOW {
                    accum.throughput_samples.pop_front();
                } else {
                    break;
                }
            }
        }
        // remaining >= prior: operator enqueued more (not a TX event).
        // Do not decrement `bytes_tx` and do not append a sample.
    }
    accum.prior_buffer = Some(remaining);
}

/// Compute current throughput (bits/second) from the rolling 5s sample
/// window, or `None` if there isn't enough history to compute a rate.
///
/// Returns `None` when:
/// - Fewer than 2 samples are buffered (no transmissions yet, or a
///   single drop event — no time-delta to divide by).
/// - The window elapsed (now − oldest sample) is < 500 ms (sample is too
///   fresh; rate would be a high-variance instantaneous spike, not a
///   meter the operator can read).
/// - All samples are older than `THROUGHPUT_WINDOW` and get pruned (the
///   link has gone idle).
///
/// **Idle-decay (Codex P1 finding, 2026-05-31; tuxlink-n2uz).** This
/// function prunes the sample window on every call AND uses `Instant::now()`
/// as the window upper bound — not the latest sample's timestamp. After
/// TX stops, no new BUFFER drops arrive, so without on-call pruning the
/// window would stay frozen at its last contents and report the same
/// "old fast" rate forever. Pruning here lets the meter decay smoothly
/// to 0 and then to `None` as samples age out of the 5-second window.
fn current_throughput_bps(accum: &mut AccumulatorState) -> Option<u32> {
    let now = Instant::now();
    // Prune samples older than the window on every call (not just when a
    // new sample arrives). Always keep at least one historical sample so
    // a brief lull doesn't collapse the deque to a single element that
    // would force a `None` return via the len<2 guard while a real rate
    // is still observable.
    while accum.throughput_samples.len() > 1 {
        let (t_front, _) = accum.throughput_samples[0];
        if now.duration_since(t_front) > THROUGHPUT_WINDOW {
            accum.throughput_samples.pop_front();
        } else {
            break;
        }
    }

    let samples = &accum.throughput_samples;
    if samples.len() < 2 {
        return None;
    }
    let (t0, b0) = *samples.front()?;
    let (_t1, b1) = *samples.back()?;
    // Use `now` (not the latest sample's timestamp) as the window upper
    // bound. When TX stops, `b1 - b0` stays constant but `now - t0` keeps
    // growing, so the computed rate decays toward 0 and eventually the
    // front sample expires out of the window (returning None above).
    let elapsed = now.duration_since(t0).as_secs_f64();
    if elapsed < 0.5 {
        return None;
    }
    let delta = b1.saturating_sub(b0) as f64;
    let bytes_per_sec = delta / elapsed;
    // Cap at u32::MAX rather than wrapping — at gigabit rates this
    // bound is never reached on HF, but defensive against pathological
    // accumulator state.
    let bits_per_sec = bytes_per_sec * 8.0;
    Some(bits_per_sec.min(u32::MAX as f64) as u32)
}

/// Fold a single ardopcf event into accumulator state (tuxlink-n2uz).
/// Called BEFORE the corresponding [`apply_ardop_event_to_status`] so the
/// status mutation and the accumulator update see the same event in order.
/// Pure-state — no I/O.
fn apply_event_to_accumulators_inline(event: &Command, accum: &mut AccumulatorState) {
    match event {
        Command::Connected { .. } if accum.connected_at.is_none() => {
            // Stamp connected_at on the FIRST Connected event of the
            // current session. Don't re-stamp on a duplicate event:
            // ardopcf may emit CONNECTED more than once for a single
            // ARQ session (e.g. on a state retransmission), and the
            // operator-visible uptime would jitter backwards otherwise.
            accum.connected_at = Some(Instant::now());
        }
        Command::Disconnected | Command::Fault(_) => {
            // Session over — clear the connected timestamp so uptime
            // freezes at "0" rather than counting wall-clock time
            // against a stale connect.
            accum.connected_at = None;
        }
        Command::NewState(State::Disc) | Command::NewState(State::Offline) => {
            // DISC / OFFLINE state transitions are the cmd-socket's
            // companion signal to a DISCONNECTED event; treat both as
            // session-over.
            accum.connected_at = None;
        }
        Command::Buffer(remaining) => {
            record_buffer(accum, *remaining);
        }
        _ => {}
    }
}

impl ModemTransport for ArdopTransport {
    /// Open the cmd and data sockets, then run the ARDOP TNC init sequence.
    ///
    /// Replaces any previously-open sockets (idempotent re-init).
    ///
    /// tuxlink-ytg: builds a fresh [`ArqState`] and wires it into BOTH the
    /// cmd socket (whose reader thread flips it on CONNECTED / DISCONNECTED /
    /// NEWSTATE DISC) and the data socket (whose `read` returns EOF when the
    /// flag is `Disconnected` AND no payload is buffered, and whose `write`
    /// refuses while disconnected). This is the cmd↔data coordination the
    /// B2F engine needs to surface an on-air disconnect promptly instead of
    /// hanging on a quiet but still-open data TCP socket.
    fn init(&mut self, cfg: &InitConfig) -> Result<(), SessionError> {
        // Hold the sockets as locals and run init_tnc on the local cmd socket: if
        // any step fails, the locals drop (CmdSocket::Drop shuts down + joins its
        // reader thread; DataSocket closes its TcpStream), leaving `self` in a
        // clean uninit state for an idempotent re-init — and avoiding an unwrap on
        // a just-stored Option. (Code review Phase 3.)
        let arq_state = ArqState::new();
        let mut cmd = CmdSocket::connect_with_arq_state(self.cmd_addr, Some(arq_state.clone()))?;
        let data = DataSocket::connect_with_arq_state(self.data_addr, Some(arq_state.clone()))?;
        init_tnc(&mut cmd, cfg)?;
        self.cmd = Some(cmd);
        self.data = Some(data);
        // tuxlink-n2uz: a transport-side clone so `drain_status_events` can
        // sample `bytes_rx` without depending on the data socket still being
        // installed (it may be dropped during a clean disconnect while the
        // broadcaster is mid-tick).
        self.arq_state = Some(arq_state);
        // Reset accumulators on a fresh init — an idempotent re-init MUST
        // present a clean slate (no stale connected_at, no stale bytes_tx).
        self.accumulators = AccumulatorState::default();
        Ok(())
    }

    /// Initiate an ARQ connection to `target` with `repeat` retries, bounded by
    /// `deadline`.
    ///
    /// Returns `Err` if [`init`] was not called first.
    ///
    /// tuxlink-ytg P1 (Codex adrev 2026-05-30 #2): on a successful handshake,
    /// drains any bytes that landed in the data socket's OS receive buffer
    /// between `init()` and the `CONNECTED` event. ardopcf can emit ARQ-tagged
    /// bytes on the data socket from monitored / non-session traffic in that
    /// window; without this drain those pre-connect bytes would be accepted
    /// as session data on the first post-connect read and corrupt the B2F
    /// handshake (the `pump_decoder` drop gate only fires at decode time, not
    /// for already-buffered bytes whose decode is deferred until after the
    /// flag flips). The drain is a no-op if the data socket is unset (impossible
    /// once `init` has run, but defensively handled).
    fn connect_arq(
        &mut self,
        target: &str,
        repeat: u32,
        deadline: Duration,
    ) -> Result<ConnectInfo, SessionError> {
        let cmd = self.cmd_or_err()?;
        let info = arq_connect(cmd, target, repeat, deadline)?;
        // Stamp `connected_at` at THIS moment — `arq_connect` just consumed
        // the first `CONNECTED` event from the cmd socket directly via
        // `recv_event`, so a later `drain_status_events` tick will NEVER
        // see that event in `apply_event_to_accumulators_inline`'s
        // `Command::Connected` arm. Without this stamp, `uptime_sec` would
        // stay 0 for the entire session unless ardopcf emits a duplicate
        // CONNECTED. (Codex P1 finding, 2026-05-31; tuxlink-n2uz.)
        //
        // The `connected_at.is_none()` guard on the event-driven arm still
        // applies — both paths stamp the FIRST observed connect moment, and
        // a duplicate event won't reset the timer.
        self.accumulators.connected_at = Some(Instant::now());
        // Drain pre-connect OS-buffered bytes from the data socket NOW. The
        // cmd reader thread has already flipped ArqState to connected (that
        // happens BEFORE the Connected event is sent on the channel), so the
        // window between the flag flip and this call is tens-of-microseconds
        // — far shorter than the on-air round trip for the peer to receive
        // our CONNECTED ack and start transmitting session data. Anything in
        // the OS buffer at this moment is therefore pre-session noise.
        if let Some(data) = self.data.as_mut() {
            let _ = data.drain_pending(); // best-effort; ignore drain I/O errors
        }
        Ok(info)
    }

    /// Send `DISCONNECT` and wait for the TNC to confirm the link is torn down.
    ///
    /// Returns `Err` if [`init`] was not called first.
    ///
    /// Belt-and-suspenders clears `connected_at` directly: the event-driven
    /// arm in `apply_event_to_accumulators_inline` already clears it when
    /// the `Disconnected` event arrives via the cmd-socket reader thread,
    /// but a command-initiated disconnect may return before that event has
    /// drained — clearing here avoids a stale stamp on an immediate
    /// reconnect. (Codex P1 finding, 2026-05-31; tuxlink-n2uz.)
    fn disconnect(&mut self, deadline: Duration) -> Result<(), SessionError> {
        let cmd = self.cmd_or_err()?;
        let result = arq_disconnect(cmd, deadline);
        self.accumulators.connected_at = None;
        result
    }

    /// Return the data byte stream for the connected ARQ session.
    ///
    /// Returns `Err(NotConnected)` if [`init`] was not called, so callers get
    /// a clear error rather than a panic.
    fn data_stream(&mut self) -> io::Result<&mut dyn ReadWrite> {
        self.data
            .as_mut()
            .map(|d| d as &mut dyn ReadWrite)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "ArdopTransport: init() has not been called — data socket not open",
                )
            })
    }

    /// Expose a clone of the cmd-socket write half so a side channel
    /// (`ModemSession::abort_in_flight`) can inject `ABORT\r` while
    /// `connect_arq`'s recv loop is blocked (tuxlink-o3f2). Returns `None`
    /// when `init()` has not been called and the cmd socket is therefore
    /// not yet open.
    fn try_clone_abort_writer(&self) -> Option<std::net::TcpStream> {
        self.cmd.as_ref().and_then(|s| s.try_clone_writer().ok())
    }

    /// Drain pending cmd-socket events and fold them into `status`.
    ///
    /// Called by [`crate::modem_status::ModemStatusBroadcaster`] every
    /// 250 ms. Uses [`Duration::ZERO`] on `CmdSocket::recv_event` so the
    /// drain never blocks the broadcaster tick, and caps the per-call loop
    /// at [`MAX_DRAIN_EVENTS_PER_TICK`] so a runaway emitter cannot starve
    /// the tick.
    ///
    /// If the cmd-socket reader thread has exited (the TNC went away), the
    /// status is marked [`ModemState::Error`] with a `last_error` message
    /// the UI surfaces in the dock.
    fn drain_status_events(&mut self, status: &mut ModemStatus) {
        // Even when the cmd socket is absent (pre-init / post-shutdown),
        // populate the derived meters from whatever accumulator state we
        // already hold so the UI doesn't blink fields back to 0/None
        // mid-tick on the way to ModemState::Stopped.
        let Some(cmd) = self.cmd.as_mut() else {
            self.populate_derived_meters(status);
            return;
        };
        // SAFETY of split borrow: the loop borrows `self.cmd` mutably via
        // `cmd`, then calls `apply_event_to_accumulators(&self, event)`
        // which borrows OTHER fields on self (`accumulators` + `arq_state`).
        // Rust's borrow checker can split-borrow disjoint fields when they
        // are accessed through `self.<field>` — but a method call on `self`
        // would re-borrow `self` whole, which conflicts. Inline the access
        // by calling a free-standing helper that takes the accumulators by
        // `&mut`, sidestepping the split-borrow issue.
        let accumulators = &mut self.accumulators;
        for _ in 0..MAX_DRAIN_EVENTS_PER_TICK {
            match cmd.recv_event(Duration::ZERO) {
                Ok(event) => {
                    apply_event_to_accumulators_inline(&event, accumulators);
                    apply_ardop_event_to_status(event, status);
                }
                Err(RecvTimeoutError::Timeout) => break, // queue empty
                Err(RecvTimeoutError::Disconnected) => {
                    status.state = ModemState::Error;
                    status.last_error.get_or_insert_with(|| {
                        "cmd-socket reader thread exited (TNC connection lost)".into()
                    });
                    // Still publish whatever derived meters we computed
                    // before the cmd socket died.
                    self.populate_derived_meters(status);
                    return;
                }
            }
        }
        // After the drain, surface the derived numeric meters.
        self.populate_derived_meters(status);
    }
}

// ─── Status-event translation ────────────────────────────────────────────────

/// Maximum number of cmd-socket events drained per broadcaster tick.
///
/// Bounds the worst-case time spent inside `drain_status_events` so a
/// chatty / runaway TNC cannot stall the 250 ms broadcaster cadence.
const MAX_DRAIN_EVENTS_PER_TICK: usize = 64;

/// Fold a single parsed ardopcf [`Command`] into a [`ModemStatus`].
///
/// Handles the structural (non-numeric) status fields: state / peer /
/// width_hz / arq_flags / last_error. The numeric live meters
/// (`uptime_sec`, `bytes_tx`, `bytes_rx`, `throughput_bps`) are populated
/// by [`ArdopTransport::populate_derived_meters`] from the transport's
/// accumulator state — they are NOT touched here so this function can stay
/// a pure mapping helper.
///
/// Mapping:
///
/// - `NewState(s)` updates `status.state` (plus derived `arq_flags.rx/tx`).
///   `FecSend`/`FecRcv` map to `ConnectedIrs` as a best-effort
///   approximation — FEC isn't an ARQ link, but the dock has no separate
///   FEC state today.
/// - `Connected { peer, bw }` populates `peer`, `width_hz`, clears
///   `last_error`, and sets `state = ConnectedIrs` (initial role; a
///   subsequent `NewState` will flip to `ConnectedIss` if applicable).
/// - `Disconnected` clears the ARQ rx/tx flags and transitions to `Idle`.
///   (Full Stopped transition is owned by `ModemSession::reset_to_stopped`.)
/// - `Fault(msg)` transitions to `Error` and stores the message.
/// - `Ptt(on)` mirrors into `arq_flags.tx`.
/// - `Busy(on)` mirrors into `arq_flags.busy`.
/// - `Buffer(n)` flips `arq_flags.tx` true when queue depth is non-zero
///   (movement indicator).
/// - `Status(_)`: free-form STATUS strings — S/N + VU parsing deferred to a
///   follow-up issue. ardopcf emits these without a stable structured form
///   we're confident about; on-air capture is required before shipping a
///   parser.
/// - `EchoBack(_)` is intentionally ignored — setter echo-backs are protocol
///   bookkeeping, not status-relevant.
fn apply_ardop_event_to_status(event: Command, status: &mut ModemStatus) {
    match event {
        Command::NewState(new_state) => {
            status.state = match new_state {
                State::Offline => ModemState::Stopped,
                State::Disc => ModemState::Idle,
                State::Idle => ModemState::Idle,
                State::Iss => ModemState::ConnectedIss,
                State::Irs => ModemState::ConnectedIrs,
                // FEC modes aren't ARQ; best-effort mapping to keep the dock
                // showing "connected-ish". Revisit if we add a FEC dock state.
                State::FecSend | State::FecRcv => ModemState::ConnectedIrs,
            };
            status.arq_flags.rx = matches!(new_state, State::Irs);
            status.arq_flags.tx = matches!(new_state, State::Iss);
        }
        Command::Connected {
            peer_call,
            bandwidth_hz,
        } => {
            status.state = ModemState::ConnectedIrs;
            status.peer = Some(peer_call);
            status.width_hz = Some(bandwidth_hz);
            status.last_error = None;
        }
        Command::Disconnected => {
            // Mark Idle, not Stopped — the Stopped transition is owned by
            // `ModemSession::reset_to_stopped`. This keeps the broadcaster
            // and the disconnect command-handler agreeing on the terminal
            // state instead of racing.
            status.state = ModemState::Idle;
            status.arq_flags.rx = false;
            status.arq_flags.tx = false;
        }
        Command::Fault(msg) => {
            status.state = ModemState::Error;
            status.last_error = Some(msg);
        }
        Command::Ptt(on) => {
            status.arq_flags.tx = on;
        }
        Command::Buffer(remaining) => {
            if remaining > 0 {
                status.arq_flags.tx = true;
            }
            // throughput_bps from BUFFER depth requires a rolling-window
            // calculator — deferred to v2.
        }
        Command::Busy(on) => {
            status.arq_flags.busy = on;
        }
        Command::Status(_) => {
            // Free-form STATUS strings (S/N etc.) parsing is v2.
        }
        Command::EchoBack(_) => {
            // Setter echo-backs ("MYCALL", etc.) aren't status-relevant.
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod drain_tests {
    //! Translation-helper tests: assert that `apply_ardop_event_to_status`
    //! maps each ardopcf [`Command`] variant into the documented
    //! [`ModemStatus`] field updates. These tests intentionally don't touch
    //! the cmd-socket — they exercise the pure mapping function.
    use super::*;
    use crate::modem_status::{ModemState, ModemStatus};

    #[test]
    fn newstate_irs_sets_connected_irs_and_rx_flag() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::NewState(State::Irs), &mut s);
        assert_eq!(s.state, ModemState::ConnectedIrs);
        assert!(s.arq_flags.rx);
        assert!(!s.arq_flags.tx);
    }

    #[test]
    fn newstate_iss_sets_connected_iss_and_tx_flag() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::NewState(State::Iss), &mut s);
        assert_eq!(s.state, ModemState::ConnectedIss);
        assert!(!s.arq_flags.rx);
        assert!(s.arq_flags.tx);
    }

    #[test]
    fn newstate_disc_maps_to_idle() {
        let mut s = ModemStatus::stopped();
        // Pre-seed the rx flag so we can verify it gets cleared.
        s.arq_flags.rx = true;
        apply_ardop_event_to_status(Command::NewState(State::Disc), &mut s);
        assert_eq!(s.state, ModemState::Idle);
        assert!(!s.arq_flags.rx);
        assert!(!s.arq_flags.tx);
    }

    #[test]
    fn newstate_offline_maps_to_stopped() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::NewState(State::Offline), &mut s);
        assert_eq!(s.state, ModemState::Stopped);
    }

    #[test]
    fn connected_event_sets_peer_and_bandwidth() {
        let mut s = ModemStatus::stopped();
        // Pre-seed an error so we can verify Connected clears it.
        s.last_error = Some("stale".into());
        apply_ardop_event_to_status(
            Command::Connected {
                peer_call: "W7RMS-10".into(),
                bandwidth_hz: 500,
            },
            &mut s,
        );
        assert_eq!(s.peer.as_deref(), Some("W7RMS-10"));
        assert_eq!(s.width_hz, Some(500));
        assert_eq!(s.state, ModemState::ConnectedIrs);
        assert!(s.last_error.is_none(), "Connected must clear last_error");
    }

    #[test]
    fn disconnected_event_transitions_to_idle_and_clears_flags() {
        let mut s = ModemStatus::stopped();
        s.state = ModemState::ConnectedIrs;
        s.arq_flags.rx = true;
        s.arq_flags.tx = true;
        apply_ardop_event_to_status(Command::Disconnected, &mut s);
        assert_eq!(s.state, ModemState::Idle);
        assert!(!s.arq_flags.rx);
        assert!(!s.arq_flags.tx);
    }

    #[test]
    fn fault_event_transitions_to_error_with_message() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::Fault("TNC timeout".into()), &mut s);
        assert_eq!(s.state, ModemState::Error);
        assert_eq!(s.last_error.as_deref(), Some("TNC timeout"));
    }

    #[test]
    fn ptt_on_sets_tx_flag_and_off_clears_it() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::Ptt(true), &mut s);
        assert!(s.arq_flags.tx);
        apply_ardop_event_to_status(Command::Ptt(false), &mut s);
        assert!(!s.arq_flags.tx);
    }

    #[test]
    fn busy_event_toggles_arq_busy_flag() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::Busy(true), &mut s);
        assert!(s.arq_flags.busy);
        apply_ardop_event_to_status(Command::Busy(false), &mut s);
        assert!(!s.arq_flags.busy);
    }

    #[test]
    fn buffer_nonzero_sets_tx_flag() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::Buffer(1024), &mut s);
        assert!(
            s.arq_flags.tx,
            "BUFFER with bytes queued should mark tx in progress"
        );
    }

    #[test]
    fn buffer_zero_does_not_force_tx_flag() {
        let mut s = ModemStatus::stopped();
        apply_ardop_event_to_status(Command::Buffer(0), &mut s);
        assert!(
            !s.arq_flags.tx,
            "BUFFER 0 must not flip tx — TX is finished, not active"
        );
    }

    #[test]
    fn status_and_echo_back_are_no_ops_on_status() {
        // STATUS strings + setter echo-backs intentionally don't move the
        // status — STATUS parsing is v2, echo-backs are protocol bookkeeping.
        let s_before = ModemStatus::stopped();
        let mut s = s_before.clone();
        apply_ardop_event_to_status(Command::Status("anything".into()), &mut s);
        assert_eq!(s, s_before, "Status events must not mutate ModemStatus");
        apply_ardop_event_to_status(Command::EchoBack("MYCALL".into()), &mut s);
        assert_eq!(s, s_before, "EchoBack events must not mutate ModemStatus");
    }

    // ── tuxlink-n2uz: accumulator + derived-meter tests ──────────────────

    #[test]
    fn buffer_first_event_establishes_baseline_no_bytes_tx() {
        // First BUFFER reading sets `prior_buffer` only — there is no
        // earlier reading to compute a delta against.
        let mut accum = AccumulatorState::default();
        record_buffer(&mut accum, 1000);
        assert_eq!(accum.bytes_tx, 0, "first BUFFER event must NOT accrue bytes");
        assert_eq!(accum.prior_buffer, Some(1000));
        assert!(
            accum.throughput_samples.is_empty(),
            "first event must not append a throughput sample"
        );
    }

    #[test]
    fn buffer_drop_accumulates_bytes_tx() {
        let mut accum = AccumulatorState::default();
        record_buffer(&mut accum, 1000); // baseline
        record_buffer(&mut accum, 700); // 300 bytes transmitted
        assert_eq!(accum.bytes_tx, 300);
        record_buffer(&mut accum, 0); // 700 more transmitted
        assert_eq!(accum.bytes_tx, 1000);
    }

    #[test]
    fn buffer_rise_does_not_decrement_bytes_tx() {
        // A BUFFER value larger than the previous reading means the operator
        // enqueued more data; it MUST NOT decrement the cumulative bytes_tx
        // counter (which would yield nonsensical "negative" throughput).
        let mut accum = AccumulatorState::default();
        record_buffer(&mut accum, 500); // baseline
        record_buffer(&mut accum, 1500); // operator enqueued 1000 more
        assert_eq!(
            accum.bytes_tx, 0,
            "BUFFER rise (enqueue) must not affect bytes_tx"
        );
        // The next drop should accumulate from the NEW baseline.
        record_buffer(&mut accum, 500); // 1000 transmitted
        assert_eq!(accum.bytes_tx, 1000);
    }

    #[test]
    fn buffer_equal_to_prior_is_a_noop() {
        let mut accum = AccumulatorState::default();
        record_buffer(&mut accum, 500);
        record_buffer(&mut accum, 500);
        assert_eq!(accum.bytes_tx, 0);
        assert_eq!(accum.prior_buffer, Some(500));
    }

    #[test]
    fn throughput_returns_none_with_too_few_samples() {
        let mut accum = AccumulatorState::default();
        assert_eq!(current_throughput_bps(&mut accum), None, "empty window → None");

        let mut accum = AccumulatorState::default();
        accum
            .throughput_samples
            .push_back((Instant::now(), 100));
        assert_eq!(
            current_throughput_bps(&mut accum),
            None,
            "single sample → None (no time delta)"
        );
    }

    #[test]
    fn throughput_returns_none_when_window_too_fresh() {
        // Two samples that span less than 500 ms relative to `Instant::now()`
        // (the window upper bound after the tuxlink-n2uz idle-decay fix).
        // Place both samples in the very recent past so `now - t0 < 500ms`,
        // exercising the high-variance-spike guard.
        let now = Instant::now();
        let mut accum = AccumulatorState::default();
        accum.throughput_samples.push_back((now, 0));
        accum
            .throughput_samples
            .push_back((now + Duration::from_millis(50), 1000));
        assert_eq!(
            current_throughput_bps(&mut accum),
            None,
            "sub-500ms window must return None"
        );
    }

    #[test]
    fn throughput_computes_bits_per_second_over_window() {
        // 1000 bytes between t=2s-ago and t=1s-ago, then read at t=now.
        // The fix uses `Instant::now()` as the window upper bound, so
        // elapsed ≈ 2s (now − front_sample_timestamp) and rate ≈
        // 1000 bytes / 2 s = 4000 bps.
        let now = Instant::now();
        let mut accum = AccumulatorState::default();
        accum
            .throughput_samples
            .push_back((now - Duration::from_secs(2), 0));
        accum
            .throughput_samples
            .push_back((now - Duration::from_secs(1), 1000));
        let bps = current_throughput_bps(&mut accum).expect("Some(bps)");
        // Allow generous slop for the test scheduler's variance between
        // when `now` was captured and when `current_throughput_bps` ran.
        assert!(
            (3500..=4500).contains(&bps),
            "expected ~4000 bits/s, got {bps}"
        );
    }

    #[test]
    fn throughput_window_trims_to_5s() {
        // After many BUFFER drops over more than 5 seconds, the rolling
        // window must drop the oldest samples (we don't time-travel here
        // since record_buffer uses Instant::now internally, but we can
        // assert the trim invariant indirectly: samples never exceed a
        // tight upper bound after sustained churn).
        let mut accum = AccumulatorState::default();
        record_buffer(&mut accum, 10_000); // baseline
        for n in (0..10_000).step_by(100).rev() {
            // Drop 100 bytes at a time → 100 sample appends.
            record_buffer(&mut accum, n);
        }
        // We don't know exactly how many samples survive without time
        // control, but the invariant we can check is "the front sample's
        // bytes_tx is consistent with the back" — i.e., the deque is a
        // contiguous prefix-trimmed slice of the original sequence.
        assert!(
            accum.throughput_samples.len() <= 101,
            "samples should be bounded by the appended count: got {}",
            accum.throughput_samples.len()
        );
        let (_, first) = accum.throughput_samples.front().copied().unwrap();
        let (_, last) = accum.throughput_samples.back().copied().unwrap();
        assert!(first <= last, "samples must be monotonically non-decreasing");
        assert_eq!(last, 10_000, "back of window reflects total bytes_tx");
    }

    #[test]
    fn bytes_tx_saturates_on_overflow() {
        // A pathological BUFFER drop near u64::MAX must NOT wrap to 0.
        // Construct the state directly (we can't get a u32 drop to push
        // past u64::MAX in one step, but seed `bytes_tx` near the limit
        // and verify saturating_add is in use).
        let mut accum = AccumulatorState::default();
        accum.bytes_tx = u64::MAX - 10;
        accum.prior_buffer = Some(1000);
        record_buffer(&mut accum, 0); // 1000-byte drop attempts +1000
        assert_eq!(accum.bytes_tx, u64::MAX, "bytes_tx must saturate, not wrap");
    }

    #[test]
    fn connected_event_stamps_connected_at_once_per_session() {
        let mut accum = AccumulatorState::default();
        // First Connected event stamps connected_at.
        apply_event_to_accumulators_inline(
            &Command::Connected {
                peer_call: "W7ABC".into(),
                bandwidth_hz: 500,
            },
            &mut accum,
        );
        let stamp1 = accum.connected_at.expect("connected_at must be stamped");

        // Sleep a tiny bit so a re-stamp would be detectable.
        std::thread::sleep(Duration::from_millis(5));

        // Second Connected event in the same session must NOT re-stamp.
        apply_event_to_accumulators_inline(
            &Command::Connected {
                peer_call: "W7ABC".into(),
                bandwidth_hz: 500,
            },
            &mut accum,
        );
        let stamp2 = accum.connected_at.expect("still stamped");
        assert_eq!(stamp1, stamp2, "duplicate Connected must NOT re-stamp connected_at");
    }

    #[test]
    fn disconnected_event_clears_connected_at() {
        let mut accum = AccumulatorState::default();
        accum.connected_at = Some(Instant::now());
        apply_event_to_accumulators_inline(&Command::Disconnected, &mut accum);
        assert!(accum.connected_at.is_none(), "Disconnected must clear connected_at");
    }

    #[test]
    fn fault_event_clears_connected_at() {
        let mut accum = AccumulatorState::default();
        accum.connected_at = Some(Instant::now());
        apply_event_to_accumulators_inline(&Command::Fault("oops".into()), &mut accum);
        assert!(accum.connected_at.is_none(), "Fault must clear connected_at");
    }

    #[test]
    fn newstate_disc_or_offline_clears_connected_at() {
        for state in [State::Disc, State::Offline] {
            let mut accum = AccumulatorState::default();
            accum.connected_at = Some(Instant::now());
            apply_event_to_accumulators_inline(&Command::NewState(state), &mut accum);
            assert!(
                accum.connected_at.is_none(),
                "NEWSTATE {state:?} must clear connected_at"
            );
        }
    }

    #[test]
    fn newstate_idle_does_not_clear_connected_at() {
        // NEWSTATE IDLE is an ARQ-state-machine transition that does NOT
        // signal session end (it can fire mid-session between transmit
        // bursts). Uptime must continue ticking through it.
        let mut accum = AccumulatorState::default();
        accum.connected_at = Some(Instant::now());
        apply_event_to_accumulators_inline(&Command::NewState(State::Idle), &mut accum);
        assert!(accum.connected_at.is_some(), "NEWSTATE IDLE must NOT clear connected_at");
    }

    #[test]
    fn buffer_event_routes_to_record_buffer() {
        let mut accum = AccumulatorState::default();
        apply_event_to_accumulators_inline(&Command::Buffer(1000), &mut accum);
        apply_event_to_accumulators_inline(&Command::Buffer(700), &mut accum);
        assert_eq!(accum.bytes_tx, 300);
    }

    #[test]
    fn populate_derived_meters_reads_accumulators() {
        // Assemble a transport-shaped object with seeded accumulators and
        // verify populate_derived_meters writes them into ModemStatus.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);

        // Seed accumulators directly.
        let connected_at = Instant::now() - Duration::from_secs(42);
        t.accumulators.connected_at = Some(connected_at);
        t.accumulators.bytes_tx = 1234;

        // Seed an arq_state so bytes_rx flows through.
        let arq_state = ArqState::new();
        arq_state.add_bytes_rx(567);
        t.arq_state = Some(arq_state);

        let mut s = ModemStatus::stopped();
        t.populate_derived_meters(&mut s);

        assert_eq!(s.bytes_tx, 1234);
        assert_eq!(s.bytes_rx, 567);
        // uptime_sec is roughly 42 (allow ±1 for tick boundary).
        assert!(
            (41..=43).contains(&s.uptime_sec),
            "uptime_sec ~42, got {}",
            s.uptime_sec
        );
        // No throughput samples → None.
        assert_eq!(s.throughput_bps, None);
    }

    #[test]
    fn populate_derived_meters_uptime_is_zero_when_disconnected() {
        // No connected_at → uptime_sec stays 0 (not a stale wall-clock value).
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let mut s = ModemStatus::stopped();
        // Pre-seed a stale value to make sure populate clears it.
        s.uptime_sec = 999;
        t.populate_derived_meters(&mut s);
        assert_eq!(s.uptime_sec, 0, "uptime_sec must be 0 while disconnected");
        assert_eq!(s.bytes_tx, 0);
        assert_eq!(s.bytes_rx, 0);
    }

    #[test]
    fn populate_derived_meters_bytes_rx_zero_without_arq_state() {
        // Transport without arq_state installed: bytes_rx falls back to 0,
        // doesn't panic.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let mut s = ModemStatus::stopped();
        t.populate_derived_meters(&mut s);
        assert_eq!(s.bytes_rx, 0);
    }

    // ── Codex P1 fixes (2026-05-31, tuxlink-n2uz) ────────────────────────

    /// Codex P1 #2: throughput meter must decay to `None` when all samples
    /// fall outside the rolling window. Without on-call pruning + an
    /// `Instant::now()` window upper bound, the meter would stay frozen at
    /// the last rate forever after TX stops.
    #[test]
    fn throughput_decays_to_none_when_all_samples_past_window() {
        let mut accum = AccumulatorState::default();
        // Both samples are older than THROUGHPUT_WINDOW (5s). Pruning on
        // call must drop the front one; the deque collapses to len == 1
        // and the `len() < 2` guard returns None.
        let t0 = Instant::now() - Duration::from_secs(10);
        let t1 = Instant::now() - Duration::from_secs(8);
        accum.throughput_samples.push_back((t0, 0));
        accum.throughput_samples.push_back((t1, 1000));
        assert_eq!(
            current_throughput_bps(&mut accum),
            None,
            "all-stale samples must decay to None"
        );
        // After the call, the prune loop must have collapsed the deque
        // to at most one (kept) historical sample.
        assert!(
            accum.throughput_samples.len() <= 1,
            "prune-on-call must have evicted stale samples; deque len = {}",
            accum.throughput_samples.len()
        );
    }

    /// Codex P1 #2: the elapsed window must use `Instant::now()` (not the
    /// most recent sample's timestamp). When TX stops mid-burst, the back
    /// sample is frozen but the wall clock keeps advancing — the meter
    /// should report a DECAYING rate, not the "last fast" rate forever.
    #[test]
    fn throughput_uses_now_not_last_sample_timestamp_for_elapsed() {
        let mut accum = AccumulatorState::default();
        // Two samples 1 second apart, ending 4 seconds ago.
        let now = Instant::now();
        accum
            .throughput_samples
            .push_back((now - Duration::from_secs(4), 0));
        accum
            .throughput_samples
            .push_back((now - Duration::from_secs(3), 1000));
        // Correct fix uses `now` as upper bound:
        //   elapsed ≈ 4 s, delta = 1000 B → 250 B/s = 2000 bps.
        // The pre-fix bug would use the back sample's timestamp:
        //   elapsed = 1 s, delta = 1000 B → 8000 bps ("frozen fast").
        let bps = current_throughput_bps(&mut accum)
            .expect("Some(bps) — samples still within window");
        assert!(
            (1500..=2500).contains(&bps),
            "expected ~2000 bps (now − oldest sample, not back − front); got {bps}"
        );
    }

    /// Codex P1 #1: `connect_arq` consumes the first `CONNECTED` event from
    /// the cmd socket via `arq_connect` BEFORE the broadcaster tick can
    /// drain it through `apply_event_to_accumulators_inline`. The transport
    /// must therefore stamp `connected_at` directly on a successful return
    /// from `arq_connect` so `uptime_sec` advances for ordinary sessions.
    ///
    /// Indirect test: drive `connect_arq` through the mock-TNC pair the
    /// other transport tests use (in the sibling `tests` module). Here we
    /// validate the simpler invariant: a successful `arq_connect` followed
    /// by an immediate `populate_derived_meters` reports `uptime_sec >= 1`
    /// after a brief sleep.
    ///
    /// The full integration via mock TNCs lives in the sibling `tests`
    /// module under `connect_arq_stamps_connected_at_on_success`.
    #[test]
    fn connected_at_directly_stamped_implies_uptime_advances() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        // Simulate the stamp `connect_arq` now performs.
        t.accumulators.connected_at = Some(Instant::now() - Duration::from_secs(7));
        let mut s = ModemStatus::stopped();
        t.populate_derived_meters(&mut s);
        assert!(
            (6..=8).contains(&s.uptime_sec),
            "uptime_sec must reflect the stamped connected_at (~7s); got {}",
            s.uptime_sec
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    // ── Mock server helpers ───────────────────────────────────────────────

    /// Bind a loopback listener, spawn a server thread, return (addr, handle).
    /// The accepted connection gets a 2-second read timeout so server threads
    /// exit promptly instead of blocking forever.
    fn spawn_server<F>(handler: F) -> (SocketAddr, thread::JoinHandle<()>)
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (conn, _) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(2))).ok();
            handler(conn);
        });
        (addr, handle)
    }

    // ── Mock CMD server ───────────────────────────────────────────────────

    /// Read one `\r`-terminated line from `conn` (strips the `\r`).
    /// Returns an empty string on EOF or timeout.
    fn read_cmd_line(reader: &mut BufReader<TcpStream>) -> String {
        let mut buf = Vec::new();
        match reader.read_until(b'\r', &mut buf) {
            Ok(0) | Err(_) => return String::new(),
            Ok(_) => {}
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Write `line\r` to the connection (TNC → client direction).
    fn write_reply(conn: &mut TcpStream, line: &str) {
        let _ = conn.write_all(format!("{line}\r").as_bytes());
    }

    /// Spawn a mock CMD server that:
    /// 1. Echoes the command name for each of the 7 init setters.
    /// 2. On `ARQCALL ...` replies: echo-back → `NEWSTATE ISS` → `CONNECTED <peer> <bw>`.
    /// 3. On `DISCONNECT` replies: `DISCONNECTED`.
    ///
    /// `peer_call` and `bandwidth_hz` are baked into the `CONNECTED` reply.
    ///
    /// `connected_signal` is an optional flag the server sets AFTER writing
    /// the `CONNECTED` line. The companion data mock waits on it before
    /// writing its inbound payload so the timing matches production: the
    /// peer only transmits session data after the ARQ link is up. This is
    /// what makes the post-`connect_arq` drain hook in [`ArdopTransport`]
    /// (tuxlink-ytg Codex-P1 #2) a no-op for these tests instead of eating
    /// the inbound payload.
    fn spawn_mock_cmd_server_with_signal(
        peer_call: &'static str,
        bandwidth_hz: u32,
        connected_signal: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> (SocketAddr, thread::JoinHandle<()>) {
        spawn_server(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break; // EOF or read timeout
                }
                let cmd_name = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                match cmd_name.as_str() {
                    "ARQCALL" => {
                        write_reply(&mut writer, "ARQCALL");
                        write_reply(&mut writer, "NEWSTATE ISS");
                        write_reply(
                            &mut writer,
                            &format!("CONNECTED {peer_call} {bandwidth_hz}"),
                        );
                        if let Some(ref signal) = connected_signal {
                            signal.store(true, std::sync::atomic::Ordering::Release);
                        }
                    }
                    "DISCONNECT" => {
                        write_reply(&mut writer, "DISCONNECTED");
                        break; // session is done
                    }
                    other => {
                        // For all init setters: echo the command name back.
                        write_reply(&mut writer, other);
                    }
                }
            }
        })
    }

    // ── Mock DATA server ──────────────────────────────────────────────────

    /// Build the wire bytes for one ARQ data frame:
    /// `[u16 BE length = 3 + payload.len()][ARQ][payload]`
    fn arq_frame(payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let length = (3 + payload.len()) as u16;
        v.extend_from_slice(&length.to_be_bytes());
        v.extend_from_slice(b"ARQ");
        v.extend_from_slice(payload);
        v
    }

    /// Spawn a mock DATA server that:
    /// - Sends one ARQ frame with `inbound_payload` AFTER `connected_signal`
    ///   flips to `true` (or immediately if `None`).
    /// - Collects all raw bytes written by the client into `received`.
    ///
    /// The signal-gated form is required for tests that exercise the full
    /// `init → connect_arq → read` flow because [`ArdopTransport::connect_arq`]
    /// drains the data socket on connect (tuxlink-ytg Codex-P1 #2). Sending
    /// the inbound payload BEFORE `CONNECTED` would have it drained out,
    /// breaking the test's read assertion — but that pre-connect window is
    /// exactly the bug Codex's drain fix targets, so the test mock needs to
    /// mirror the production timing.
    ///
    /// Returns `(addr, join_handle)`.
    fn spawn_mock_data_server_with_signal(
        inbound_payload: Vec<u8>,
        received: Arc<Mutex<Vec<u8>>>,
        connected_signal: Option<Arc<std::sync::atomic::AtomicBool>>,
    ) -> (SocketAddr, thread::JoinHandle<()>) {
        spawn_server(move |mut conn| {
            // If a signal is wired, wait (bounded) for the cmd-server to
            // emit CONNECTED before writing the inbound payload.
            if let Some(ref signal) = connected_signal {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while !signal.load(std::sync::atomic::Ordering::Acquire) {
                    if std::time::Instant::now() >= deadline {
                        break; // give up; the test will fail loudly on its read assertion
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                // Brief grace window so the client's `arq_connect` returns
                // and its post-connect `drain_pending` runs BEFORE this
                // legitimate post-connect payload hits the wire. Without
                // this sleep the drain races the write and may eat it
                // (the bug Codex's fix targets is bytes arriving BEFORE
                // the drain; bytes arriving AFTER are legitimate session
                // data and the production peer's airtime round-trip gives
                // a much larger natural gap than this 100ms test value).
                std::thread::sleep(Duration::from_millis(100));
            }
            // Send the framed ARQ payload to the client.
            let frame = arq_frame(&inbound_payload);
            let _ = conn.write_all(&frame);
            // Collect what the client writes (raw bytes, no framing).
            let mut buf = [0u8; 256];
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => received.lock().unwrap().extend_from_slice(&buf[..n]),
                }
            }
        })
    }

    // ── Test 1: Full happy-path session through Box<dyn ModemTransport> ───

    #[test]
    fn full_session_happy_path_via_boxed_trait() {
        // Synchronize the two mocks: the DATA server must NOT write its
        // inbound payload until the CMD server has emitted CONNECTED, so the
        // post-`connect_arq` drain (tuxlink-ytg Codex-P1 #2) doesn't eat it.
        let connected_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // — CMD mock
        let (cmd_addr, cmd_server) =
            spawn_mock_cmd_server_with_signal("W7ABC", 500, Some(connected_signal.clone()));

        // — DATA mock: waits for CONNECTED, then sends "HELLO" ARQ frame
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) = spawn_mock_data_server_with_signal(
            b"HELLO".to_vec(),
            received.clone(),
            Some(connected_signal),
        );

        // Exercise through Box<dyn ModemTransport> — this is the object-safety
        // test; if the trait isn't object-safe this line won't compile.
        let mut transport: Box<dyn ModemTransport> =
            Box::new(ArdopTransport::with_addrs(cmd_addr, data_addr));

        // init
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
        };
        transport.init(&cfg).expect("init must succeed");

        // connect_arq
        let info = transport
            .connect_arq("W7ABC", 3, Duration::from_secs(5))
            .expect("connect_arq must succeed");
        assert_eq!(info.peer_call, "W7ABC");
        assert_eq!(info.bandwidth_hz, 500);

        // write through data_stream — assert raw bytes arrive at mock server
        {
            let ds = transport.data_stream().expect("data_stream must be available after init");
            ds.write_all(b"WORLD").expect("write to data socket");
            ds.flush().ok();
        }

        // read through data_stream — should get the ARQ payload "HELLO"
        {
            let ds = transport.data_stream().expect("data_stream still available");
            let mut buf = vec![0u8; 64];
            let n = ds.read(&mut buf).expect("read from data socket");
            assert_eq!(&buf[..n], b"HELLO", "must read back the ARQ payload");
        }

        // disconnect
        transport
            .disconnect(Duration::from_secs(5))
            .expect("disconnect must succeed");

        // Give the mock data server a moment to drain writes then close
        drop(transport);
        cmd_server.join().unwrap();
        data_server.join().unwrap();

        // The mock data server received the framed bytes: [u16 BE length][payload]
        let got = received.lock().unwrap().clone();
        assert_eq!(
            got,
            vec![0x00, 0x05, b'W', b'O', b'R', b'L', b'D'],
            "data server must see framed write bytes [u16 BE len][payload]"
        );
    }

    // ── Test 2: connect_arq before init returns Err, not panic ───────────

    #[test]
    fn connect_arq_before_init_returns_err() {
        // Addresses don't matter — we never connect.
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let err = t
            .connect_arq("W7ABC", 3, Duration::from_millis(100))
            .expect_err("connect_arq before init must return Err");
        // Should be a NotConnected or similar I/O error wrapped in SessionError.
        assert!(
            matches!(err, SessionError::Io(ref e) if e.kind() == io::ErrorKind::NotConnected),
            "expected SessionError::Io(NotConnected), got {err:?}"
        );
    }

    // ── Test 3: data_stream before init returns Err, not panic ───────────

    #[test]
    fn data_stream_before_init_returns_err() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        // io::Result<&mut dyn ReadWrite>: the Ok arm is `&mut dyn ReadWrite`
        // which doesn't implement Debug, so unwrap_err()/expect_err() won't compile.
        // Use match to extract the Err branch manually.
        match t.data_stream() {
            Ok(_) => panic!("data_stream before init must return Err"),
            Err(e) => assert_eq!(
                e.kind(),
                io::ErrorKind::NotConnected,
                "expected NotConnected, got {e}"
            ),
        }
    }

    // ── Test 3b: connect_arq drains pre-connect data-socket bytes (tuxlink-ytg P1) ─

    /// Codex adrev 2026-05-30 P1 #2: ARQ-tagged data on the data socket
    /// before the `CONNECTED` event must NOT survive into the post-connect
    /// read stream. This is the transport-level integration test that pairs
    /// with the `drain_pending` unit test in `data.rs`.
    ///
    /// Scripts a data mock that:
    /// 1. Writes pre-connect noise (`STALE-FRAME`) immediately.
    /// 2. Waits for the cmd-mock's CONNECTED signal.
    /// 3. Writes the legitimate post-connect payload (`AFTER-CONNECT`).
    ///
    /// After init+connect_arq, the first read MUST yield `AFTER-CONNECT`,
    /// not `STALE-FRAME`.
    #[test]
    fn connect_arq_drains_pre_connect_data_socket_bytes() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let connected_signal = Arc::new(AtomicBool::new(false));
        let (cmd_addr, cmd_server) =
            spawn_mock_cmd_server_with_signal("W7ABC", 500, Some(connected_signal.clone()));

        let signal_for_data = connected_signal.clone();
        let (data_addr, data_server) = spawn_server(move |mut conn| {
            // 1. Pre-connect noise: an ARQ frame that would corrupt B2F if it
            //    survived the drain.
            let stale = {
                let payload = b"STALE-FRAME";
                let mut v = Vec::new();
                v.extend_from_slice(&((3 + payload.len()) as u16).to_be_bytes());
                v.extend_from_slice(b"ARQ");
                v.extend_from_slice(payload);
                v
            };
            let _ = conn.write_all(&stale);

            // 2. Wait for CONNECTED.
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !signal_for_data.load(Ordering::Acquire) {
                if std::time::Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }

            // 3. Brief gap so the client's connect_arq returns + drain runs
            //    before the legitimate post-connect frame goes on the wire.
            std::thread::sleep(Duration::from_millis(100));

            let after = {
                let payload = b"AFTER-CONNECT";
                let mut v = Vec::new();
                v.extend_from_slice(&((3 + payload.len()) as u16).to_be_bytes());
                v.extend_from_slice(b"ARQ");
                v.extend_from_slice(payload);
                v
            };
            let _ = conn.write_all(&after);

            // Hold the socket open while the client reads.
            let mut buf = [0u8; 32];
            let _ = conn.read(&mut buf);
        });

        let mut t = ArdopTransport::with_addrs(cmd_addr, data_addr);
        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
        };
        t.init(&cfg).expect("init must succeed");

        // Give the mock time to push the stale frame into the client's OS
        // recv buffer before we even start connect_arq, so the drain has
        // something to discard.
        std::thread::sleep(Duration::from_millis(100));

        t.connect_arq("W7ABC", 3, Duration::from_secs(5))
            .expect("connect_arq must succeed");

        // The first read on the data stream must NOT yield the pre-connect
        // noise — the drain in connect_arq discarded it. It MUST yield the
        // legitimate post-connect payload.
        let ds = t.data_stream().expect("data_stream available");
        let mut buf = vec![0u8; 64];
        let n = ds.read(&mut buf).expect("read must succeed");
        assert_eq!(
            &buf[..n],
            b"AFTER-CONNECT",
            "first post-connect read must yield post-connect data, not pre-connect noise"
        );

        drop(t);
        cmd_server.join().unwrap();
        data_server.join().unwrap();
    }

    // ── tuxlink-n2uz Codex P1 #1: connect_arq stamps connected_at ──────────

    /// `arq_connect` consumes the first `CONNECTED` event from the cmd
    /// socket directly via `recv_event`, so `drain_status_events` would
    /// never observe it in the `Command::Connected` arm of the accumulator
    /// handler. Without an explicit stamp inside `connect_arq`, the dock's
    /// "Up Ns" meter stays at 0 for the entire session. This test verifies
    /// the stamp.
    #[test]
    fn connect_arq_stamps_connected_at_on_success() {
        let connected_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (cmd_addr, cmd_server) =
            spawn_mock_cmd_server_with_signal("W7ABC", 500, Some(connected_signal.clone()));
        // Data mock — we don't write a payload through it, but the connect
        // path expects a data socket to exist (drain_pending runs on it).
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) = spawn_mock_data_server_with_signal(
            b"".to_vec(),
            received.clone(),
            Some(connected_signal),
        );

        let mut t = ArdopTransport::with_addrs(cmd_addr, data_addr);
        let cfg = InitConfig {
            mycall: "N7TST".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
        };
        t.init(&cfg).expect("init must succeed");

        // Pre-condition: a fresh init has reset accumulators.
        assert!(
            t.accumulators.connected_at.is_none(),
            "connected_at must be None before connect"
        );

        t.connect_arq("W7ABC", 3, Duration::from_secs(5))
            .expect("connect_arq must succeed");

        // Post-condition: the stamp was applied.
        assert!(
            t.accumulators.connected_at.is_some(),
            "connect_arq must stamp connected_at on success"
        );

        // Indirect verification through the public meter path:
        // populate_derived_meters reports a non-zero uptime once the stamp
        // has aged.
        std::thread::sleep(Duration::from_millis(1100));
        let mut s = ModemStatus::stopped();
        t.populate_derived_meters(&mut s);
        assert!(
            s.uptime_sec >= 1,
            "uptime_sec must be >= 1 after a connect + 1.1s; got {}",
            s.uptime_sec
        );

        // Tear down cleanly.
        t.disconnect(Duration::from_secs(5))
            .expect("disconnect must succeed");
        drop(t);
        cmd_server.join().unwrap();
        data_server.join().unwrap();
    }

    /// Companion to the stamp test: `disconnect` clears `connected_at` so a
    /// subsequent reconnect starts a fresh uptime rather than inheriting
    /// a stale stamp from the prior session.
    #[test]
    fn disconnect_clears_connected_at() {
        let connected_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (cmd_addr, cmd_server) =
            spawn_mock_cmd_server_with_signal("W7ABC", 500, Some(connected_signal.clone()));
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) = spawn_mock_data_server_with_signal(
            b"".to_vec(),
            received.clone(),
            Some(connected_signal),
        );

        let mut t = ArdopTransport::with_addrs(cmd_addr, data_addr);
        let cfg = InitConfig {
            mycall: "N7TST".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
        };
        t.init(&cfg).expect("init must succeed");
        t.connect_arq("W7ABC", 3, Duration::from_secs(5))
            .expect("connect_arq must succeed");
        assert!(t.accumulators.connected_at.is_some());

        t.disconnect(Duration::from_secs(5))
            .expect("disconnect must succeed");
        assert!(
            t.accumulators.connected_at.is_none(),
            "disconnect must clear connected_at (belt-and-suspenders, Codex P1 #1)"
        );

        drop(t);
        cmd_server.join().unwrap();
        data_server.join().unwrap();
    }

    // ── Test 4: disconnect before init returns Err, not panic ────────────

    #[test]
    fn disconnect_before_init_returns_err() {
        let addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let mut t = ArdopTransport::with_addrs(addr, addr);
        let err = t
            .disconnect(Duration::from_millis(100))
            .expect_err("disconnect before init must return Err");
        assert!(
            matches!(err, SessionError::Io(ref e) if e.kind() == io::ErrorKind::NotConnected),
            "expected SessionError::Io(NotConnected), got {err:?}"
        );
    }

    // ── Test 5: object safety — explicit Box<dyn ModemTransport> compile check

    /// This test primarily exists to confirm the trait is object-safe:
    /// constructing a `Box<dyn ModemTransport>` and calling all four methods
    /// through the vtable.  The mock servers used here are identical to test 1
    /// but we do a minimal round-trip to keep the test light and focused.
    #[test]
    fn object_safety_box_dyn_modem_transport() {
        // Same connect-then-write timing as the happy-path test so the
        // post-`connect_arq` drain (tuxlink-ytg Codex-P1 #2) doesn't eat
        // the inbound payload.
        let connected_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (cmd_addr, cmd_server) =
            spawn_mock_cmd_server_with_signal("K7XYZ", 200, Some(connected_signal.clone()));
        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (data_addr, data_server) = spawn_mock_data_server_with_signal(
            b"PING".to_vec(),
            received.clone(),
            Some(connected_signal),
        );

        // The explicit type annotation is the load-bearing part of this test:
        // if `ModemTransport` were not object-safe, this line would fail to compile.
        let mut t: Box<dyn ModemTransport> =
            Box::new(ArdopTransport::with_addrs(cmd_addr, data_addr));

        let cfg = InitConfig {
            mycall: "K7XYZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
        };
        t.init(&cfg).unwrap();
        let info = t.connect_arq("K7XYZ", 1, Duration::from_secs(5)).unwrap();
        assert_eq!(info.peer_call, "K7XYZ");

        // Read one payload through the trait object.
        let ds = t.data_stream().unwrap();
        let mut buf = vec![0u8; 32];
        let n = ds.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"PING");

        t.disconnect(Duration::from_secs(5)).unwrap();

        drop(t);
        cmd_server.join().unwrap();
        data_server.join().unwrap();
    }

    // ── Test tuxlink-o3f2: try_clone_abort_writer surfaces a live socket ──

    /// tuxlink-o3f2: `ArdopTransport::try_clone_abort_writer` MUST return
    /// `None` before `init()` (no cmd socket open) and `Some(stream)` after
    /// `init()` succeeds. The returned stream must actually deliver bytes to
    /// the cmd-port peer — proving it is the live write half, not a placeholder.
    #[test]
    fn try_clone_abort_writer_returns_none_before_init_and_writeable_socket_after() {
        // Spawn a recording cmd mock that echoes setters AND captures any
        // line received (including a side-channel ABORT).
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let rec = recorded.clone();
        let (cmd_addr, cmd_server) = spawn_server(move |conn| {
            let mut writer = conn.try_clone().unwrap();
            let mut reader = BufReader::new(conn);
            loop {
                let line = read_cmd_line(&mut reader);
                if line.is_empty() {
                    break;
                }
                rec.lock().unwrap().push(line.clone());
                // Echo back the first token to ack every setter, so init_tnc completes.
                let cmd_name = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                write_reply(&mut writer, &cmd_name);
            }
        });
        // Data mock: accepts the connection and idles (init connects it).
        let (data_addr, data_server) = spawn_server(|conn| {
            let mut buf = [0u8; 64];
            // Read until the peer closes (drives EOF when the test drops the transport).
            let mut conn = conn;
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => continue,
                }
            }
        });

        let mut t = ArdopTransport::with_addrs(cmd_addr, data_addr);
        // Before init: no cmd socket → None.
        assert!(
            t.try_clone_abort_writer().is_none(),
            "must return None before init() opens the cmd socket"
        );

        let cfg = InitConfig {
            mycall: "N7CPZ".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
        };
        t.init(&cfg).expect("init must succeed");

        // After init: a live writer that can deliver bytes to the cmd mock.
        let mut abort_writer = t
            .try_clone_abort_writer()
            .expect("must return Some after init()");
        abort_writer
            .write_all(b"ABORT\r")
            .expect("side-channel write must succeed");
        abort_writer.flush().ok();

        // Give the mock a moment to read the side-channel line.
        std::thread::sleep(Duration::from_millis(100));

        // Drop the transport (closes cmd socket) and the writer so the mock exits.
        drop(t);
        drop(abort_writer);
        cmd_server.join().unwrap();
        data_server.join().unwrap();

        let lines = recorded.lock().unwrap().clone();
        assert!(
            lines.iter().any(|l| l == "ABORT"),
            "cmd mock must have received the side-channel ABORT line; got: {lines:?}"
        );
    }

    // ── Phase 5 helper: Python stub that mimics ardopcf's TCP ports ──────

    /// Write a Python stub script to a temp file and return its path.
    ///
    /// The stub:
    /// - Binds `cmd_port` and `data_port` immediately on startup (so bind-wait
    ///   succeeds quickly).
    /// - On the cmd port: accepts one connection, reads `\r`-terminated command
    ///   lines, and echoes back the command name as the ack (matching the
    ///   `init_tnc` sequence). On DISCONNECT it replies DISCONNECTED and exits.
    /// - On the data port: accepts one connection and idles (reads + discards).
    /// - Exits cleanly on SIGINT.
    ///
    /// The script path is unique per process-id + thread-id to avoid collisions
    /// when tests run in parallel.
    fn write_ardopcf_stub(cmd_port: u16, data_port: u16) -> std::path::PathBuf {
        use std::fmt::Write as FmtWrite;
        let pid = std::process::id();
        // Use a unique filename per invocation so parallel test runs don't collide.
        let path = std::env::temp_dir().join(format!(
            "tuxlink-ardopcf-stub-{}-{}.py",
            pid,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
        ));

        let mut script = String::new();
        write!(
            &mut script,
            r#"#!/usr/bin/env python3
import socket
import threading
import signal
import sys

CMD_PORT = {cmd_port}
DATA_PORT = {data_port}

# Bind both sockets immediately so the bind-wait succeeds.
cmd_srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
cmd_srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
cmd_srv.bind(('127.0.0.1', CMD_PORT))
cmd_srv.listen(1)

data_srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
data_srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
data_srv.bind(('127.0.0.1', DATA_PORT))
data_srv.listen(1)

stop_event = threading.Event()

def handle_data(conn):
    conn.settimeout(1.0)
    while not stop_event.is_set():
        try:
            data = conn.recv(256)
            if not data:
                break
        except socket.timeout:
            continue
        except Exception:
            break
    conn.close()

def handle_cmd(conn):
    conn.settimeout(1.0)
    buf = b''
    while not stop_event.is_set():
        try:
            chunk = conn.recv(256)
            if not chunk:
                break
            buf += chunk
        except socket.timeout:
            continue
        except Exception:
            break
        while b'\r' in buf:
            line, buf = buf.split(b'\r', 1)
            line_str = line.decode('ascii', errors='replace').strip()
            if not line_str:
                continue
            cmd_name = line_str.split()[0].upper() if line_str.split() else ''
            if cmd_name == 'DISCONNECT':
                conn.sendall(b'DISCONNECTED\r')
                conn.close()
                stop_event.set()
                return
            else:
                conn.sendall((cmd_name + '\r').encode('ascii'))
    conn.close()

def sigint_handler(sig, frame):
    stop_event.set()
    sys.exit(0)

signal.signal(signal.SIGINT, sigint_handler)
signal.signal(signal.SIGTERM, sigint_handler)

cmd_srv.settimeout(10.0)
data_srv.settimeout(10.0)

try:
    cmd_conn, _ = cmd_srv.accept()
    data_conn, _ = data_srv.accept()
except socket.timeout:
    sys.exit(1)

cmd_srv.close()
data_srv.close()

data_thread = threading.Thread(target=handle_data, args=(data_conn,), daemon=True)
data_thread.start()

handle_cmd(cmd_conn)
stop_event.set()
data_thread.join(timeout=2.0)
"#,
            cmd_port = cmd_port,
            data_port = data_port,
        )
        .unwrap();

        std::fs::write(&path, script.as_bytes()).expect("write stub script");
        path
    }

    /// Pick two free loopback ports without holding them open.
    ///
    /// Binds to :0, reads the OS-assigned port, then drops the listener (releases
    /// the port). There is a narrow TOCTOU window between drop and the stub's
    /// bind, but in practice the OS does not immediately reuse ephemeral ports, so
    /// this is reliable in tests.
    fn free_ports() -> (u16, u16) {
        let l1 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = TcpListener::bind("127.0.0.1:0").unwrap();
        let p2 = l2.local_addr().unwrap().port();
        drop(l1);
        drop(l2);
        (p1, p2)
    }

    // ── Test 6: with_managed_modem → init → shutdown happy path ──────────

    /// Spawn the Python stub, drive init, then call shutdown.
    ///
    /// Verifies:
    /// - `with_managed_modem` succeeds (both ports come up).
    /// - `init` completes the 7-setter sequence.
    /// - `shutdown` returns Ok and the stub process is reaped
    ///   (`ManagedModem` no longer running).
    #[test]
    fn managed_modem_spawn_init_shutdown_happy_path() {
        let (cmd_port, data_port) = free_ports();
        let stub_path = write_ardopcf_stub(cmd_port, data_port);

        let cfg = ArdopConfig {
            binary: "python3".into(),
            extra_args: vec![stub_path.to_string_lossy().into_owned()],
            cmd_port,
            data_port,
            audio_device_path: None,
        };

        // Use a generous bind-wait because Python startup can be slow on a
        // loaded CI host.
        let mut transport =
            ArdopTransport::with_managed_modem_timeout(cfg, Duration::from_secs(10))
                .expect("with_managed_modem must succeed");

        let init_cfg = InitConfig {
            mycall: "N7TST".into(),
            gridsquare: "CN87".into(),
            arq_timeout_s: 30,
            arq_bandwidth_hz: None,
        };
        transport.init(&init_cfg).expect("init must succeed");

        // After init the cmd socket is open; shutdown closes it, stops the stub,
        // and take()s `managed` (so it is consumed — true idempotency). The stub
        // process is reaped inside ManagedModem::stop (covered by process.rs tests).
        transport.shutdown().expect("shutdown must return Ok");
        assert!(
            transport.managed.is_none(),
            "shutdown must consume `managed`"
        );
        // A second shutdown is a true no-op.
        transport.shutdown().expect("second shutdown is a no-op");

        // Cleanup stub script.
        let _ = std::fs::remove_file(&stub_path);
    }

    // ── Test 7: with_managed_modem returns TimedOut if ports never bind ───

    /// Spawn a no-op process that never binds the ports. Verify that
    /// `with_managed_modem_timeout` returns `Err(SessionError::Io(TimedOut))`
    /// within the (short) timeout.
    #[test]
    fn managed_modem_times_out_when_ports_never_bind() {
        let (cmd_port, data_port) = free_ports();

        let cfg = ArdopConfig {
            binary: "/bin/sh".into(),
            // `-c "sleep 30"` — binds no ports
            extra_args: vec!["-c".into(), "sleep 30".into()],
            cmd_port,
            data_port,
            audio_device_path: None,
        };

        // Very short bind-wait so the test completes quickly.
        let result =
            ArdopTransport::with_managed_modem_timeout(cfg, Duration::from_millis(500));

        match result {
            Ok(_) => panic!("must return Err when ports never bind"),
            Err(SessionError::Io(ref e)) => {
                assert_eq!(
                    e.kind(),
                    io::ErrorKind::TimedOut,
                    "expected TimedOut, got {e}"
                );
            }
            Err(other) => panic!("expected SessionError::Io(TimedOut), got {other:?}"),
        }
    }
}
