//! Tauri commands for modem (ARDOP) operations.
//!
//! RADIO-1: `modem_ardop_connect` requires a per-session consent token issued
//! by the frontend's RADIO-1 modal. The backend rejects any connect attempt
//! whose token doesn't match the current session token. See Phase 6.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

use crate::config::{self, ArdopUiConfig};
use crate::modem_status::{ModemSession, ModemState, ModemStatus};
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
pub fn modem_ardop_disconnect_inner(session: &Arc<ModemSession>) -> Result<(), String> {
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

/// Inner helper: RADIO-1-gated ARDOP connect via the default factory (spawns
/// real `ArdopTransport::with_managed_modem`). Tests use
/// `modem_ardop_connect_inner_with_factory` to inject a stub transport.
pub fn modem_ardop_connect_inner(
    session: &Arc<ModemSession>,
    target: &str,
    consent_token: &str,
    ardop_ui: &ArdopUiConfig,
) -> Result<(), String> {
    modem_ardop_connect_inner_with_factory(
        session,
        target,
        consent_token,
        ardop_ui,
        |cfg, _target| {
            ArdopTransport::with_managed_modem(cfg)
                .map(|t| Box::new(t) as Box<dyn ModemTransport>)
                .map_err(|e| format!("spawn failed: {e}"))
        },
    )
}

/// Inner helper with a factory seam — Step 4 of Task 3.3. The factory closure
/// constructs the `Box<dyn ModemTransport>` given an `ArdopConfig` and the
/// target callsign. Production calls hand in
/// `ArdopTransport::with_managed_modem`; tests hand in a stub.
///
/// # RADIO-1
///
/// The first check is the per-session consent-token comparison. ANY call
/// with a missing-or-wrong token returns `Err` BEFORE the factory runs,
/// BEFORE `init`, BEFORE `connect_arq` — i.e., no spawn, no socket bind,
/// no I/O whatsoever. The token is in-process replay protection (mints
/// via `modem_mint_consent`, not yet implemented at the time of this
/// function — Task 6.2); a compromised renderer cannot self-mint because
/// the token is generated server-side. Plain string equality on the wire
/// is the design.
///
/// # Bounded airtime
///
/// `connect_arq` is bounded by [`CONNECT_DEADLINE`] (120s). The 2026-05-22
/// runaway-connect incident is the calibration: a 110s no-abort runaway
/// forced a radio power-off. There is NO retry loop in this function — if
/// `init` or `connect_arq` fails, the status flips to `Error` and we
/// return immediately. A retry must be a fresh user-initiated Connect
/// with a fresh consent token (Part 97 per-invocation rule).
pub fn modem_ardop_connect_inner_with_factory<F>(
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
    if !session.has_valid_token(consent_token) {
        return Err(
            "RADIO-1: missing or invalid consent token; mint one via the Connect modal first"
                .into(),
        );
    }

    // ─── Translate ArdopUiConfig (frontend) → ArdopConfig (backend) ─────
    // ardopcf's positional CLI is `ardopcf [-p <ptt>] <cmd_port> <capture> <playback>`.
    // The PTT flag, when present, must precede the positional triple.
    let mut extra_args: Vec<String> = Vec::with_capacity(5);
    if let Some(ref ptt) = ardop_ui.ptt_serial_path {
        extra_args.push("-p".into());
        extra_args.push(ptt.clone());
    }
    extra_args.push(ardop_ui.cmd_port.to_string());
    extra_args.push(ardop_ui.capture_device.clone());
    extra_args.push(ardop_ui.playback_device.clone());

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
/// and `gridsquare` from `identity.grid` (defaulting to `"AA00"` when no
/// grid is set — the ARDOP TNC requires a non-empty value but the broadcast
/// precision gate happens upstream in the position layer).
fn init_config_from_persisted_config() -> InitConfig {
    let (mycall, grid) = config::read_config()
        .map(|c| {
            // Prefer callsign (CMS path); fall back to identifier (offline path).
            let call = c
                .identity
                .callsign
                .clone()
                .or_else(|| c.identity.identifier.clone())
                .unwrap_or_default();
            (call, c.identity.grid.unwrap_or_default())
        })
        .unwrap_or_default();

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
    }
}

/// RADIO-1-gated ARDOP connect. Returns an actionable error when
/// audio devices are not yet configured (operator must complete
/// Settings → ARDOP before calling).
#[tauri::command]
pub fn modem_ardop_connect(
    session: State<'_, Arc<ModemSession>>,
    target: String,
    consent_token: String,
) -> Result<(), String> {
    let ardop_ui = config_get_ardop();
    if ardop_ui.capture_device.is_empty() || ardop_ui.playback_device.is_empty() {
        return Err(
            "ARDOP audio devices not configured — open Settings → ARDOP first".into(),
        );
    }
    modem_ardop_connect_inner(&session, &target, &consent_token, &ardop_ui)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONFIG_SCHEMA_VERSION;
    use crate::modem_status::ModemState;

    #[test]
    fn round_trip_persists_through_config() {
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
        }
    }

    #[test]
    fn modem_ardop_connect_rejects_when_token_missing() {
        // No token minted → has_valid_token returns false → the gate fires
        // BEFORE the factory is invoked. If the factory ran, this test
        // would still pass (the stub doesn't spawn anything), so the
        // load-bearing assertion is the error string mentioning RADIO-1 /
        // consent — that is the operator-visible signal.
        let session = Arc::new(ModemSession::new());
        // Use a tracker to assert the factory was never called even with
        // a token that the session doesn't recognize.
        let factory_ran = std::sync::atomic::AtomicBool::new(false);
        let err = modem_ardop_connect_inner_with_factory(
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
        let result = modem_ardop_connect_inner_with_factory(
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
    }

    // ── Task 6.2 — mint + connect end-to-end via the same code path ──────

    /// RADIO-1: prove the `modem_mint_consent` Tauri command path produces a
    /// token that unlocks `modem_ardop_connect`. We test the underlying
    /// `mint_consent_token()` call (the same function the command wraps) +
    /// `modem_ardop_connect_inner_with_factory` so the end-to-end loop is
    /// verified WITHOUT requiring a Tauri `State` constructor. If a future
    /// refactor splits the two functions onto different storage, this test
    /// will fail loudly — which is the desired signal.
    #[test]
    fn mint_then_connect_with_matching_token_succeeds() {
        use crate::modem_status::ModemSession;
        let session = std::sync::Arc::new(ModemSession::new());
        // Directly testing the same path `modem_mint_consent` uses.
        let token = session.mint_consent_token();
        let result = modem_ardop_connect_inner_with_factory(
            &session,
            "W7RMS-10",
            &token,
            &test_ardop_ui_config(),
            |_cfg, _t| Ok(stub_transport()),
        );
        assert!(result.is_ok(), "result: {result:?}");
    }
}
