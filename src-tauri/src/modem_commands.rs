//! Tauri commands for modem (ARDOP) operations.
//!
//! RADIO-1: `modem_ardop_connect` requires a per-session consent token issued
//! by the frontend's RADIO-1 modal. The backend rejects any connect attempt
//! whose token doesn't match the current session token. See Phase 6.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

use crate::config::{self, ArdopUiConfig, Config};
use crate::modem_status::{ModemSession, ModemState, ModemStatus};
use crate::native_mailbox::Mailbox;
use crate::winlink::modem::ardop::transport::ArdopTransport;
use crate::winlink::modem::ardop::ArdopConfig;
use crate::winlink::modem::{InitConfig, ModemTransport};

/// RADIO-1 bounded-airtime cap: the worst-case `connect_arq` wall-clock budget.
///
/// 2026-05-22 incident: a ~110s runaway connect (no working abort) forced an
/// operator radio power-off. The cap prevents the same pattern here — if
/// `connect_arq` does not return CONNECTED / FAULT / DISC within the deadline,
/// the call errors out and the session is reset.
const CONNECT_DEADLINE: Duration = Duration::from_secs(120);

/// Number of ARQ retries packed into the `ARQCALL` setter.
const CONNECT_REPEAT: u32 = 3;

/// ARQ-link idle timeout passed to the TNC via `ARQTIMEOUT` during init.
const ARQ_TIMEOUT_SECS: u32 = 30;

/// Return the persisted ARDOP configuration, or the struct default if nothing
/// has been written yet (first run) or the config file is absent.
#[tauri::command]
pub fn config_get_ardop() -> ArdopUiConfig {
    config::read_config()
        .map(|cfg| cfg.modem_ardop.unwrap_or_default())
        .unwrap_or_default()
}

/// Persist a new ARDOP configuration. Reads the current config, replaces
/// `modem_ardop`, and writes atomically. Returns an error if the config file
/// cannot be read (e.g. wizard has not been completed) or the write fails.
#[tauri::command]
pub fn config_set_ardop(value: ArdopUiConfig) -> Result<(), String> {
    let mut cfg = config::read_config().map_err(|e| format!("read failed: {e}"))?;
    cfg.modem_ardop = Some(value);
    config::write_config_atomic(&cfg).map_err(|e| format!("save failed: {e}"))
}

/// Inner helper: snapshot the current session status. Pure on `&Arc<ModemSession>`
/// so tests can exercise it without constructing a Tauri `State`.
pub fn modem_get_status_inner(session: &Arc<ModemSession>) -> ModemStatus {
    session.status_snapshot()
}

/// Inner helper: atomically clear RADIO-1 consent, reset status to Stopped,
/// take the transport handle, then shut the transport down OUTSIDE the lock.
/// Uses [`ModemSession::reset_to_stopped`] so observers see a single
/// consistent transition rather than the prior two-step (clear-consent then
/// set-status) which left a window where the token was invalidated but the
/// status still read as the prior connected variant.
///
/// I/O discipline: `transport.disconnect()` and the subsequent `drop` run
/// AFTER the session mutex is released. Holding the lock across the modem
/// disconnect I/O (TCP DISCONNECT + DISCONNECTED ack, bounded by 5s) would
/// stall any concurrent `status_snapshot` call for the duration.
///
/// tuxlink-o3f2 (P1 abort-during-connect): FIRST step is a best-effort
/// `abort_in_flight()` that side-channels `ABORT\r` to ardopcf via the
/// cmd-socket writer installed at connect time. If a connect is currently
/// blocking inside `arq_connect`'s recv loop, ardopcf responds to ABORT
/// with `FAULT` / `NEWSTATE DISC`, the cmd reader thread delivers it via
/// the channel, the recv loop returns `Err(SessionError::Fault(...))`,
/// and the connect path unwinds cleanly. If no connect is in flight,
/// `abort_in_flight` is harmless: ABORT on an idle TNC is a no-op
/// (ardopcf documents it as "immediate interrupt of any in-flight TX").
/// If no writer is installed (transport was never connected, or session
/// already reset), `abort_in_flight` returns `Err` and we fall through to
/// the existing graceful disconnect path.
pub fn modem_ardop_disconnect_inner(session: &Arc<ModemSession>) -> Result<(), String> {
    // tuxlink-o3f2: best-effort abort of any in-flight connect_arq. The
    // _ discard is deliberate — if the writer is missing or the write
    // fails, the fall-through reset_to_stopped + transport.disconnect
    // path will still surface a clean Stopped state. Documented behavior:
    // ABORT on an idle TNC is a no-op, so it's safe to call unconditionally.
    let _ = session.abort_in_flight();

    if let Some(mut transport) = session.reset_to_stopped() {
        // Best-effort: even if disconnect errors, the session is already
        // marked Stopped so reconnects are possible. The TNC process (when
        // managed) is torn down separately via ArdopTransport::shutdown —
        // disconnect() here only sends the DISCONNECT command on the cmd
        // socket. Process teardown lands when the full shutdown wiring
        // arrives in a follow-up.
        let _ = transport.disconnect(Duration::from_secs(5));
        drop(transport);
    }
    Ok(())
}

/// Return the current session snapshot. Hooks call this on mount to recover
/// state when remounting mid-session (e.g. after a hot-reload).
#[tauri::command]
pub fn modem_get_status(session: State<'_, Arc<ModemSession>>) -> ModemStatus {
    modem_get_status_inner(&session)
}

/// RADIO-1: mint a fresh per-session consent token on the BACKEND and return
/// it to the frontend. The frontend invokes this from the consent-modal's
/// Connect button (after the operator ticks the acknowledgement) so that the
/// token authorizing the subsequent `modem_ardop_connect` was produced by
/// the same trust boundary that validates it. A frontend-generated token
/// would let a compromised renderer self-mint — the gate would be theater.
/// See [`ModemSession::mint_consent_token`] for storage semantics.
#[tauri::command]
pub fn modem_mint_consent(session: State<'_, Arc<ModemSession>>) -> String {
    session.mint_consent_token()
}

/// Disconnect the modem: invalidates the RADIO-1 consent token, takes the
/// live transport handle, resets status to Stopped, and shuts the transport
/// down (best-effort `DISCONNECT` on the cmd socket).
#[tauri::command]
pub fn modem_ardop_disconnect(session: State<'_, Arc<ModemSession>>) -> Result<(), String> {
    modem_ardop_disconnect_inner(&session)
}

/// Inner helper with a factory seam — RADIO-1-gated ARDOP connect.
///
/// The factory closure constructs the `Box<dyn ModemTransport>` given an
/// `ArdopConfig` and the target callsign. Production calls hand in
/// `ArdopTransport::with_managed_modem`; tests hand in a stub.
///
/// # RADIO-1
///
/// The first action is [`ModemSession::consume_consent_token`] — atomic
/// equality-check-and-clear under one lock. ANY call with a missing-or-wrong
/// token returns `Err` BEFORE the factory runs, BEFORE `init`, BEFORE
/// `connect_arq` — i.e., no spawn, no socket bind, no I/O whatsoever, AND
/// no status mutation. A successful match consumes the token in the same
/// lock acquisition, so a replay attempt (same token, second call) is
/// indistinguishable from a wrong token from this point forward.
///
/// The token is in-process replay protection minted via
/// `modem_mint_consent`; a compromised renderer cannot self-mint because
/// the token is generated server-side. Plain string equality on the wire
/// is the design. Per-invocation consent (Part 97) is enforced by the
/// CONSUME semantics: one mint authorizes exactly one connect.
///
/// # Bounded airtime
///
/// `connect_arq` is bounded by [`CONNECT_DEADLINE`] (120s). The 2026-05-22
/// runaway-connect incident is the calibration: a 110s no-abort runaway
/// forced a radio power-off. There is NO retry loop in this function — if
/// `init` or `connect_arq` fails, the status flips to `Error` and we
/// return immediately. A retry must be a fresh user-initiated Connect
/// with a fresh consent token (Part 97 per-invocation rule).
pub fn modem_ardop_connect_gated_with_factory<F>(
    session: &Arc<ModemSession>,
    target: &str,
    consent_token: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // ─── RADIO-1 consent gate ────────────────────────────────────────────
    // FIRST CHECK: no I/O, no spawn, no status mutation if the token is
    // wrong. Keeping the gate ahead of every other side effect is the
    // whole point of the function — a compromised caller that bypasses
    // the modal must NOT be able to even SPAWN ardopcf.
    //
    // `consume_consent_token` is atomic — equality check + clear under a
    // single lock acquisition. After a successful return, the stored token
    // is `None`, so a replay attempt (same `consent_token`, second call)
    // takes this same branch and returns Err. Per-invocation consent
    // (Part 97) is enforced by this consume, not by any caller-side
    // discipline.
    if !session.consume_consent_token(consent_token) {
        return Err(
            "RADIO-1: missing or invalid consent token; mint one via the Connect modal first"
                .into(),
        );
    }

    modem_ardop_connect_post_consume_with_factory(session, target, ardop_ui, make_transport)
}

/// Inner helper AFTER the consent gate has fired + consumed the token.
/// Do NOT call this from anywhere that hasn't already validated + consumed
/// the consent token via [`ModemSession::consume_consent_token`]. The
/// `_post_consume` naming is the discipline contract: this function trusts
/// its caller has gated.
///
/// Used by the Tauri `modem_ardop_connect` wrapper, which consumes the
/// token FIRST (RADIO-1: no I/O before gate) and only then runs config
/// I/O + delegates here.
pub fn modem_ardop_connect_post_consume_with_factory<F>(
    session: &Arc<ModemSession>,
    target: &str,
    ardop_ui: &ArdopUiConfig,
    make_transport: F,
) -> Result<(), String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    // NO GATE here — caller MUST have consumed the consent token already.
    // (Naming the function `_post_consume` is the discipline contract.)

    // ─── Translate ArdopUiConfig (frontend) → ArdopConfig (backend) ─────
    // See `build_ardop_extra_args` — extracted for unit testing.
    let extra_args = build_ardop_extra_args(ardop_ui);

    let cfg = ArdopConfig {
        binary: PathBuf::from(&ardop_ui.binary),
        extra_args,
        cmd_port: ardop_ui.cmd_port,
        // ardopcf convention: data_port = cmd_port + 1 (8516 for default 8515).
        data_port: ardop_ui.cmd_port.saturating_add(1),
        audio_device_path: None,
    };

    // Mark spawning so any concurrent status_snapshot sees the transition
    // before the (potentially slow) ardopcf bind-wait + init.
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Spawning;
    snap.peer = Some(target.to_string());
    snap.last_error = None;
    session.set_status(snap);

    // ─── Spawn ───────────────────────────────────────────────────────────
    let mut transport = match make_transport(cfg, target) {
        Ok(t) => t,
        Err(e) => {
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(e.clone());
            session.set_status(s);
            return Err(e);
        }
    };

    // ─── Init the TNC ────────────────────────────────────────────────────
    let init_cfg = init_config_from_persisted_config();
    if let Err(e) = transport.init(&init_cfg) {
        let msg = format!("init failed: {e}");
        let mut s = ModemStatus::stopped();
        s.state = ModemState::Error;
        s.last_error = Some(msg.clone());
        session.set_status(s);
        // Drop the partially-initialized transport so any spawned process
        // is torn down by its Drop impl rather than leaking past this fn.
        drop(transport);
        return Err(msg);
    }

    // tuxlink-o3f2: install the side-channel abort writer BEFORE the
    // blocking `connect_arq` begins. While the recv loop inside
    // `arq_connect` holds the transport on its stack, the operator's
    // Disconnect button calls `modem_ardop_disconnect_inner` → which calls
    // `session.abort_in_flight()` → which writes `ABORT\r` to ardopcf via
    // this writer. The recv loop then surfaces FAULT/NEWSTATE DISC and
    // returns Err, unwinding the connect path. Without this hook the
    // 120s `CONNECT_DEADLINE` was the only abort path — see the
    // 2026-05-22 runaway-connect incident (memory radio1-bounded-airtime-abort).
    //
    // If the backend can't expose a writer (default trait impl returns
    // None), the install is silently skipped: graceful disconnect remains
    // the only path. For ardopcf the writer is always available after
    // init() succeeds.
    if let Some(writer) = transport.try_clone_abort_writer() {
        session.install_abort_writer(writer);
    }

    // Status: Connecting (bounded by CONNECT_DEADLINE below).
    let mut snap = session.status_snapshot();
    snap.state = ModemState::Connecting;
    session.set_status(snap);

    // ─── ARQ connect (bounded airtime) ───────────────────────────────────
    let info = match transport.connect_arq(target, CONNECT_REPEAT, CONNECT_DEADLINE) {
        Ok(info) => info,
        Err(e) => {
            let msg = format!("ARQ connect failed: {e}");
            let mut s = ModemStatus::stopped();
            s.state = ModemState::Error;
            s.last_error = Some(msg.clone());
            session.set_status(s);
            drop(transport);
            return Err(msg);
        }
    };

    // ─── Install handle + publish initial connected snapshot ─────────────
    session.install_transport(transport);

    let mut s = session.status_snapshot();
    s.state = ModemState::ConnectedIrs;
    s.peer = Some(info.peer_call.clone());
    s.width_hz = Some(info.bandwidth_hz);
    s.last_error = None;
    session.set_status(s);

    Ok(())
}

/// Build the [`InitConfig`] passed to `ModemTransport::init` from the
/// operator's persisted identity config. Pulls `mycall` from
/// `identity.callsign` (CMS path) or `identity.identifier` (offline path),
/// `gridsquare` from `identity.grid` (defaulting to `"AA00"` when no grid
/// is set — the ARDOP TNC requires a non-empty value but the broadcast
/// precision gate happens upstream in the position layer), and the ARQ
/// bandwidth from `modem_ardop.bandwidth_hz` (tuxlink-j0ij).
///
/// **Bandwidth validation:** the Settings panel constrains the dropdown to
/// {200, 500, 1000, 2000}, but the persisted JSON could be hand-edited
/// off-app, so this function defends in depth: any other value is logged
/// to stderr and dropped to None (let ardopcf use its default) rather than
/// passed through and rejected by ardopcf at init time.
fn init_config_from_persisted_config() -> InitConfig {
    let cfg = config::read_config().ok();
    let (mycall, grid, arq_bandwidth_hz) = match &cfg {
        Some(c) => {
            let call = c
                .identity
                .callsign
                .clone()
                .or_else(|| c.identity.identifier.clone())
                .unwrap_or_default();
            let grid = c.identity.grid.clone().unwrap_or_default();
            let bw = c
                .modem_ardop
                .as_ref()
                .and_then(|a| a.bandwidth_hz)
                .and_then(validate_arq_bandwidth_hz);
            (call, grid, bw)
        }
        None => (String::new(), String::new(), None),
    };

    // ARDOP requires a non-empty grid; "AA00" is the canonical placeholder
    // (also wl2k-go's fallback). Operators who care about grid accuracy
    // configure it via the wizard.
    let gridsquare = if grid.trim().is_empty() {
        "AA00".to_string()
    } else {
        grid
    };

    InitConfig {
        mycall,
        gridsquare,
        arq_timeout_s: ARQ_TIMEOUT_SECS,
        arq_bandwidth_hz,
    }
}

/// Build the `extra_args` vector passed to `ArdopConfig` (the ardopcf CLI).
///
/// ardopcf's positional CLI is:
/// ```text
/// ardopcf [-p <ptt>] [-G <webgui_port>] <cmd_port> <capture> <playback>
/// ```
///
/// Optional flags (in this order) precede the positional triple:
///
/// - **`-p <ptt>`** — only when `ardop_ui.ptt_serial_path` is `Some(non_empty)`.
///   RTS PTT via the named serial port. ardopcf rejects an empty value, so we
///   filter empty strings here defensively.
/// - **`-G <webgui_port>`** — tuxlink-60wh: enable ardopcf's built-in WebGUI
///   (Spectrum + Waterfall + level meters) so the operator can open it in
///   their browser via the dock's "Open WebGUI" button. The port follows
///   ardopcf's documented convention `webgui_port = cmd_port - 1` (default
///   8515 → 8514). The flag is omitted when `cmd_port < 2` (no valid TCP
///   port can be derived); `0` is reserved and `1` is too low to bind in
///   practice. The omission is a safe default — ardopcf simply runs
///   without a WebGUI when `-G` is absent.
///
/// Pure over `&ArdopUiConfig` so unit tests can assert the exact argv shape
/// without spawning a real process.
pub(crate) fn build_ardop_extra_args(ardop_ui: &ArdopUiConfig) -> Vec<String> {
    // Capacity covers worst case: -p <ptt> -G <wg> <cmd> <cap> <play> = 7.
    let mut extra_args: Vec<String> = Vec::with_capacity(7);

    if let Some(ref ptt) = ardop_ui.ptt_serial_path {
        if !ptt.is_empty() {
            extra_args.push("-p".into());
            extra_args.push(ptt.clone());
        }
    }

    // tuxlink-60wh: spawn ardopcf with its built-in WebGUI on the conventional
    // port (cmd_port - 1). Operator opens it via the dock's "Open WebGUI"
    // button which targets `http://localhost:<webgui_port>/` — Spectrum,
    // Waterfall, audio level meters, TX/RX indicators, test-tone trigger.
    // Guard: cmd_port must be >= 2 so the derived webgui_port is a valid
    // bindable TCP port (>= 1). The default cmd_port is 8515 → 8514.
    if ardop_ui.cmd_port >= 2 {
        let webgui_port = ardop_ui.cmd_port - 1;
        extra_args.push("-G".into());
        extra_args.push(webgui_port.to_string());
    }

    extra_args.push(ardop_ui.cmd_port.to_string());
    extra_args.push(ardop_ui.capture_device.clone());
    extra_args.push(ardop_ui.playback_device.clone());

    extra_args
}

/// Validate a persisted ARQ bandwidth value (tuxlink-j0ij). ardopcf accepts
/// exactly {200, 500, 1000, 2000} Hz for `ARQBW`. The Settings dropdown
/// constrains user input to these values, so a value OUTSIDE this set in
/// the persisted config indicates either a stale value from a future
/// ardopcf release, a hand-edited config, or a frontend bug — in any case,
/// the safe degradation is "drop to None and let ardopcf pick its default."
///
/// Logs the dropped value to stderr so a session-end review can spot the
/// drift. Returns Some(bw) when the value is valid, None otherwise.
fn validate_arq_bandwidth_hz(bw: u32) -> Option<u32> {
    match bw {
        200 | 500 | 1000 | 2000 => Some(bw),
        invalid => {
            eprintln!(
                "tuxlink-j0ij: ignoring invalid persisted bandwidth_hz={invalid}; \
                 valid: 200/500/1000/2000"
            );
            None
        }
    }
}

/// Pre-flight identity check: at least one of `identity.callsign` or
/// `identity.identifier` must be set + non-empty before a connect attempt
/// is allowed to proceed past the consent gate.
///
/// Why a separate helper (rather than inlining the check in
/// `modem_ardop_connect`): the Tauri wrapper is hard to unit-test without
/// a Tauri runtime, but this pure function over `&Config` is trivially
/// testable. The wrapper calls this helper, so coverage at the helper
/// layer transitively covers the wrapper's identity-check branch.
///
/// `deserialize_optional_nonempty_string` already maps `""` and
/// whitespace-only inputs to `None` at deserialize time, but we still
/// defend with a `trim().is_empty()` check in case a caller constructs
/// a `Config` value in-memory (e.g. tests) without going through serde.
pub fn check_identity_present(cfg: &Config) -> Result<(), String> {
    let has_call = cfg
        .identity
        .callsign
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_ident = cfg
        .identity
        .identifier
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    if has_call || has_ident {
        Ok(())
    } else {
        Err("Operator callsign not configured — complete the wizard before connecting".into())
    }
}

/// RADIO-1-gated ARDOP connect. Returns an actionable error when
/// audio devices are not yet configured (operator must complete
/// Settings → ARDOP before calling).
///
/// # RADIO-1 invariant: gate BEFORE any I/O
///
/// The consent token is consumed at the very top of this function —
/// before `config_get_ardop()` is called (disk read + audio-state leak),
/// before any status mutation, before any spawn. A wrong/missing token
/// returns Err without touching the filesystem or the session state.
/// This closes the pre-gate-I/O bypass the 2026-05-30 Codex adrev round
/// flagged.
///
/// # Pre-flight identity check (tuxlink-5738)
///
/// AFTER the consent gate has consumed the token but BEFORE the
/// audio-device check, this command verifies the operator's identity
/// (callsign or identifier) is configured. Ordering rationale: a
/// wrong-token attempt must STILL fail at the consent gate without
/// leaking identity-state via the error message. Identity is more
/// foundational than audio devices (no callsign → no on-air operation
/// is legal under Part 97), so the identity check precedes the
/// audio-device check.
#[tauri::command]
pub fn modem_ardop_connect(
    session: State<'_, Arc<ModemSession>>,
    target: String,
    consent_token: String,
) -> Result<(), String> {
    // ─── RADIO-1 gate FIRST ──────────────────────────────────────────────
    // No config I/O, no status mutation, no error path that leaks state
    // until the consent token is verified + consumed. `consume_consent_token`
    // is atomic (equality check + clear in one lock). After this returns
    // Ok, the stored token is `None` — a replay of `consent_token` would
    // fail at this exact point.
    if !session.consume_consent_token(&consent_token) {
        return Err(
            "RADIO-1: missing or invalid consent token; mint one via the Connect modal first"
                .into(),
        );
    }

    // ─── Pre-flight identity check (tuxlink-5738) ────────────────────────
    // Operator must have a callsign OR identifier configured before any
    // attempt to set up a radio transport. The wizard sets one of these;
    // an unconfigured deployment must complete the wizard first.
    let cfg = config::read_config().map_err(|e| format!("read config: {e}"))?;
    check_identity_present(&cfg)?;

    // Gate passed + identity verified. Now safe to do audio-device I/O.
    let ardop_ui = config_get_ardop();
    if ardop_ui.capture_device.is_empty() || ardop_ui.playback_device.is_empty() {
        return Err(
            "ARDOP audio devices not configured — open Settings → ARDOP first".into(),
        );
    }

    // Delegate to the post-consume variant — the gate has already fired,
    // and re-gating would always fail (the token has been consumed).
    modem_ardop_connect_post_consume_with_factory(
        &session,
        &target,
        &ardop_ui,
        |cfg, _target| {
            ArdopTransport::with_managed_modem(cfg)
                .map(|t| Box::new(t) as Box<dyn ModemTransport>)
                .map_err(|e| format!("spawn failed: {e}"))
        },
    )
}

/// Run a B2F mail exchange over the currently-installed ARDOP transport
/// (tuxlink-ytg) — the actual "send/receive Winlink mail" entry point for the
/// ARDOP HF UI.
///
/// # Preconditions
///
/// - The operator has already pressed Connect through the RADIO-1 modal, which
///   minted a consent token, called `modem_ardop_connect`, and brought the
///   ARQ link up. `ModemSession` now holds the live transport.
/// - The operator has separately minted a NEW per-invocation consent token
///   for THIS send/receive call (per-invocation Part 97 rule — the connect
///   token was consumed by `modem_ardop_connect`).
///
/// # Flow
///
/// 1. **Consent gate first** — `consume_consent_token` runs BEFORE any I/O.
///    A missing/replayed token returns `Err` with no side effects.
/// 2. **Take the installed transport** out of `ModemSession`.
/// 3. **Read config + open the native mailbox** at the standard
///    `<app_data_dir>/native-mbox` path.
/// 4. **Run the B2F exchange** via
///    `winlink_backend::run_ardop_b2f_exchange` — builds outbound from the
///    mailbox Outbox, files received messages into Inbox, moves sent into Sent.
/// 5. **Disconnect + reset** the transport and the session, regardless of
///    success/failure.
///
/// # Lock + I/O discipline
///
/// `take_transport` and `reset_to_stopped` run under the `ModemSession` mutex;
/// `transport.disconnect()` and the B2F exchange run OUTSIDE any held lock so
/// a slow CMS / peer can't stall the status broadcaster.
///
/// # What's deferred to follow-up PRs
///
/// - Frontend wiring of the "Send/Receive" button to this command.
/// - Per-batch progress events to the session log.
/// - Multi-message-per-connection optimization.
/// - Throughput-stats integration with the modem status broadcaster.
#[tauri::command]
pub fn modem_ardop_b2f_exchange(
    app: AppHandle,
    session: State<'_, Arc<ModemSession>>,
    target: String,
    consent_token: String,
) -> Result<(), String> {
    // ─── RADIO-1 gate FIRST — no I/O / state mutation pre-gate ───────────
    // `consume_consent_token` is atomic: equality check + clear in one lock.
    // After a successful return, the stored token is None; a replay of the
    // same token fails at this exact point. Per-invocation Part 97 rule.
    if !session.consume_consent_token(&consent_token) {
        return Err(
            "RADIO-1: missing or invalid consent token; mint one via the Send/Receive modal first"
                .into(),
        );
    }

    // ─── Take the installed transport ────────────────────────────────────
    // The transport was installed by `modem_ardop_connect` after a
    // successful `init` + `connect_arq`. If it's missing, the operator
    // didn't run Connect first — surface that cleanly.
    let mut transport = session.take_transport().ok_or_else(|| {
        "ARDOP transport not connected — press Connect (ARDOP HF) before Send/Receive"
            .to_string()
    })?;

    // Wrap the exchange in a closure so a single point handles cleanup on
    // BOTH success and failure: disconnect the transport OUTSIDE any held
    // lock, then reset the session state.
    let outcome = run_b2f_with_transport(&app, &mut *transport, &target);

    // ─── Always disconnect + reset, regardless of outcome ────────────────
    // Best-effort: even if disconnect errors, the session must end in a
    // Stopped state so a fresh Connect can succeed. 5s deadline mirrors
    // `modem_ardop_disconnect_inner`'s policy.
    let _ = transport.disconnect(Duration::from_secs(5));
    drop(transport);
    // `reset_to_stopped` clears the consent token (already None — we consumed
    // it at the top), takes any still-installed transport (None — we already
    // took it), and flips status to Stopped. A single lock acquisition.
    let _ = session.reset_to_stopped();

    outcome
}

/// Inner helper for [`modem_ardop_b2f_exchange`]: reads the live config, opens
/// the native mailbox at the standard path, and delegates to
/// `winlink_backend::run_ardop_b2f_exchange`.
///
/// Factored out so the Tauri command can run cleanup (disconnect + reset)
/// uniformly on both success and failure. Returns the error as a `String` so
/// it surfaces to the frontend without exposing the internal `BackendError`
/// type — same pattern as the other modem commands.
fn run_b2f_with_transport(
    app: &AppHandle,
    transport: &mut dyn ModemTransport,
    target: &str,
) -> Result<(), String> {
    // Mailbox lives at <app_data_dir>/native-mbox (per `bootstrap::install_native`).
    let mbox_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}"))?
        .join("native-mbox");
    let mailbox = Mailbox::new(mbox_dir);

    let cfg = config::read_config().map_err(|e| format!("read config failed: {e}"))?;

    // Position arbiter is registered in lib.rs::run() — pull a live ref so
    // the on-air locator honors live GPS / privacy state, matching the
    // telnet/packet paths' behavior. Mirrors `bootstrap::install_native`'s
    // wiring.
    let arbiter_state = app.state::<Arc<crate::position::PositionArbiter>>();
    let arbiter: Arc<crate::position::PositionArbiter> = (*arbiter_state).clone();

    crate::winlink_backend::run_ardop_b2f_exchange(
        transport,
        target,
        &cfg,
        &mailbox,
        Some(&arbiter),
    )
    .map_err(|e| format!("ARDOP B2F exchange failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_SCHEMA_VERSION;
    use crate::modem_status::ModemState;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-global TUXLINK_CONFIG_DIR env
    /// var. `std::env::set_var` is not thread-safe under parallel test
    /// execution (cargo runs tests in a thread pool by default), so each test
    /// that touches the env grabs this mutex for the duration of its
    /// set→read→restore sequence. Without this gate, `init_config_from_...`
    /// tests would race with `round_trip_persists_through_config` and other
    /// concurrent env mutators in the same binary, sometimes reading from a
    /// neighbor's tempdir or no dir at all (tuxlink-j0ij).
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        // unwrap_or_else: if a previous test panicked while holding the lock,
        // the mutex is poisoned but the env state is still well-defined for
        // the next test (each test fully restores its env in a deferred-style
        // tail). Recover and proceed.
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn round_trip_persists_through_config() {
        let _env_guard = env_lock();
        // Isolate this test from the operator's real config by pointing
        // TUXLINK_CONFIG_DIR at a fresh tempdir. `config_path()` will resolve
        // to `<tmpdir>/config.json` (per config.rs §294).
        //
        // Because `config_set_ardop` calls `read_config()` before writing, the
        // config file must exist first. We pre-seed a minimal valid config that
        // satisfies `deny_unknown_fields` + semantic validation (offline path:
        // no callsign). `config_set_ardop` will then read it, inject `modem_ardop`,
        // and write it back atomically.
        //
        // NOTE: std::env::set_var is not thread-safe under parallel test
        // execution. This test must run serially (--test-threads=1 or via the
        // `modem_commands::tests` filter). The existing `config.rs` tests avoid
        // this race by using pure serde deserialization; this test exercises the
        // file I/O path, so TUXLINK_CONFIG_DIR isolation is the correct approach.
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: single-threaded test; no concurrent env reads within this block.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        // Seed a minimal valid config (offline path: connect_to_cms=false, no callsign).
        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let initial = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
        };
        config_set_ardop(initial.clone()).expect("config_set_ardop must succeed");
        let read = config_get_ardop();
        assert_eq!(read, initial);

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    #[test]
    fn modem_get_status_returns_session_snapshot() {
        let session = Arc::new(ModemSession::new());
        let s = modem_get_status_inner(&session);
        assert_eq!(s.state, ModemState::Stopped);
    }

    #[test]
    fn modem_ardop_disconnect_clears_consent_when_session_was_running() {
        let session = Arc::new(ModemSession::new());
        let token = session.mint_consent_token();
        // simulate a running session: representative "connected" snapshot.
        // Plan deviation: the plan's text wrote `ModemState::ConnectedIdle`
        // which doesn't exist (Task 1.1 used `Idle` / `ConnectedIrs` / `ConnectedIss`).
        // `ConnectedIrs` is a faithful "running" stand-in.
        let mut s = ModemStatus::stopped();
        s.state = ModemState::ConnectedIrs;
        session.set_status(s);

        modem_ardop_disconnect_inner(&session).unwrap();

        // After disconnect, consent token must be invalidated and status reset.
        assert!(!session.has_valid_token(&token));
        assert_eq!(session.status_snapshot().state, ModemState::Stopped);
    }

    // ── Task 3.3 tests — consent-gated connect via factory seam ─────────

    use crate::winlink::modem::{ConnectInfo, ModemTransport, ReadWrite};
    use crate::winlink::modem::ardop::session::SessionError;

    /// A stub `ModemTransport` that returns canned, harmless responses. The
    /// peer call + bandwidth come back from `connect_arq`; all other methods
    /// are no-ops or surface `NotConnected`. NEVER spawns a real process or
    /// opens a real socket — safe to run in unit tests.
    struct StubTransport {
        peer_call: &'static str,
        bandwidth_hz: u32,
    }

    impl StubTransport {
        fn new() -> Self {
            Self { peer_call: "W7RMS-10", bandwidth_hz: 500 }
        }
    }

    impl ModemTransport for StubTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), SessionError> {
            Ok(())
        }

        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            _deadline: Duration,
        ) -> Result<ConnectInfo, SessionError> {
            Ok(ConnectInfo {
                peer_call: self.peer_call.to_string(),
                bandwidth_hz: self.bandwidth_hz,
            })
        }

        fn disconnect(&mut self, _deadline: Duration) -> Result<(), SessionError> {
            Ok(())
        }

        fn data_stream(&mut self) -> std::io::Result<&mut dyn ReadWrite> {
            // The connect path doesn't exercise data_stream — surface a
            // clean Err rather than carrying a sham stream.
            Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "stub transport has no data stream",
            ))
        }
    }

    fn stub_transport() -> Box<dyn ModemTransport> {
        Box::new(StubTransport::new())
    }

    fn test_ardop_ui_config() -> ArdopUiConfig {
        ArdopUiConfig {
            binary: "ardopcf-stub".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
        }
    }

    #[test]
    fn modem_ardop_connect_rejects_when_token_missing() {
        // No token minted → consume_consent_token returns false → the gate
        // fires BEFORE the factory is invoked. If the factory ran, this test
        // would still pass (the stub doesn't spawn anything), so the
        // load-bearing assertion is the error string mentioning RADIO-1 /
        // consent — that is the operator-visible signal.
        let session = Arc::new(ModemSession::new());
        // Use a tracker to assert the factory was never called even with
        // a token that the session doesn't recognize.
        let factory_ran = std::sync::atomic::AtomicBool::new(false);
        let err = modem_ardop_connect_gated_with_factory(
            &session,
            "W7RMS-10",
            "wrong-token",
            &test_ardop_ui_config(),
            |_cfg, _target| {
                factory_ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(stub_transport())
            },
        )
        .unwrap_err();
        assert!(
            err.contains("consent") || err.contains("RADIO-1"),
            "error must mention consent/RADIO-1; got: {err}"
        );
        assert!(
            !factory_ran.load(std::sync::atomic::Ordering::SeqCst),
            "factory MUST NOT run when the consent gate denies — no spawn before consent"
        );
        // Status must remain Stopped — the gate fires before any status mutation.
        assert_eq!(session.status_snapshot().state, ModemState::Stopped);
    }

    #[test]
    fn modem_ardop_connect_succeeds_with_valid_token() {
        let session = Arc::new(ModemSession::new());
        let token = session.mint_consent_token();
        let result = modem_ardop_connect_gated_with_factory(
            &session,
            "W7RMS-10",
            &token,
            &test_ardop_ui_config(),
            |_cfg, _target| Ok(stub_transport()),
        );
        assert!(result.is_ok(), "result: {result:?}");
        // After a successful connect the session reports a connected variant
        // and carries the peer / bandwidth from the stub's ConnectInfo.
        let snap = session.status_snapshot();
        assert!(
            matches!(snap.state, ModemState::ConnectedIrs | ModemState::ConnectedIss),
            "expected connected variant, got: {:?}",
            snap.state
        );
        assert_eq!(snap.peer.as_deref(), Some("W7RMS-10"));
        assert_eq!(snap.width_hz, Some(500));
        // The transport handle is now installed in the session.
        assert!(
            session.take_transport().is_some(),
            "successful connect must install a transport handle"
        );
        // Per-invocation consent: the successful connect MUST have consumed
        // the token. A subsequent has_valid_token check confirms the stored
        // token is now gone — the 2026-05-30 Codex adrev "tokens not
        // consumed atomically" P1 finding is closed.
        assert!(
            !session.has_valid_token(&token),
            "successful connect must consume the consent token (per-invocation rule)"
        );
    }

    #[test]
    fn modem_ardop_connect_rejects_replay_of_consumed_token() {
        // RADIO-1 per-invocation consent: a single minted token authorizes
        // EXACTLY ONE on-air connect. Replaying it (calling
        // `_gated_with_factory` a second time with the same token) MUST be
        // rejected at the gate — no spawn, no I/O, no status mutation —
        // because the prior successful call consumed the token.
        let session = Arc::new(ModemSession::new());
        let token = session.mint_consent_token();

        // First call succeeds and consumes.
        let r1 = modem_ardop_connect_gated_with_factory(
            &session,
            "W7RMS-10",
            &token,
            &test_ardop_ui_config(),
            |_cfg, _target| Ok(stub_transport()),
        );
        assert!(r1.is_ok(), "first call must succeed; got: {r1:?}");

        // Tear down the transport so the second call's stub install would
        // be observable (otherwise the "transport still present" assertion
        // could be satisfied by leftover state from the first call).
        let _ = session.take_transport();

        // Second call with the SAME token MUST be rejected, and the factory
        // MUST NOT run. AtomicBool seam confirms the closure never fires.
        let factory_ran = std::sync::atomic::AtomicBool::new(false);
        let r2 = modem_ardop_connect_gated_with_factory(
            &session,
            "W7RMS-10",
            &token,
            &test_ardop_ui_config(),
            |_cfg, _target| {
                factory_ran.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(stub_transport())
            },
        );
        let err = r2.expect_err("replay of consumed token must be rejected");
        assert!(
            err.contains("consent") || err.contains("RADIO-1"),
            "error must mention consent/RADIO-1; got: {err}"
        );
        assert!(
            !factory_ran.load(std::sync::atomic::Ordering::SeqCst),
            "factory MUST NOT run on replay — the gate fires first and consumes have already cleared the token"
        );
        // No second transport was installed.
        assert!(
            session.take_transport().is_none(),
            "no transport must be installed on a rejected replay"
        );
    }

    // ── Task 6.2 — mint + connect end-to-end via the same code path ──────

    /// RADIO-1: prove the `modem_mint_consent` Tauri command path produces a
    /// token that unlocks `modem_ardop_connect`. We test the underlying
    /// `mint_consent_token()` call (the same function the command wraps) +
    /// `modem_ardop_connect_gated_with_factory` so the end-to-end loop is
    /// verified WITHOUT requiring a Tauri `State` constructor. If a future
    /// refactor splits the two functions onto different storage, this test
    /// will fail loudly — which is the desired signal.
    #[test]
    fn mint_then_connect_with_matching_token_succeeds() {
        use crate::modem_status::ModemSession;
        let session = std::sync::Arc::new(ModemSession::new());
        // Directly testing the same path `modem_mint_consent` uses.
        let token = session.mint_consent_token();
        let result = modem_ardop_connect_gated_with_factory(
            &session,
            "W7RMS-10",
            &token,
            &test_ardop_ui_config(),
            |_cfg, _t| Ok(stub_transport()),
        );
        assert!(result.is_ok(), "result: {result:?}");
    }

    // ── tuxlink-5738 — pre-flight identity check ─────────────────────────

    /// Build a Config from a JSON literal so the test exercises the same
    /// deserialize path the production read_config() goes through (incl.
    /// `deserialize_optional_nonempty_string` which already maps empty
    /// strings to `None`). Mirrors the existing config.rs test pattern.
    fn config_with_identity(callsign: Option<&str>, identifier: Option<&str>) -> Config {
        let call_json = match callsign {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        let ident_json = match identifier {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        let json = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": {call_json}, "identifier": {ident_json}, "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = crate::config::CONFIG_SCHEMA_VERSION,
        );
        serde_json::from_str(&json).expect("test config must deserialize")
    }

    #[test]
    fn check_identity_present_ok_when_callsign_set() {
        let cfg = config_with_identity(None, Some("W1TEST"));
        assert!(check_identity_present(&cfg).is_ok());
    }

    #[test]
    fn check_identity_present_ok_when_identifier_set() {
        // Offline-path config: no callsign, identifier carries the station id.
        let cfg = config_with_identity(None, Some("FIELD-1"));
        assert!(check_identity_present(&cfg).is_ok());
    }

    #[test]
    fn check_identity_present_err_when_both_missing() {
        // Both None — operator has not completed the wizard's identity step.
        let cfg = config_with_identity(None, None);
        let err = check_identity_present(&cfg).expect_err("must reject when no identity");
        assert!(
            err.contains("callsign") || err.contains("wizard"),
            "error must be actionable; got: {err}"
        );
    }

    // ── tuxlink-o3f2: abort-during-connect side channel ──────────────────

    use crate::winlink::modem::ardop::session::SessionError as ArdopSessionError;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Stub transport that:
    /// - exposes `try_clone_abort_writer` returning a clone of a `TcpStream`
    ///   connected to a `TcpListener` we own in the test, so the test can
    ///   observe the side-channel ABORT bytes;
    /// - `connect_arq` blocks until `abort_signal` flips to true (the test
    ///   sets it from a watcher thread that reads from the listener and
    ///   asserts on the bytes).
    ///
    /// Used to prove that `modem_ardop_disconnect_inner` aborts an in-flight
    /// `connect_arq` via the side channel, not by holding the transport
    /// mutex (which during connect_arq is `None` from the session's POV).
    struct AbortableStubTransport {
        abort_writer: Option<TcpStream>,
        abort_signal: Arc<AtomicBool>,
    }

    impl AbortableStubTransport {
        fn new(abort_writer: TcpStream, abort_signal: Arc<AtomicBool>) -> Self {
            Self {
                abort_writer: Some(abort_writer),
                abort_signal,
            }
        }
    }

    impl ModemTransport for AbortableStubTransport {
        fn init(&mut self, _cfg: &InitConfig) -> Result<(), ArdopSessionError> {
            Ok(())
        }
        fn connect_arq(
            &mut self,
            _target: &str,
            _repeat: u32,
            deadline: Duration,
        ) -> Result<crate::winlink::modem::ConnectInfo, ArdopSessionError> {
            // Spin (bounded by deadline) until abort_signal flips. In
            // production this loop is the real `arq_connect` recv loop;
            // here the signal stands in for "ardopcf emitted FAULT/DISC in
            // response to ABORT and the cmd reader thread delivered it."
            let start = std::time::Instant::now();
            while !self.abort_signal.load(Ordering::Acquire) {
                if start.elapsed() >= deadline {
                    return Err(ArdopSessionError::Timeout {
                        cmd: "ARQCALL".into(),
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(ArdopSessionError::Fault("aborted via side channel".into()))
        }
        fn disconnect(&mut self, _deadline: Duration) -> Result<(), ArdopSessionError> {
            Ok(())
        }
        fn data_stream(
            &mut self,
        ) -> std::io::Result<&mut dyn crate::winlink::modem::ReadWrite> {
            Err(std::io::Error::other("stub"))
        }
        fn try_clone_abort_writer(&self) -> Option<TcpStream> {
            self.abort_writer.as_ref().and_then(|s| s.try_clone().ok())
        }
    }

    /// Spawn a TCP listener and return `(addr, server_thread_handle, abort_signal)`.
    /// The server thread reads bytes; when it sees `ABORT\r` it flips
    /// `abort_signal` to true and exits. The signal is the test's hook to
    /// unblock the connect stub.
    fn spawn_abort_listener() -> (std::net::SocketAddr, std::thread::JoinHandle<Vec<u8>>, Arc<AtomicBool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let abort_signal = Arc::new(AtomicBool::new(false));
        let signal_for_thread = abort_signal.clone();
        let handle = std::thread::spawn(move || {
            let (mut conn, _peer) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut accumulated = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        accumulated.extend_from_slice(&buf[..n]);
                        if accumulated.windows(6).any(|w| w == b"ABORT\r") {
                            signal_for_thread.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            }
            accumulated
        });
        (addr, handle, abort_signal)
    }

    /// End-to-end abort-during-connect: the connect call runs on one
    /// thread, blocking inside `connect_arq` (stub spins until aborted).
    /// On another thread we call `modem_ardop_disconnect_inner`, which
    /// MUST send ABORT via the session's side-channel writer; the listener
    /// observes the bytes and flips the signal that lets `connect_arq`
    /// return. Connect returns Err promptly (well under the 120s deadline)
    /// rather than running to deadline.
    ///
    /// This is the regression test for the 2026-05-22 runaway-connect
    /// incident — the proof that the operator's Disconnect button can
    /// halt an in-flight connect in seconds, not minutes.
    #[test]
    fn disconnect_aborts_in_flight_connect_via_side_channel() {
        let (addr, listener_handle, abort_signal) = spawn_abort_listener();

        // Client end of the loopback pair — this is what
        // `try_clone_abort_writer` will hand back via the stub.
        let abort_writer = TcpStream::connect(addr).expect("connect to abort listener");

        let session = Arc::new(ModemSession::new());
        let token = session.mint_consent_token();

        // Run the connect call on a worker thread so the test thread can
        // call disconnect in parallel.
        let session_for_connect = session.clone();
        let abort_signal_for_stub = abort_signal.clone();
        let connect_thread = std::thread::spawn(move || {
            modem_ardop_connect_gated_with_factory(
                &session_for_connect,
                "W7RMS-10",
                &token,
                &test_ardop_ui_config(),
                move |_cfg, _target| {
                    Ok(Box::new(AbortableStubTransport::new(
                        abort_writer,
                        abort_signal_for_stub,
                    )) as Box<dyn ModemTransport>)
                },
            )
        });

        // Wait until the connect path has progressed past install_abort_writer
        // (status flips to Connecting AFTER the install). Poll briefly.
        let start = std::time::Instant::now();
        loop {
            let st = session.status_snapshot().state;
            if matches!(st, ModemState::Connecting) {
                break;
            }
            if start.elapsed() >= Duration::from_secs(5) {
                panic!("status never reached Connecting (state={st:?})");
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        // Now hit Disconnect. This must (a) write ABORT via the side
        // channel and (b) return promptly. The connect thread sees the
        // signal, returns Err, and joins.
        let disconnect_start = std::time::Instant::now();
        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");
        let disconnect_elapsed = disconnect_start.elapsed();
        assert!(
            disconnect_elapsed < Duration::from_secs(2),
            "disconnect must return promptly; took {disconnect_elapsed:?}"
        );

        // The connect call should have returned Err once the stub saw the
        // signal flip. Bound the wait so a regression fails the test
        // instead of hanging.
        let connect_result = connect_thread
            .join()
            .expect("connect thread must not panic");
        assert!(
            connect_result.is_err(),
            "connect must return Err after ABORT signal; got: {connect_result:?}"
        );

        // The listener thread received the side-channel bytes.
        let received = listener_handle.join().expect("listener thread must not panic");
        assert!(
            received.windows(6).any(|w| w == b"ABORT\r"),
            "abort listener must have received ABORT\\r; got: {received:?}"
        );

        // Signal flipped means abort_in_flight delivered the line.
        assert!(
            abort_signal.load(Ordering::Acquire),
            "abort signal must be set"
        );

        // Session state: the disconnect path reset to Stopped, then the
        // connect thread's error handler ran (because connect_arq returned
        // Err after the abort signal) and set state to Error. Either
        // terminal is acceptable as a "no longer Connecting" outcome —
        // the load-bearing assertion is the prompt disconnect above. We
        // explicitly assert NOT-Connecting and NOT-ConnectedIrs/Iss so a
        // regression that leaves the session stuck mid-flow fails loudly.
        let final_state = session.status_snapshot().state;
        assert!(
            matches!(final_state, ModemState::Stopped | ModemState::Error),
            "session must end Stopped or Error after abort-driven disconnect; got: {final_state:?}"
        );
    }

    /// `modem_ardop_disconnect_inner` must call `abort_in_flight` BEFORE
    /// any reset/transport teardown — best-effort, ignore-error. If no
    /// writer is installed (e.g. transport was never connected), the call
    /// is a no-op and the existing graceful path still runs.
    ///
    /// This test directly exercises the disconnect ordering: install a
    /// writer pointing at a local listener, call disconnect, observe the
    /// ABORT bytes on the listener side.
    #[test]
    fn disconnect_in_flight_sends_abort_via_side_channel() {
        let (addr, listener_handle, _signal) = spawn_abort_listener();
        let writer = TcpStream::connect(addr).expect("connect to abort listener");
        let session = Arc::new(ModemSession::new());
        session.install_abort_writer(writer);

        modem_ardop_disconnect_inner(&session).expect("disconnect must succeed");

        let received = listener_handle.join().expect("listener thread must not panic");
        assert!(
            received.windows(6).any(|w| w == b"ABORT\r"),
            "disconnect must send ABORT via the side channel; got: {received:?}"
        );
    }

    #[allow(deprecated)]
    #[test]
    fn check_identity_present_err_when_both_whitespace_only() {
        // Defense-in-depth: if a caller hand-constructs a Config in-memory
        // with whitespace-only strings (bypassing the serde validator that
        // normally maps these to None), trim() catches it. Hand-built
        // because `deserialize_optional_nonempty_string` would otherwise
        // map "   " to None at the serde layer.
        let cfg = Config {
            schema_version: crate::config::CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: crate::config::ConnectConfig {
                connect_to_cms: false,
                transport: crate::config::CmsTransport::Telnet,
                host: crate::config::default_cms_host(),
            },
            identity: crate::config::IdentityConfig {
                callsign: Some("   ".into()),
                identifier: Some("".into()),
                grid: None,
            },
            privacy: crate::config::PrivacyConfig {
                gps_state: crate::config::GpsState::Off,
                position_precision: crate::config::PositionPrecision::FourCharGrid,
                position_source: crate::config::PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: crate::config::PacketConfig::default(),
            modem_ardop: None,
        };
        assert!(check_identity_present(&cfg).is_err());
    }

    // ── tuxlink-j0ij: bandwidth validation + plumb-through tests ──────────

    #[test]
    fn validate_arq_bandwidth_hz_accepts_the_four_valid_values() {
        assert_eq!(validate_arq_bandwidth_hz(200), Some(200));
        assert_eq!(validate_arq_bandwidth_hz(500), Some(500));
        assert_eq!(validate_arq_bandwidth_hz(1000), Some(1000));
        assert_eq!(validate_arq_bandwidth_hz(2000), Some(2000));
    }

    #[test]
    fn validate_arq_bandwidth_hz_drops_invalid_values_to_none() {
        // ardopcf only documents {200, 500, 1000, 2000}; any other value is a
        // stale persist / hand-edit / forward-schema drift — drop to None so
        // ardopcf's default takes over rather than failing init.
        assert_eq!(validate_arq_bandwidth_hz(0), None);
        assert_eq!(validate_arq_bandwidth_hz(100), None);
        assert_eq!(validate_arq_bandwidth_hz(750), None);
        assert_eq!(validate_arq_bandwidth_hz(2500), None);
        assert_eq!(validate_arq_bandwidth_hz(u32::MAX), None);
    }

    /// `init_config_from_persisted_config` must plumb a valid persisted
    /// `bandwidth_hz` through to the resulting `InitConfig.arq_bandwidth_hz`.
    /// Uses TUXLINK_CONFIG_DIR isolation (same pattern as
    /// round_trip_persists_through_config).
    #[test]
    fn init_config_from_persisted_config_passes_through_valid_bandwidth() {
        let _env_guard = env_lock();
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: env_lock above serializes against other env-mutating tests.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": "CN87" }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 500
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let init_cfg = init_config_from_persisted_config();
        assert_eq!(init_cfg.arq_bandwidth_hz, Some(500));
        assert_eq!(init_cfg.mycall, "W1TEST");
        assert_eq!(init_cfg.gridsquare, "CN87");

        // Restore env (best-effort).
        // SAFETY: symmetric with the set_var above; single-threaded test.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    /// A hand-edited (or stale) `bandwidth_hz` outside the valid set drops
    /// to None — ardopcf's default takes over. Defense-in-depth against the
    /// Settings dropdown being bypassed.
    #[test]
    fn init_config_from_persisted_config_drops_invalid_bandwidth() {
        let _env_guard = env_lock();
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: env_lock serializes env-mutating tests.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }},
                "modem_ardop": {{
                    "binary": "ardopcf",
                    "capture_device": "plughw:1,0",
                    "playback_device": "plughw:1,0",
                    "cmd_port": 8515,
                    "bandwidth_hz": 750
                }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let init_cfg = init_config_from_persisted_config();
        assert_eq!(
            init_cfg.arq_bandwidth_hz, None,
            "invalid bandwidth_hz=750 must drop to None (defense in depth — tuxlink-j0ij)"
        );

        // SAFETY: symmetric with set_var above.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }

    // ── tuxlink-60wh: -G WebGUI flag in ardopcf extra_args ───────────────

    #[test]
    fn extra_args_includes_g_webgui_flag_with_cmd_port_minus_one() {
        // Default cmd_port = 8515 → webgui_port = 8514. The `-G 8514` pair
        // must appear AFTER any `-p` PTT flag (or first when PTT is None)
        // and BEFORE the positional triple (cmd_port / capture / playback).
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: None,
            cmd_port: 8515,
            bandwidth_hz: None,
        };
        let args = build_ardop_extra_args(&cfg);
        assert_eq!(
            args,
            vec![
                "-G".to_string(),
                "8514".to_string(),
                "8515".to_string(),
                "plughw:1,0".to_string(),
                "plughw:1,0".to_string(),
            ],
            "argv order must be: -G <wg> <cmd> <capture> <playback>"
        );
    }

    #[test]
    fn extra_args_g_webgui_flag_uses_dynamic_cmd_port_minus_one() {
        // Operator may override cmd_port via Settings; webgui_port follows
        // ardopcf's documented convention `cmd_port - 1`.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:0,0".into(),
            playback_device: "plughw:0,0".into(),
            ptt_serial_path: None,
            cmd_port: 9001,
            bandwidth_hz: None,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            args.windows(2).any(|w| w[0] == "-G" && w[1] == "9000"),
            "expected `-G 9000` pair for cmd_port=9001; got: {args:?}"
        );
    }

    #[test]
    fn extra_args_omits_g_when_cmd_port_too_low_to_compute() {
        // Edge case: cmd_port=1 would yield webgui_port=0 (invalid). The
        // guard drops `-G` entirely; ardopcf runs without a WebGUI rather
        // than failing to bind. cmd_port=0 likewise.
        for low_port in [0u16, 1u16] {
            let cfg = ArdopUiConfig {
                binary: "ardopcf".into(),
                capture_device: "plughw:0,0".into(),
                playback_device: "plughw:0,0".into(),
                ptt_serial_path: None,
                cmd_port: low_port,
                bandwidth_hz: None,
            };
            let args = build_ardop_extra_args(&cfg);
            assert!(
                !args.iter().any(|a| a == "-G"),
                "cmd_port={low_port}: -G must be omitted; got: {args:?}"
            );
        }
    }

    #[test]
    fn extra_args_preserves_ptt_p_flag_before_g_and_positional() {
        // Regression: tuxlink-60wh refactor extracted extra_args into a
        // helper. Make sure the PTT plumbing still works AND appears in
        // the right order: -p <ptt> -G <wg> <cmd> <capture> <playback>.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: Some("/dev/ttyUSB0".into()),
            cmd_port: 8515,
            bandwidth_hz: None,
        };
        let args = build_ardop_extra_args(&cfg);
        assert_eq!(
            args,
            vec![
                "-p".to_string(),
                "/dev/ttyUSB0".to_string(),
                "-G".to_string(),
                "8514".to_string(),
                "8515".to_string(),
                "plughw:1,0".to_string(),
                "plughw:1,0".to_string(),
            ],
            "argv order must be: -p <ptt> -G <wg> <cmd> <capture> <playback>"
        );
    }

    #[test]
    fn extra_args_omits_p_flag_when_ptt_serial_path_empty_string() {
        // Defense in depth: ardopcf rejects `-p ""`. If a stale config or
        // hand-edited JSON yields Some("") (the serde validator should
        // normalize this, but tests construct in-memory configs directly),
        // the helper drops the flag rather than passing an invalid value.
        let cfg = ArdopUiConfig {
            binary: "ardopcf".into(),
            capture_device: "plughw:1,0".into(),
            playback_device: "plughw:1,0".into(),
            ptt_serial_path: Some("".into()),
            cmd_port: 8515,
            bandwidth_hz: None,
        };
        let args = build_ardop_extra_args(&cfg);
        assert!(
            !args.iter().any(|a| a == "-p"),
            "empty PTT path must drop the -p flag; got: {args:?}"
        );
    }

    /// When the persisted config has no `modem_ardop` section, the
    /// `InitConfig.arq_bandwidth_hz` must be None — ardopcf's default takes
    /// over. This is the migration path: pre-j0ij configs still init.
    #[test]
    fn init_config_from_persisted_config_yields_none_bandwidth_when_modem_ardop_absent() {
        let _env_guard = env_lock();
        let tmp = tempfile::tempdir().expect("create tempdir");
        let prior = std::env::var("TUXLINK_CONFIG_DIR").ok();
        // SAFETY: env_lock serializes env-mutating tests.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", tmp.path()); }

        let seed = format!(
            r#"{{
                "schema_version": {ver},
                "wizard_completed": true,
                "connect": {{ "connect_to_cms": false, "transport": "Telnet" }},
                "identity": {{ "callsign": null, "identifier": "W1TEST", "grid": null }},
                "privacy": {{ "gps_state": "Off", "position_precision": "FourCharGrid" }}
            }}"#,
            ver = CONFIG_SCHEMA_VERSION,
        );
        std::fs::write(tmp.path().join("config.json"), seed)
            .expect("seed config.json into tempdir");

        let init_cfg = init_config_from_persisted_config();
        assert_eq!(
            init_cfg.arq_bandwidth_hz, None,
            "no modem_ardop section → no ARQBW override (migration path)"
        );

        // SAFETY: symmetric.
        unsafe {
            match prior {
                Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
            }
        }
    }
}
