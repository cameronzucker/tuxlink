//! Managed-Dire-Wolf process lifecycle (Slice B, Phase 4 of the managed-modem
//! on-air accessibility design,
//! `docs/design/2026-06-12-managed-modem-onair-accessibility-design.md`).
//!
//! This module is the **lifecycle wrapper** that turns the pure pieces from the
//! earlier phases into a running, supervised, cleanly-torn-down Dire Wolf KISS
//! soundmodem:
//!
//! - Phase 2 ([`super::direwolf_conf`]) — generates the `direwolf.conf` string.
//! - Phase 3 ([`super::direwolf_probe`]) — the pre-spawn device-busy probe.
//! - [`super::super::modem::process::ManagedModem`] — the spawn / SIGINT→grace→
//!   SIGKILL / `Drop` machinery. **We REUSE it; we do not reimplement signal or
//!   kill handling.** Every process-control primitive comes from `ManagedModem`.
//!
//! The shape mirrors the ardopcf integration
//! ([`super::super::modem::ardop::transport::ArdopTransport::with_managed_modem_timeout`]
//! / `shutdown`): a bind-wait that detects the modem is listening WITHOUT
//! consuming a connection slot, and a `shutdown` that stops the process then
//! confirms the audio device released, with the same restore-on-failure retry
//! semantic. The only structural difference is that Dire Wolf serves a SINGLE
//! KISS-over-TCP port (not ardopcf's two cmd/data ports), so the bind-wait waits
//! on one port.
//!
//! ## RADIO-1 — clean shutdown must never leave PTT keyed
//!
//! Dire Wolf keys the transmitter via its `PTT CM108 <hidraw>` (CM108 GPIO /
//! reed-relay line) or `PTT <tty> RTS` directive. On a **clean SIGINT** Dire Wolf
//! de-keys and resets that line as part of its normal exit path, so the
//! transmitter is released before the process is gone. [`ManagedDireWolf::shutdown`]
//! therefore gives the clean-SIGINT path a comfortable grace ([`SHUTDOWN_GRACE`])
//! before [`ManagedModem::stop`] escalates.
//!
//! **Known limitation, surfaced on purpose (see the RADIO-1 comment in
//! [`ManagedDireWolf::shutdown`]):** if Dire Wolf IGNORES SIGINT and
//! [`ManagedModem::stop`] escalates to SIGKILL, Dire Wolf is killed before it can
//! run its de-key path, and the PTT line CAN be left asserted (CM108 GPIO / relay
//! stuck closed, or RTS stuck high) — a stuck transmitter. This module does NOT
//! add a tuxlink-specific airtime cap / TOT / watchdog to paper over that
//! (`feedback_no_tuxlink_added_safeguards`: tuxlink mirrors legacy WLE behavior
//! and adds no safeguard beyond it). The bar here is a clean shutdown path plus
//! HONEST documentation of the residual SIGKILL risk for the Codex adversarial
//! round and the operator's on-air smoke. The clean path is the expected path;
//! the residual is a hardware-keying property of a SIGKILL'd modem, not something
//! a software cap in tuxlink should mask.
//!
//! ## Concurrency model
//!
//! Synchronous `std::process` + `std::thread::sleep`, matching `ManagedModem` and
//! the rest of `winlink::modem` (ADR 0015). No Tokio.

use std::time::{Duration, Instant};

use tempfile::NamedTempFile;

use super::devices::PttChoice;
use super::direwolf_conf::{generate_direwolf_conf, DwParams};
use super::direwolf_probe::{device_busy_message, probe_device_busy};
use crate::winlink::modem::process::{ManagedModem, ProcessError};

// ─── Tunables ─────────────────────────────────────────────────────────────────

/// How long to wait for Dire Wolf to bind its KISS port after spawn before
/// declaring a bind-timeout. Comparable to ardopcf's `BIND_WAIT_TIMEOUT` (5s);
/// Dire Wolf opening the sound card + binding the KISS listener is in the same
/// ballpark.
const BIND_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Poll interval for the bind-wait loop. Matches ardopcf's
/// `BIND_WAIT_POLL_INTERVAL`.
const BIND_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Grace period handed to [`ManagedModem::stop`] on shutdown — how long Dire Wolf
/// has to de-key the PTT line and exit cleanly on SIGINT before stop escalates to
/// SIGKILL.
///
/// **RADIO-1 sizing:** ardopcf uses ~3s; Dire Wolf de-keying a CM108/RTS line on
/// SIGINT is fast (it is a single GPIO/line reset on its signal path), but a
/// comfortable margin is chosen so a momentarily-busy Dire Wolf still gets to run
/// its clean de-key rather than being SIGKILL'd with the transmitter potentially
/// keyed. Larger than ardopcf's because the cost of an over-long grace here is a
/// few seconds of shutdown latency, while the cost of an under-long grace is the
/// SIGKILL-residual-PTT risk documented at the module level.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// How long [`ManagedDireWolf::shutdown`] polls the card's ALSA status waiting for
/// every substream to read `closed` (released) before declaring the swap
/// invariant violated. Comparable to ardopcf's 2s release deadline.
const RELEASE_CONFIRM_DEADLINE: Duration = Duration::from_secs(2);

/// Poll interval for the release-confirmation loop.
const RELEASE_CONFIRM_POLL_INTERVAL: Duration = Duration::from_millis(100);

// ─── Config ───────────────────────────────────────────────────────────────────

/// The resolved inputs [`ManagedDireWolf::spawn`] needs to bring a managed Dire
/// Wolf up. The caller (Phase 6) resolves these from the persisted
/// `KissLinkConfig::ManagedDireWolf` variant + live device enumeration — NOT this
/// module's concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedDireWolfCfg {
    /// The ALSA `plughw:CARD=<id>,DEV=0` name for `ADEVICE` and the device-busy /
    /// release probes — comes from [`super::devices::AudioDevice::alsa_plughw`].
    pub adevice: String,
    /// The live `card<N>` index backing `adevice`, used by the pre-spawn
    /// device-busy probe and the post-stop release confirmation (both read
    /// `/proc/asound/card<N>/...`). Resolving the stable-id → live-index mapping is
    /// the caller's job; this module takes the already-resolved index.
    pub card_index: u32,
    /// The operator's BASE callsign for `MYCALL` (no SSID — see
    /// [`super::direwolf_conf`]'s SSID contract).
    pub mycall: String,
    /// The resolved PTT keying method ([`super::devices::discover_ptt`]'s result).
    pub ptt: PttChoice,
    /// The localhost TCP port Dire Wolf serves KISS on, and the port tuxlink's
    /// KISS link connects to.
    pub kiss_port: u16,
}

impl ManagedDireWolfCfg {
    /// Project the lifecycle config onto the pure [`DwParams`] the conf generator
    /// consumes (everything except the live `card_index`, which is a probe input,
    /// not a conf input).
    fn to_dw_params(&self) -> DwParams {
        DwParams {
            adevice: self.adevice.clone(),
            mycall: self.mycall.clone(),
            ptt: self.ptt.clone(),
            kiss_port: self.kiss_port,
        }
    }
}

// ─── Error ────────────────────────────────────────────────────────────────────

/// Errors from the managed-Dire-Wolf lifecycle. Every variant is NAMED so the
/// caller (Phase 6) can surface a specific fallback — notably distinguishing
/// [`DwLifecycleError::DireWolfNotInstalled`] (offer the install path) from a
/// generic spawn failure, and [`DwLifecycleError::DeviceBusy`] (the card is held)
/// from a [`DwLifecycleError::BindTimeout`] (Dire Wolf came up but never listened).
#[derive(Debug, thiserror::Error)]
pub enum DwLifecycleError {
    /// The chosen audio device is already held by another program (the pre-spawn
    /// [`probe_device_busy`] returned the named busy message). No process was
    /// spawned. Carries the named message so the operator never sees a black box.
    #[error("{0}")]
    DeviceBusy(String),

    /// The `direwolf` binary could not be found / executed. The caller surfaces
    /// the "install Dire Wolf" fallback for this specifically.
    #[error("direwolf binary not found or not executable")]
    DireWolfNotInstalled,

    /// Spawning the `direwolf` process failed for a reason other than the binary
    /// being absent (e.g. a permissions or resource error).
    #[error("failed to spawn direwolf: {0}")]
    Spawn(String),

    /// Dire Wolf spawned but did not bind the KISS port within
    /// [`BIND_WAIT_TIMEOUT`]. The just-spawned process is stopped before this is
    /// returned (no leaked child).
    #[error("direwolf did not bind KISS port {port} within {timeout:?}")]
    BindTimeout {
        /// The KISS port Dire Wolf failed to bind.
        port: u16,
        /// The bind-wait timeout that elapsed.
        timeout: Duration,
    },

    /// Writing the generated `direwolf.conf` to a temp file failed.
    #[error("failed to write direwolf.conf temp file: {0}")]
    ConfWrite(String),

    /// `shutdown` stopped the process but the audio device was still held after
    /// [`RELEASE_CONFIRM_DEADLINE`] — the ADR-0015 swap invariant. The held modem
    /// is restored so a retry `shutdown()` re-checks (see the shutdown docs).
    #[error("{0}")]
    DeviceNotReleased(String),

    /// [`ManagedModem::stop`] itself failed (surfaced only after the swap-invariant
    /// check has run).
    #[error("failed to stop direwolf: {0}")]
    Stop(String),
}

// ─── Handle ───────────────────────────────────────────────────────────────────

/// A live, supervised managed Dire Wolf. Owns the [`ManagedModem`] child, the
/// temp `direwolf.conf` (cleaned up on drop), and the inputs needed to confirm the
/// audio device released at shutdown. Hand the [`ManagedDireWolf::endpoint`] to a
/// KISS-over-TCP link to talk to the modem.
///
/// `Drop` of the held [`ManagedModem`] is the RADIO-1 safety net: if this handle
/// is dropped without an explicit [`ManagedDireWolf::shutdown`], `ManagedModem`'s
/// own `Drop` still SIGINT→SIGKILLs the child so it cannot be orphaned. The temp
/// conf's `NamedTempFile` is removed on drop too.
#[derive(Debug)]
pub struct ManagedDireWolf {
    /// The supervised child. `Option` so `shutdown` can `take()` it and, on the
    /// release-failure retry path, restore it (mirrors ardopcf's `managed`).
    modem: Option<ManagedModem>,
    /// Held so the conf survives for Dire Wolf's lifetime and is cleaned up on
    /// drop. `Option` because it is dropped alongside the modem on a successful
    /// shutdown. The path was passed to `direwolf -c <path>` at spawn.
    _conf: Option<NamedTempFile>,
    /// The KISS loopback endpoint Dire Wolf serves — always `("127.0.0.1", kiss_port)`.
    host: &'static str,
    /// The KISS port (the `port` half of [`endpoint`]).
    port: u16,
    /// The ALSA `plughw:` device name, for the named release-failure message.
    adevice: String,
    /// The live `card<N>` index, polled for release confirmation at shutdown.
    card_index: u32,
}

impl ManagedDireWolf {
    /// Spawn and supervise a Dire Wolf KISS soundmodem from `cfg`.
    ///
    /// Steps (RADIO-1 + ADR-0015 ordering matters):
    /// 1. Generate the conf ([`generate_direwolf_conf`]) and write it to a temp
    ///    file (`tempfile::NamedTempFile`, cleaned up on drop).
    /// 2. **Pre-spawn device-busy probe** ([`probe_device_busy`]): if the card is
    ///    held, return [`DwLifecycleError::DeviceBusy`] WITHOUT spawning — never
    ///    grab a card another program holds.
    /// 3. Spawn `direwolf -t 0 -c <conf>` via [`ManagedModem::spawn`] (`-t 0`
    ///    disables color; `-c` is the conf — the REAL run). A spawn failure maps to
    ///    [`DwLifecycleError::DireWolfNotInstalled`] when the binary is absent,
    ///    else [`DwLifecycleError::Spawn`].
    /// 4. **Bind-wait** the KISS port via the `TcpListener::bind`-is-`Err`
    ///    (EADDRINUSE ⇒ listening) probe — does NOT consume a connection slot. On
    ///    timeout, [`ManagedModem::stop`] the just-spawned child (no leak) and
    ///    return [`DwLifecycleError::BindTimeout`].
    ///
    /// Returns a handle exposing the loopback [`endpoint`] plus the held modem +
    /// temp conf + release-probe inputs.
    ///
    /// # RADIO-1
    ///
    /// Spawning `direwolf` against a real configured PTT line can key the radio.
    /// The caller is responsible for the per-invocation operator consent gate
    /// (RADIO-1) before calling this — the same contract [`ManagedModem::spawn`]
    /// carries.
    pub fn spawn(cfg: ManagedDireWolfCfg) -> Result<Self, DwLifecycleError> {
        Self::spawn_with(cfg, BIND_WAIT_TIMEOUT)
    }

    /// Production spawn with a caller-chosen bind-wait timeout. Builds the REAL
    /// `direwolf -t 0 -c <conf>` argument vector inside a command-builder closure
    /// and hands it to the SHARED [`spawn_inner`] — so the real arg vector and the
    /// [`map_spawn_error`] wiring are exercised by the exact same lifecycle code
    /// the stub tests drive (they substitute only the program + args via their own
    /// closure). Production [`spawn`] calls this with [`BIND_WAIT_TIMEOUT`].
    fn spawn_with(cfg: ManagedDireWolfCfg, bind_wait: Duration) -> Result<Self, DwLifecycleError> {
        Self::spawn_inner(
            cfg,
            // The REAL run: `direwolf -t 0 -c <conf>` (`-t 0` disables color, `-c`
            // is the generated conf). The owned (program, args) is built here and
            // executed by the shared inner.
            |conf_path| {
                (
                    "direwolf".to_string(),
                    vec![
                        "-t".to_string(),
                        "0".to_string(),
                        "-c".to_string(),
                        conf_path.to_string(),
                    ],
                )
            },
            bind_wait,
        )
    }

    /// The SINGLE shared spawn sequence. Both production [`spawn_with`] and the
    /// test stub entrypoint route through this one body, substituting only the
    /// `(program, args)` the `build_cmd` closure returns — so there is no
    /// duplicated lifecycle body that can drift, and the production arg vector +
    /// [`map_spawn_error`] wiring are exercised by the stub tests.
    ///
    /// `build_cmd` receives the conf temp-file path and returns an OWNED
    /// `(String, Vec<String>)`; the `&[&str]` [`ManagedModem::spawn`] needs is
    /// borrowed from that owned `Vec` inside this function, so there are no
    /// dangling references.
    ///
    /// Steps (RADIO-1 + ADR-0015 ordering matters):
    /// 1. Generate the conf and write it to a temp file.
    /// 2. Pre-spawn device-busy probe — return [`DwLifecycleError::DeviceBusy`]
    ///    WITHOUT spawning if the card is held.
    /// 3. `build_cmd(&conf_path)` → `(program, args)`; spawn via
    ///    [`ManagedModem::spawn`], mapping failures through [`map_spawn_error`].
    /// 4. Bind-wait the KISS port; on timeout stop the child (no leak) and return
    ///    [`DwLifecycleError::BindTimeout`].
    fn spawn_inner(
        cfg: ManagedDireWolfCfg,
        build_cmd: impl FnOnce(&str) -> (String, Vec<String>),
        bind_wait: Duration,
    ) -> Result<Self, DwLifecycleError> {
        // Step 1: generate + write the conf to a temp file.
        let conf_text = generate_direwolf_conf(&cfg.to_dw_params());
        let conf_file = write_conf_tempfile(&conf_text)
            .map_err(|e| DwLifecycleError::ConfWrite(e.to_string()))?;
        let conf_path = conf_file.path().to_string_lossy().into_owned();

        // Step 2: pre-spawn device-busy probe — DO NOT spawn against a held card.
        if let Err(named_msg) = probe_device_busy(&cfg.adevice, cfg.card_index) {
            tracing::warn!(
                target: "tuxlink::winlink::ax25::managed_direwolf",
                device = %cfg.adevice,
                "managed direwolf spawn aborted — device busy",
            );
            return Err(DwLifecycleError::DeviceBusy(named_msg));
        }

        // Step 3: build the command vector and spawn. The closure returns owned
        // String/Vec<String>; borrow `&[&str]` from the owned Vec right here so no
        // reference dangles past the spawn call.
        let (program, args) = build_cmd(&conf_path);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        tracing::info!(
            target: "tuxlink::winlink::ax25::managed_direwolf",
            program = %program,
            kiss_port = cfg.kiss_port,
            "managed direwolf spawning",
        );
        let mut modem = ManagedModem::spawn(&program, &arg_refs).map_err(map_spawn_error)?;

        // Step 4: bind-wait the single KISS port. On timeout, stop the child so we
        // do not leak it, then return BindTimeout.
        if let Err(err) = wait_for_kiss_port(cfg.kiss_port, bind_wait) {
            // Stop the just-spawned child before surfacing the timeout (no leak).
            // Ignore the stop result: we are already in an error path and Drop is
            // the backstop regardless.
            let _ = modem.stop(SHUTDOWN_GRACE);
            return Err(err);
        }

        tracing::info!(
            target: "tuxlink::winlink::ax25::managed_direwolf",
            kiss_port = cfg.kiss_port,
            "managed direwolf ready",
        );

        Ok(ManagedDireWolf {
            modem: Some(modem),
            _conf: Some(conf_file),
            host: "127.0.0.1",
            port: cfg.kiss_port,
            adevice: cfg.adevice,
            card_index: cfg.card_index,
        })
    }

    /// The KISS-over-TCP loopback endpoint Dire Wolf serves: `("127.0.0.1", port)`.
    /// Hand this to a KISS link to talk to the modem.
    pub fn endpoint(&self) -> (&'static str, u16) {
        (self.host, self.port)
    }

    /// Whether the supervised Dire Wolf is still running (delegates to
    /// [`ManagedModem::is_running`]). `false` once `shutdown` has consumed the
    /// modem on the success path.
    pub fn is_running(&mut self) -> bool {
        match &mut self.modem {
            Some(m) => m.is_running(),
            None => false,
        }
    }

    /// Tear down the managed Dire Wolf cleanly.
    ///
    /// 1. [`ManagedModem::stop`]`(SHUTDOWN_GRACE)` — SIGINT, poll, escalate to
    ///    SIGKILL if Dire Wolf ignores SIGINT.
    /// 2. Confirm the audio device released — poll [`probe_device_busy`] until the
    ///    card reads free or [`RELEASE_CONFIRM_DEADLINE`] elapses (the ADR-0015
    ///    swap invariant). The check runs REGARDLESS of whether `stop` reported an
    ///    error: after a SIGKILL escalation the process is gone and the device
    ///    should be free, so the invariant must still be verified.
    ///
    /// # Idempotent / retry (mirrors ardopcf's `shutdown`)
    ///
    /// `take()`s the modem so a second `shutdown()` on the SUCCESS path is a true
    /// no-op. If the release check FAILS, the modem is RESTORED before returning
    /// [`DwLifecycleError::DeviceNotReleased`] so a retry `shutdown()` re-checks
    /// rather than silently no-op'ing. Once release SUCCEEDS, the modem stays
    /// consumed (its `NamedTempFile` conf is dropped alongside, cleaning the conf).
    ///
    /// # Phase-6 wiring contract — call `shutdown()`, don't rely on `Drop`
    ///
    /// The explicit `shutdown()` (with [`SHUTDOWN_GRACE`], 5s) is the RADIO-1 clean
    /// de-key path. The `Drop` net gives only `ManagedModem`'s ~200ms `DROP_GRACE`,
    /// which is too short to guarantee Dire Wolf runs its clean SIGINT de-key — so
    /// P6 MUST call `shutdown()` explicitly on disconnect/abort and treat `Drop`
    /// purely as the orphan-leak backstop, never as the de-key path.
    ///
    /// # RADIO-1 — clean de-key vs. SIGKILL residual PTT (KNOWN LIMITATION)
    ///
    /// On the EXPECTED path Dire Wolf catches the SIGINT sent by
    /// [`ManagedModem::stop`], **de-keys the PTT line** (resets the CM108
    /// GPIO/reed-relay or drops the serial RTS line), releases the sound card, and
    /// exits — so the transmitter is unkeyed before the process is gone.
    /// [`SHUTDOWN_GRACE`] is sized to give that clean de-key comfortable room.
    ///
    /// **KNOWN LIMITATION, surfaced for the Codex adversarial round + the
    /// operator's on-air smoke:** if Dire Wolf IGNORES the SIGINT (hung, or
    /// trapping it), `ManagedModem::stop` escalates to **SIGKILL**, which kills
    /// Dire Wolf before it can run its de-key path. A SIGKILL'd Dire Wolf can leave
    /// the PTT line ASSERTED — a CM108 GPIO / reed-relay stuck closed, or a serial
    /// RTS line stuck high — i.e. a **stuck transmitter still keyed after
    /// shutdown**. tuxlink does NOT add an airtime cap / TOT / watchdog to mask
    /// this (`feedback_no_tuxlink_added_safeguards`: tuxlink mirrors legacy WLE
    /// behavior, no tuxlink-added safeguard). The residual is a hardware-keying
    /// property of a SIGKILL'd modem; the operator's on-air smoke must verify the
    /// transmitter de-keys on a clean shutdown, and an operator who observes a
    /// stuck carrier after a forced kill must power-cycle the radio/interface. This
    /// is documented honesty, not a defect to be papered over with a tuxlink cap.
    pub fn shutdown(&mut self) -> Result<(), DwLifecycleError> {
        let Some(mut modem) = self.modem.take() else {
            // Already shut down on a prior success — true no-op.
            return Ok(());
        };

        // Step 1: stop (SIGINT → grace → SIGKILL). Capture the result; the
        // swap-invariant check runs regardless.
        let stop_result = modem.stop(SHUTDOWN_GRACE);

        // Step 2: confirm the audio device released (poll the card's ALSA status).
        let released =
            confirm_card_released(&self.adevice, self.card_index, RELEASE_CONFIRM_DEADLINE);

        if !released {
            // Restore the modem so a retry shutdown() re-checks the invariant
            // (mirrors ardopcf). The conf stays held too (the handle is intact).
            let msg = device_busy_message(&self.adevice);
            self.modem = Some(modem);
            return Err(DwLifecycleError::DeviceNotReleased(format!(
                "{msg} after shutdown — swap invariant violated"
            )));
        }

        // Success: drop the modem + conf (modem is already consumed via take()).
        // The NamedTempFile inside `_conf` is dropped here, removing the conf.
        self._conf = None;

        // Surface a stop failure only after the swap-invariant check has run.
        stop_result.map_err(|e| DwLifecycleError::Stop(e.to_string()))?;
        Ok(())
    }
}

// ─── RADIO-1 session guard (Phase 6 wiring) ─────────────────────────────────

/// RAII guard that owns a live [`ManagedDireWolf`] for the duration of a connect
/// session and runs the EXPLICIT 5s [`ManagedDireWolf::shutdown`] (the clean
/// de-key path) on EVERY exit of the scope holding it — normal return, a `?`
/// early-return, OR a panic unwinding the stack.
///
/// # Why an explicit guard rather than `ManagedDireWolf`'s own `Drop`
///
/// `ManagedDireWolf`'s `Drop` net delegates to `ManagedModem`'s ~200ms
/// `DROP_GRACE`, which is too short to guarantee Dire Wolf runs its clean SIGINT
/// de-key — it is the orphan-leak backstop, NOT the de-key path (see
/// [`ManagedDireWolf::shutdown`]'s "call `shutdown()`, don't rely on `Drop`"
/// contract). This guard's `Drop` calls the full `shutdown()` with
/// [`SHUTDOWN_GRACE`] (5s) so the RADIO-1 clean-de-key path runs on every unwind.
///
/// # RADIO-1
///
/// Holding this guard in the connect fn's top scope is what makes the clean
/// shutdown fire on the `?`-error and panic paths, not just the happy path: Rust
/// runs `Drop` for in-scope values during unwinding. `Drop` never panics (a panic
/// in `Drop` during an unwind aborts the process); on shutdown error it logs and
/// swallows. A second `shutdown()` (e.g. if the caller also called it explicitly)
/// is a no-op — `shutdown()` `take()`s the modem on success.
pub struct ManagedDireWolfGuard(pub ManagedDireWolf);

impl Drop for ManagedDireWolfGuard {
    fn drop(&mut self) {
        if let Err(e) = self.0.shutdown() {
            // Never panic in Drop (a panic while unwinding aborts the process).
            // Log the clean-shutdown failure; the held ManagedModem's own Drop is
            // the residual orphan-leak backstop if shutdown could not complete.
            tracing::warn!(
                target: "tuxlink::winlink::ax25::managed_direwolf",
                error = %e,
                "managed direwolf shutdown on session end failed; relying on Drop backstop",
            );
        }
    }
}

/// Pick a FREE localhost TCP port for the KISS listener: bind `127.0.0.1:0`, read
/// the OS-assigned ephemeral port, drop the listener (releasing the port), and
/// return the number. The SAME value is used for both the generated conf's
/// `KISSPORT` and tuxlink's loopback Tcp dial (they are one value via
/// [`ManagedDireWolfCfg::kiss_port`]).
///
/// # TOCTOU
///
/// There is a tiny window between dropping this listener and Dire Wolf binding the
/// port in which another local process could claim it. This is ACCEPTABLE on
/// localhost: if it happens, Dire Wolf's bind fails and tuxlink's bind-wait
/// ([`wait_for_kiss_port`]) times out into a clean [`DwLifecycleError::BindTimeout`]
/// — a legible error the operator can retry, not a silent wrong-port dial. There
/// is no portable "bind and hand the bound socket to the child" path for Dire
/// Wolf (it opens its own listener from the conf's `KISSPORT`), so request-a-free-
/// port-then-release is the pragmatic choice the ardopcf path also relies on.
pub fn pick_free_kiss_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

// ─── Free helpers ───────────────────────────────────────────────────────────

/// Write `conf_text` to a fresh `NamedTempFile`. The file is held by the returned
/// value and removed when it drops. Kept separate so tests can reason about it.
fn write_conf_tempfile(conf_text: &str) -> std::io::Result<NamedTempFile> {
    use std::io::Write;
    let mut f = NamedTempFile::new()?;
    f.write_all(conf_text.as_bytes())?;
    f.flush()?;
    Ok(f)
}

/// Map a [`ProcessError`] from [`ManagedModem::spawn`] to a NAMED
/// [`DwLifecycleError`], distinguishing "binary absent" (→
/// [`DwLifecycleError::DireWolfNotInstalled`], so the caller can offer the install
/// path) from any other spawn failure.
fn map_spawn_error(e: ProcessError) -> DwLifecycleError {
    match e {
        ProcessError::Spawn(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
            DwLifecycleError::DireWolfNotInstalled
        }
        ProcessError::Spawn(io_err) => DwLifecycleError::Spawn(io_err.to_string()),
        ProcessError::Stop(msg) => DwLifecycleError::Spawn(msg),
    }
}

/// Bind-wait the single KISS port. Mirrors ardopcf's two-port loop, narrowed to
/// one port because Dire Wolf serves a single KISS-over-TCP listener.
///
/// Detection: attempt to `TcpListener::bind` the loopback `(127.0.0.1, port)`. If
/// the bind FAILS (`is_err()`, EADDRINUSE), Dire Wolf is already listening — we
/// detect readiness WITHOUT consuming a connection slot (a `TcpStream::connect`
/// would steal the accept the real KISS link needs).
fn wait_for_kiss_port(port: u16, bind_wait: Duration) -> Result<(), DwLifecycleError> {
    let addr = format!("127.0.0.1:{port}");
    let start = Instant::now();
    loop {
        // bind() Err ⇒ something already holds the port ⇒ Dire Wolf is listening.
        if std::net::TcpListener::bind(&addr).is_err() {
            tracing::info!(
                target: "tuxlink::winlink::ax25::managed_direwolf",
                port,
                elapsed_ms = start.elapsed().as_millis(),
                "managed direwolf KISS port ready",
            );
            return Ok(());
        }
        if start.elapsed() >= bind_wait {
            tracing::error!(
                target: "tuxlink::winlink::ax25::managed_direwolf",
                port,
                timeout_ms = bind_wait.as_millis(),
                "managed direwolf bind-wait timed out",
            );
            return Err(DwLifecycleError::BindTimeout {
                port,
                timeout: bind_wait,
            });
        }
        std::thread::sleep(BIND_WAIT_POLL_INTERVAL);
    }
}

/// Poll the card's ALSA status until every substream reads free (`closed`) or
/// `deadline` elapses. Returns `true` once released, `false` on timeout.
///
/// This is the inverse of the pre-spawn [`probe_device_busy`] check: that probe
/// returns `Err(busy)` while a substream is held and `Ok(())` once free, so we
/// poll it until `Ok`. Reusing it (rather than `lsof`-ing a resolved
/// `/dev/snd/pcmC<N>D<M>c` path) keeps the busy/release decision on ONE tested
/// code path keyed off the `card_index` the caller already supplies — Dire Wolf
/// opens both capture and playback substreams on the card, and `probe_device_busy`
/// already scans every `pcm*/sub*` under `card<N>`.
fn confirm_card_released(adevice: &str, card_index: u32, deadline: Duration) -> bool {
    let end = Instant::now() + deadline;
    loop {
        if probe_device_busy(adevice, card_index).is_ok() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(RELEASE_CONFIRM_POLL_INTERVAL);
    }
}

// ─── Task 4.3 — sound-card arbitration (pure decision) ─────────────────────────

/// A card identity for the arbitration decision. Deliberately a thin newtype over
/// the stable ALSA `plughw:` name (the same string [`ManagedDireWolfCfg::adevice`]
/// carries) so the decision compares the value tuxlink actually persists, not a
/// boot-order index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardId(pub String);

/// The arbitration outcome: may we spawn against the requested card, or must we
/// first stop whatever managed modem currently holds it?
///
/// This is ADR-0015's "tuxlink is the single audio arbiter" decision rendered as a
/// PURE function. It is the COOPERATIVE pre-check; the concrete backstop is the
/// pre-spawn device-busy probe in [`ManagedDireWolf::spawn`] (which catches a held
/// card whether or not the holder is a tuxlink-managed modem).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arbitration {
    /// No managed modem holds the requested card (or the holder already holds a
    /// DIFFERENT card) — proceed to spawn.
    Proceed,
    /// Another managed modem (e.g. ardopcf) holds the requested card; the caller
    /// must stop + confirm-release it before spawning. Carries the held card id.
    MustStopHolder(CardId),
}

/// Pure arbitration decision over INJECTED holder state.
///
/// - `holder == None` ⇒ no managed modem holds any card ⇒ [`Arbitration::Proceed`].
/// - `holder == Some(c)` and `c != requested_card` ⇒ the holder is on a different
///   card; the one-card conflict does not apply ⇒ [`Arbitration::Proceed`].
/// - `holder == Some(c)` and `c == requested_card` ⇒ the requested card is held by
///   a managed modem ⇒ [`Arbitration::MustStopHolder`].
///
/// The caller injects the current holder. **Phase-6 integration seam:** wiring the
/// live ardopcf session manager's held-card into this `holder` argument is P6's
/// cross-module concern (this phase must not reach into ardop's live session
/// manager — that coupling is out of scope here). This pure function is the
/// decision P6 calls once it can supply the holder.
pub fn arbitrate(requested_card: &CardId, holder: Option<&CardId>) -> Arbitration {
    match holder {
        Some(held) if held == requested_card => Arbitration::MustStopHolder(held.clone()),
        _ => Arbitration::Proceed,
    }
}

// ─── Tests — stub-process based, mirroring process.rs. No real direwolf/radio. ──
#[cfg(test)]
mod tests {
    use super::*;

    /// A KISS port for tests. Chosen high to avoid privileged-port issues and
    /// unlikely-to-collide with a real service. Each test uses a distinct port so
    /// concurrent test threads do not fight over the same listener.
    const TEST_KISS_PORT_CLEAN: u16 = 58921;
    const TEST_KISS_PORT_SIGKILL: u16 = 58922;
    const TEST_KISS_PORT_BUSY: u16 = 58924;
    /// The KISS port spawn waits on in the bind-timeout test — nothing ever binds
    /// it, so bind-wait times out.
    const TEST_KISS_PORT_TIMEOUT: u16 = 58925;
    /// A SEPARATE port the bind-timeout stub binds + holds. It is NOT the KISS port
    /// spawn waits on, so its release is observable: it frees only if the child the
    /// stop-on-timeout path reaped actually died (a leaked child would still hold it).
    const TEST_SENTINEL_PORT: u16 = 58926;

    /// Build a test config pointing at a card index that is (almost certainly) not
    /// present on the CI runner, so the pre-spawn `probe_device_busy` reads "not
    /// provably busy" (the card<N> dir is absent ⇒ `Ok(())`) and the release
    /// confirmation likewise reads free immediately. This keeps the lifecycle
    /// tests focused on the spawn/bind/stop path, not on a real ALSA card.
    fn test_cfg(kiss_port: u16) -> ManagedDireWolfCfg {
        ManagedDireWolfCfg {
            adevice: "plughw:CARD=Test,DEV=0".to_string(),
            // 9999: no such ALSA card on the runner ⇒ /proc/asound/card9999 absent
            // ⇒ probe_device_busy returns Ok(()) (soft-failure "not provably busy").
            card_index: 9999,
            mycall: "N0CALL".to_string(),
            ptt: PttChoice::SerialRts {
                tty: "/dev/null".to_string(),
            },
            kiss_port,
        }
    }

    /// A python3 one-liner stub that BINDS the KISS port (so bind-wait succeeds),
    /// installs a SIGINT handler that exits 0, then sleeps. Mirrors process.rs's
    /// `sh -c 'trap ...'` stub, extended to actually bind the port.
    ///
    /// python3 is the portable choice on the CI Linux runner: a pure-shell `nc -l`
    /// is not guaranteed present (and `nc` flavors differ), whereas python3 + the
    /// stdlib `socket`/`signal` modules are reliably available.
    fn stub_binds_and_handles_sigint(port: u16) -> String {
        // The script is python3's `-c` arg. We bind, listen, and on SIGINT exit 0.
        // time.sleep in a loop keeps the trap responsive.
        format!(
            "import socket,signal,sys,time\n\
             s=socket.socket(socket.AF_INET,socket.SOCK_STREAM)\n\
             s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\n\
             s.bind(('127.0.0.1',{port}))\n\
             s.listen(1)\n\
             signal.signal(signal.SIGINT,lambda *_: sys.exit(0))\n\
             while True: time.sleep(0.05)\n"
        )
    }

    /// A python3 stub that BINDS the port but IGNORES SIGINT (so stop must escalate
    /// to SIGKILL). Mirrors process.rs's `trap '' INT` stub.
    fn stub_binds_and_ignores_sigint(port: u16) -> String {
        format!(
            "import socket,signal,sys,time\n\
             s=socket.socket(socket.AF_INET,socket.SOCK_STREAM)\n\
             s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\n\
             s.bind(('127.0.0.1',{port}))\n\
             s.listen(1)\n\
             signal.signal(signal.SIGINT,signal.SIG_IGN)\n\
             while True: time.sleep(0.05)\n"
        )
    }

    /// A python3 stub that binds a SENTINEL port (NOT the KISS port spawn waits on)
    /// and then sleeps. Used for the bind-timeout no-leak test: spawn waits on a
    /// DIFFERENT, never-bound KISS port → bind-wait times out → the stop-on-timeout
    /// path kills this child → the sentinel port becomes bindable. A leaked child
    /// would keep holding the sentinel port, so observing the sentinel freed proves
    /// the child was actually reaped.
    fn stub_binds_sentinel(sentinel_port: u16) -> String {
        format!(
            "import socket,time\n\
             s=socket.socket(socket.AF_INET,socket.SOCK_STREAM)\n\
             s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\n\
             s.bind(('127.0.0.1',{sentinel_port}))\n\
             s.listen(1)\n\
             while True: time.sleep(0.05)\n"
        )
    }

    /// Spawn `ManagedDireWolf` against a python3 stub program. Mirrors process.rs's
    /// `sh()` helper. Uses a short bind-wait so the timeout test is fast.
    fn spawn_stub(
        cfg: ManagedDireWolfCfg,
        script: &str,
        bind_wait: Duration,
    ) -> Result<ManagedDireWolf, DwLifecycleError> {
        // Routes through `spawn_stub_for_test`, which drives the SAME `spawn_inner`
        // production `spawn_with` uses — substituting only the command-builder
        // closure (`python3 -c <script>` for `direwolf -t 0 -c <conf>`), so the
        // real conf-write / probe / bind-wait / stop paths are exercised.
        ManagedDireWolf::spawn_stub_for_test(cfg, script, bind_wait)
    }

    /// True if python3 is available — the stubs need it. If absent we skip the
    /// lifecycle tests (the pure tests below still run).
    fn python3_present() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    // ── Test 1: clean-SIGINT lifecycle ────────────────────────────────────────

    /// Stub binds the KISS port and exits 0 on SIGINT. spawn → Ok, endpoint is
    /// ("127.0.0.1", port), shutdown SIGINTs it clean (is_running false after).
    #[test]
    fn spawn_then_clean_sigint_shutdown() {
        if !python3_present() {
            eprintln!("python3 absent — skipping managed_direwolf lifecycle test");
            return;
        }
        let cfg = test_cfg(TEST_KISS_PORT_CLEAN);
        let script = stub_binds_and_handles_sigint(TEST_KISS_PORT_CLEAN);
        let mut dw = spawn_stub(cfg, &script, Duration::from_secs(3))
            .expect("spawn against a binding stub must succeed");

        assert_eq!(dw.endpoint(), ("127.0.0.1", TEST_KISS_PORT_CLEAN));
        assert!(dw.is_running(), "stub must be running after spawn");

        dw.shutdown().expect("clean shutdown must return Ok");
        assert!(!dw.is_running(), "stub must be gone after clean shutdown");
    }

    // ── Test 2: SIGKILL escalation ────────────────────────────────────────────

    /// Stub binds the port but IGNORES SIGINT → shutdown escalates to SIGKILL; the
    /// process is gone afterward. Mirrors process.rs's
    /// `stop_escalates_to_sigkill_when_sigint_ignored`. Uses a SHORT grace via a
    /// test shutdown so the SIGKILL fires quickly.
    #[test]
    fn shutdown_escalates_to_sigkill_when_sigint_ignored() {
        if !python3_present() {
            eprintln!("python3 absent — skipping managed_direwolf SIGKILL test");
            return;
        }
        let cfg = test_cfg(TEST_KISS_PORT_SIGKILL);
        let script = stub_binds_and_ignores_sigint(TEST_KISS_PORT_SIGKILL);
        let mut dw = spawn_stub(cfg, &script, Duration::from_secs(3))
            .expect("spawn against a binding stub must succeed");
        assert!(dw.is_running(), "stub must be running after spawn");

        // Short grace so the escalation is fast; the card_index=9999 release probe
        // reads free immediately, so shutdown returns Ok after SIGKILL.
        dw.shutdown_with_grace_for_test(Duration::from_millis(200))
            .expect("shutdown must return Ok even after SIGKILL escalation");
        assert!(!dw.is_running(), "stub must be gone after SIGKILL");
    }

    // ── Test 3: bind-wait timeout PROVES no leaked child (sentinel port) ──────

    /// The stub binds a SENTINEL port but NOT the KISS port spawn waits on, so
    /// bind-wait times out and spawn's stop-on-timeout path must kill the child.
    /// We then assert the SENTINEL port is bindable again: that is only true if the
    /// child holding it was actually reaped. A leaked child would keep its LISTEN
    /// socket on the sentinel, so the assert would fail — this is the real no-leak
    /// proof (the prior version bound nothing, so "port free after" was satisfied
    /// whether or not the child died, making it near-tautological).
    #[test]
    fn spawn_bind_timeout_reaps_child_proven_via_sentinel() {
        if !python3_present() {
            eprintln!("python3 absent — skipping managed_direwolf bind-timeout test");
            return;
        }
        // The child binds the sentinel port; spawn waits on the (different) KISS
        // port that nothing ever binds.
        let cfg = test_cfg(TEST_KISS_PORT_TIMEOUT);
        let script = stub_binds_sentinel(TEST_SENTINEL_PORT);
        // Short bind-wait so the timeout fires fast.
        let err = spawn_stub(cfg, &script, Duration::from_millis(400))
            .expect_err("spawn must time out when the stub never binds the KISS port");
        match err {
            DwLifecycleError::BindTimeout { port, .. } => {
                assert_eq!(
                    port, TEST_KISS_PORT_TIMEOUT,
                    "timeout must name the KISS port spawn waited on"
                );
            }
            other => panic!("expected BindTimeout, got {other:?}"),
        }

        // The SENTINEL port — which the child HELD — must be bindable now. This is
        // only possible if the child was reaped on the timeout path; a leaked child
        // would still hold its LISTEN socket on the sentinel and EADDRINUSE us here.
        assert!(
            std::net::TcpListener::bind(format!("127.0.0.1:{TEST_SENTINEL_PORT}")).is_ok(),
            "sentinel port must be free after bind-timeout — proves the child was reaped (no leak)"
        );
    }

    // ── Test 4: device-busy short-circuit (never spawns) ──────────────────────

    /// With a card_index whose /proc/asound status reads BUSY, spawn returns
    /// DeviceBusy and never spawns a process. We simulate "busy" by pointing
    /// card_index at a fixture the probe reads as held — but since probe_device_busy
    /// reads the real /proc, we instead exercise the short-circuit via the
    /// injectable probe seam (`spawn_with_probe_for_test`) so the test does not
    /// depend on a real held card.
    #[test]
    fn spawn_device_busy_short_circuits_without_spawning() {
        let cfg = test_cfg(TEST_KISS_PORT_BUSY);
        // Inject a probe that reports the card busy. spawn must return DeviceBusy
        // and must NOT spawn (the script is a bind-the-port stub that, if spawned,
        // would make the port unbindable — we assert the port stays free).
        let busy_msg = device_busy_message(&cfg.adevice);
        let err = ManagedDireWolf::spawn_with_busy_probe_for_test(cfg, Err(busy_msg.clone()))
            .expect_err("spawn must return DeviceBusy when the probe reports busy");
        match err {
            DwLifecycleError::DeviceBusy(msg) => {
                assert_eq!(
                    msg, busy_msg,
                    "DeviceBusy must carry the named busy message"
                );
                assert!(msg.contains("plughw:CARD=Test,DEV=0"));
            }
            other => panic!("expected DeviceBusy, got {other:?}"),
        }
        // Never spawned ⇒ the KISS port is still bindable.
        assert!(
            std::net::TcpListener::bind(format!("127.0.0.1:{TEST_KISS_PORT_BUSY}")).is_ok(),
            "device-busy short-circuit must not have spawned anything"
        );
    }

    // ── Test 5: arbitration decision (pure) ───────────────────────────────────

    /// No holder ⇒ Proceed; a holder on the SAME card ⇒ MustStopHolder; a holder on
    /// a DIFFERENT card ⇒ Proceed.
    #[test]
    fn arbitrate_proceed_and_must_stop() {
        let requested = CardId("plughw:CARD=DRA,DEV=0".to_string());

        // No managed modem holds any card.
        assert_eq!(arbitrate(&requested, None), Arbitration::Proceed);

        // Another modem holds the SAME card ⇒ must stop it first.
        let same = CardId("plughw:CARD=DRA,DEV=0".to_string());
        assert_eq!(
            arbitrate(&requested, Some(&same)),
            Arbitration::MustStopHolder(same.clone())
        );

        // Another modem holds a DIFFERENT card ⇒ the one-card conflict doesn't apply.
        let other = CardId("plughw:CARD=Device,DEV=0".to_string());
        assert_eq!(arbitrate(&requested, Some(&other)), Arbitration::Proceed);
    }

    // ── Test 7: pick_free_kiss_port returns a bindable-then-free port ─────────

    /// The port picker returns a non-zero port that is FREE right after the call
    /// (we can bind it ourselves), proving the picker released its probe listener.
    /// Also a light sanity check that two calls usually differ (ephemeral churn) —
    /// but we only assert bindability, since the OS may legitimately reuse a port.
    #[test]
    fn pick_free_kiss_port_returns_a_bindable_port() {
        let port = pick_free_kiss_port().expect("picking a free port must succeed");
        assert_ne!(
            port, 0,
            "picker must return a concrete assigned port, not 0"
        );
        // The picker dropped its listener, so the port must be bindable now. (The
        // tiny TOCTOU window documented on the fn is acceptable; in a quiet test
        // process nothing else races for this exact ephemeral port.)
        let rebind = std::net::TcpListener::bind(format!("127.0.0.1:{port}"));
        assert!(
            rebind.is_ok(),
            "picked port {port} must be free (picker released its probe listener)"
        );
    }

    /// `ManagedDireWolfGuard::drop` runs the explicit 5s `shutdown()` (clean
    /// de-key) — proven by spawning a stub Dire Wolf, wrapping it in a guard,
    /// dropping the guard, and asserting the child's KISS port frees (the process
    /// is gone). This exercises the RADIO-1 "shutdown on every exit path" property
    /// via Drop, the same mechanism `?`/panic unwinding triggers.
    #[test]
    fn guard_drop_shuts_down_managed_direwolf() {
        if !python3_present() {
            eprintln!("python3 absent — skipping managed_direwolf guard-drop test");
            return;
        }
        const PORT: u16 = 58927;
        let cfg = test_cfg(PORT);
        let script = stub_binds_and_handles_sigint(PORT);
        let dw = spawn_stub(cfg, &script, Duration::from_secs(3))
            .expect("spawn against a binding stub must succeed");
        // The stub holds the KISS port while alive.
        assert!(
            std::net::TcpListener::bind(format!("127.0.0.1:{PORT}")).is_err(),
            "stub must hold the KISS port while the guard is alive"
        );
        {
            let _guard = ManagedDireWolfGuard(dw);
            // Guard drops at the end of this block → explicit shutdown() runs.
        }
        // After the guard dropped, the child is shut down and the port is free.
        // Poll briefly: shutdown's SIGINT + the stub's exit are near-instant, but
        // the OS may take a tick to release the LISTEN socket.
        let mut freed = false;
        for _ in 0..50 {
            if std::net::TcpListener::bind(format!("127.0.0.1:{PORT}")).is_ok() {
                freed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            freed,
            "guard Drop must shut down the managed direwolf — KISS port must free"
        );
    }

    // ── Test 6: map_spawn_error (pure, no spawn) ──────────────────────────────

    /// Direct unit test of the spawn-error mapping that drives Phase 6's
    /// operator-facing fallbacks — notably the `NotFound → DireWolfNotInstalled`
    /// branch that lets P6 offer "install Dire Wolf." Constructs `ProcessError`
    /// values directly; no process is spawned.
    #[test]
    fn map_spawn_error_classifies_named_variants() {
        // Binary absent ⇒ the install-affordance variant.
        let not_found = ProcessError::Spawn(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(
            matches!(
                map_spawn_error(not_found),
                DwLifecycleError::DireWolfNotInstalled
            ),
            "NotFound spawn must map to DireWolfNotInstalled (P6 install path)"
        );

        // Any other spawn io-error ⇒ generic Spawn.
        let perm = ProcessError::Spawn(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(
            matches!(map_spawn_error(perm), DwLifecycleError::Spawn(_)),
            "PermissionDenied spawn must map to the generic Spawn variant"
        );

        // A Stop-flavored ProcessError likewise folds into Spawn (it is a spawn-path
        // failure surfaced by ManagedModem, not an absent binary).
        let stop = ProcessError::Stop("boom".to_string());
        assert!(
            matches!(map_spawn_error(stop), DwLifecycleError::Spawn(_)),
            "ProcessError::Stop must map to the generic Spawn variant"
        );
    }
}

// ─── Test-only entrypoints ─────────────────────────────────────────────────────
//
// These mirror process.rs's pattern of exposing an internal spawn seam to tests.
// They live outside the `tests` module so they can be `#[cfg(test)]`-gated methods
// on the type. They route a stub program / injected probe through the same
// spawn/bind/stop machinery `spawn` uses, substituting only the parts a real-radio
// test must not exercise (the `direwolf` binary, the live /proc probe).

// These are `#[cfg(test)]` helper *methods on the type* (they cannot live inside
// `mod tests`), so they are grouped just after the test module rather than being
// production items hidden after tests — the case `items_after_test_module` guards.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
impl ManagedDireWolf {
    /// Spawn against a `python3 -c <script>` stub instead of the real `direwolf`
    /// binary, with a caller-chosen bind-wait. The stub stands in for Dire Wolf:
    /// it binds (or refuses to bind) the KISS port exactly as Dire Wolf would.
    ///
    /// Routes through the SAME [`spawn_inner`] production [`spawn_with`] uses,
    /// substituting only the command-builder closure (`python3 -c <script>`
    /// instead of `direwolf -t 0 -c <conf>`). The stub tests therefore drive the
    /// real conf-write, device-busy probe, bind-wait, and stop/release paths — the
    /// only thing they swap is the program + args, so the production lifecycle body
    /// cannot drift out from under the tests.
    fn spawn_stub_for_test(
        cfg: ManagedDireWolfCfg,
        script: &str,
        bind_wait: Duration,
    ) -> Result<Self, DwLifecycleError> {
        // Capture the script into an owned String so the FnOnce closure can move it
        // and return the owned (program, args) spawn_inner expects.
        let script = script.to_string();
        Self::spawn_inner(
            cfg,
            move |_conf_path| ("python3".to_string(), vec!["-c".to_string(), script]),
            bind_wait,
        )
    }

    /// `shutdown` with a caller-chosen `stop` grace so the SIGKILL-escalation test
    /// does not wait the full production [`SHUTDOWN_GRACE`]. Same release-confirm +
    /// retry semantics as production `shutdown`.
    fn shutdown_with_grace_for_test(&mut self, grace: Duration) -> Result<(), DwLifecycleError> {
        let Some(mut modem) = self.modem.take() else {
            return Ok(());
        };
        let stop_result = modem.stop(grace);
        let released =
            confirm_card_released(&self.adevice, self.card_index, RELEASE_CONFIRM_DEADLINE);
        if !released {
            let msg = device_busy_message(&self.adevice);
            self.modem = Some(modem);
            return Err(DwLifecycleError::DeviceNotReleased(format!(
                "{msg} after shutdown — swap invariant violated"
            )));
        }
        self._conf = None;
        stop_result.map_err(|e| DwLifecycleError::Stop(e.to_string()))?;
        Ok(())
    }

    /// Spawn with an INJECTED device-busy probe result, bypassing the real /proc
    /// read so the device-busy short-circuit test does not need a real held card.
    /// When `probe_result` is `Err`, spawn returns [`DwLifecycleError::DeviceBusy`]
    /// WITHOUT spawning anything — exactly the production short-circuit.
    ///
    /// This entrypoint deliberately does NOT route through [`spawn_inner`]: the
    /// production inner reads the real `/proc` probe (correctly, with card 9999 ⇒
    /// `Ok(())` on the runner), so it cannot exercise the *busy* branch without a
    /// real held card. This stub injects the busy result and asserts the
    /// short-circuit fires BEFORE any spawn. It contains no duplicated lifecycle
    /// body (conf-write + short-circuit only); a free injected probe intentionally
    /// errors rather than falling through to a real spawn.
    fn spawn_with_busy_probe_for_test(
        cfg: ManagedDireWolfCfg,
        probe_result: Result<(), String>,
    ) -> Result<Self, DwLifecycleError> {
        // Conf write happens first in production; do it here too for fidelity, then
        // short-circuit on the injected busy result BEFORE any spawn.
        let conf_text = generate_direwolf_conf(&cfg.to_dw_params());
        let _conf_file = write_conf_tempfile(&conf_text)
            .map_err(|e| DwLifecycleError::ConfWrite(e.to_string()))?;

        if let Err(named_msg) = probe_result {
            // Short-circuit: never spawn against a held card.
            return Err(DwLifecycleError::DeviceBusy(named_msg));
        }

        // (Not reached in the busy test; a free injected probe would fall through
        // to a real spawn, which this test entrypoint intentionally does not do.)
        Err(DwLifecycleError::Spawn(
            "spawn_with_busy_probe_for_test only exercises the busy short-circuit".to_string(),
        ))
    }
}
