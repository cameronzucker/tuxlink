//! Action catalog service seams + registry (spec §6). Plan Task 4a landed
//! the narrow trait-object ports every `needs_radio` action delegates
//! through (`ConnectService`/`AprsService`/`ListenService`), `ActionDeps`
//! (the constructor bag `build_registry` consumes), and the shared
//! envelope-parsing helpers every radio action in `radio.rs` uses. Plan
//! Task 4b adds the CAT verb seam (`RigService`) and registers the five
//! `rig.*` actions from `cat.rs`. `data.rs`/`local.rs` (plan Tasks 4c/4d)
//! extend [`ActionDeps`] with their own service trait objects and
//! [`build_registry`] with their own `registry.register(...)` calls — see
//! the marked extension points below. Adding those modules does not change
//! anything in this file except those two marked spots.
//!
//! Plan: `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 4. Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6.

pub mod cat;
pub mod radio;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::ActionRegistry;
use tuxlink_routines::types::BusyPolicy;

use super::arbiter::{Holder, RadioArbiter};

// ============================================================================
// Service seams (spec §6's per-action delegation targets, narrowed to what
// each action's logic actually needs — see radio.rs's module doc for the
// full transport-seam recon behind each of these).
// ============================================================================

/// Outcome of one `radio.connect` dial attempt against a single
/// station×band combination — the payload [`ConnectService::connect_attempt`]
/// returns on the `Ok` path. `connected: false` is a NORMAL, expected
/// outcome (this attempt didn't reach the gateway; the action's loop moves
/// on to the next station/band combo) — see [`ConnectService`]'s doc for the
/// distinct hard-failure signal (`Err`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectOutcome {
    pub connected: bool,
    /// Which gateway/channel actually answered — `None` when `connected` is
    /// `false`. Echoed into `radio.connect`'s `gateway` output field.
    pub gateway: Option<String>,
    /// Populated when `connected` is `false` — the verbatim reason THIS
    /// attempt failed (spec Global Constraints: never paraphrased). Becomes
    /// the loop's running `last_error` and, if every combo is exhausted, the
    /// step's `last_error` output field.
    pub error: Option<String>,
}

/// The `radio.connect` delegation seam (spec §6 "Connect attempt").
///
/// # Contract — soft vs. hard failure
///
/// `Ok(ConnectOutcome { connected: false, .. })` is a SOFT, per-attempt
/// failure: the caller's station×band loop records `error` as the running
/// `last_error` and tries the next combination — exhausting every
/// combination is itself an `Ok` step OUTPUT (`{"connected": false,
/// "last_error": ...}`), never a `StepError` (plan Task 4's explicit
/// contract). `Err(String)` is a HARD, transport-level failure (rig
/// unreachable, audio device not configured, no active identity, an
/// unresolvable gateway frequency, …) that the caller surfaces immediately
/// as `StepError::Action`, verbatim, with no further attempts. Getting this
/// distinction right is the whole point of the trait: a real gateway simply
/// not answering must never abort the rest of the station/band walk, but a
/// genuinely broken transport must not spin through every remaining
/// combination pretending each might work.
#[async_trait]
pub trait ConnectService: Send + Sync {
    async fn connect_attempt(
        &self,
        station: &str,
        band: Option<&str>,
    ) -> Result<ConnectOutcome, String>;
}

/// The `radio.aprs_send` delegation seam (spec §6). `to: None` is a
/// broadcast (blank addressee, no msgno) — mirrors `AprsState::send`'s own
/// `Option<String>` contract (`ui_commands::aprs_send`) exactly, so the
/// action is a verbatim pass-through, not a reinterpretation.
#[async_trait]
pub trait AprsService: Send + Sync {
    /// Returns the minted msgid on success.
    async fn send(&self, to: Option<String>, text: String) -> Result<String, String>;
}

/// The `radio.listen` (and `radio.connect`'s optional pre-flight dwell)
/// delegation seam. Samples RF energy on `rig` for `seconds`, returning a
/// normalized RMS in `0.0..=1.0` (linear, `sample / i16::MAX`, NOT dBFS —
/// see `radio.rs`'s module doc for why this trait picked a different unit
/// than `tuxlink_capture::slot::Slot::rms_dbfs`). `cancel` must be honored
/// promptly — a dwell is genuinely interruptible mid-sample.
#[async_trait]
pub trait ListenService: Send + Sync {
    async fn sample_rms(
        &self,
        rig: &str,
        seconds: u64,
        cancel: CancellationToken,
    ) -> Result<f32, String>;
}

/// Live CAT state — the wire shape `rig.read_state`/`rig.validate_preset`/
/// `rig.apply_preset` (cat.rs, plan Task 4b) output. Mirrors `tux_rig::RigStatus`
/// field-for-field (`freq_hz`, `mode`, `ptt`), NOT the fuller "power, meters"
/// wording spec §6's "Read radio state" row uses — `tux_rig::Rig` has no
/// power/meter query verb (see `cat.rs`'s module doc for the full recon); this
/// DTO reports exactly what the real seam can observe, honestly, rather than a
/// power/meters field that would always read `null`. `mode` is `None` when
/// `RigStatus.mode` itself is `None` (rigctld returned a token
/// `tux_rig::Mode::from_rigctl` doesn't recognize) — a real, reportable
/// condition, not an error swallowed into a default.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RigStateDto {
    pub freq_hz: u64,
    pub mode: Option<String>,
    pub ptt: bool,
}

/// The `rig.*` CAT verb delegation seam (spec §6 "Radio actions", cat.rs,
/// plan Task 4b). Every method is a FRESH, short-lived `rigctld` spawn (see
/// `cat.rs`'s module doc for why this deliberately does not attempt to reuse
/// a live `ModemSession`'s already-open rig handle) — narrow enough that
/// `rig.switch_vfo`/`rig.tune_atu` (spec-listed CAT verbs with NO real
/// `tux_rig::Rig` counterpart) never implement a fake by stretching this
/// trait; they return a verbatim unsupported error instead (cat.rs).
#[async_trait]
pub trait RigService: Send + Sync {
    /// Reads live CAT state (freq/mode/PTT).
    async fn read_state(&self) -> Result<RigStateDto, String>;

    /// Sets frequency (Hz) then mode (a `tux_rig::Mode::rigctl_str` token,
    /// e.g. `"PKTUSB"`). Does NOT itself re-read to verify — `rig.apply_preset`'s
    /// action layer does that as an explicit, separate [`Self::read_state`]
    /// call (spec §6's "read-state → validate → apply chain" is two visible
    /// steps, not one opaque one).
    async fn apply(&self, freq_hz: u64, mode: String) -> Result<(), String>;
}

// ============================================================================
// Registry construction
// ============================================================================

/// Constructor bag [`build_registry`] consumes. `arbiter` is shared by every
/// `needs_radio` action (radio.rs, cat.rs). The three radio.rs service seams
/// (`connect`/`aprs`/`listen`) plus cat.rs's `rig` are wired here; Tasks
/// 4c/4d add their OWN `Arc<dyn ...Service>` fields — additive only, so
/// existing `ActionDeps { arbiter, connect, aprs, listen, rig }`
/// construction call sites (Task 5's session mount, this module's own tests)
/// keep compiling as later tasks land. Suggested shape for the extension (do
/// not add these fields until the corresponding task actually lands — an
/// unused trait object field would be dead weight the registry never
/// registers):
///
/// - Task 4c (`data.rs`): `pub data: Arc<dyn DataService>,`
/// - Task 4d (`local.rs`): `pub local: Arc<dyn LocalService>,`
pub struct ActionDeps {
    pub arbiter: Arc<RadioArbiter>,
    pub connect: Arc<dyn ConnectService>,
    pub aprs: Arc<dyn AprsService>,
    pub listen: Arc<dyn ListenService>,
    pub rig: Arc<dyn RigService>,
}

/// Builds the action registry. Tasks 4a/4b register the radio.rs and cat.rs
/// actions; Tasks 4c/4d extend this function with their own
/// `registry.register(Arc::new(...))` calls, following the exact pattern
/// below — a `struct X { arbiter: Arc<RadioArbiter>, service: Arc<dyn
/// YService> }` implementing `tuxlink_routines::action::Action`.
pub fn build_registry(deps: ActionDeps) -> ActionRegistry {
    let mut registry = ActionRegistry::default();

    registry.register(Arc::new(radio::RadioConnect::new(
        deps.arbiter.clone(),
        deps.connect.clone(),
        deps.listen.clone(),
    )));
    registry.register(Arc::new(radio::RadioListen::new(
        deps.arbiter.clone(),
        deps.listen.clone(),
    )));
    registry.register(Arc::new(radio::RadioAprsSend::new(
        deps.arbiter.clone(),
        deps.aprs.clone(),
    )));

    registry.register(Arc::new(cat::RigReadState::new(
        deps.arbiter.clone(),
        deps.rig.clone(),
    )));
    registry.register(Arc::new(cat::RigValidatePreset::new(
        deps.arbiter.clone(),
        deps.rig.clone(),
    )));
    registry.register(Arc::new(cat::RigApplyPreset::new(
        deps.arbiter.clone(),
        deps.rig.clone(),
    )));
    registry.register(Arc::new(cat::RigSwitchVfo::new(deps.arbiter.clone())));
    registry.register(Arc::new(cat::RigTuneAtu::new(deps.arbiter.clone())));

    // ---- Extension point (Task 4c/4d) -----------------------------------
    // registry.register(Arc::new(data::SpaceWxWwv::new(...)));
    // registry.register(Arc::new(local::ComposeMessage::new(...)));
    // -----------------------------------------------------------------------

    registry
}

// ============================================================================
// Shared envelope-parsing helpers (used by every radio action in radio.rs;
// cat.rs/data.rs/local.rs may reuse the same envelope keys for their own
// `needs_radio` actions, since Task 5's engine glue injects these once, not
// per-action-family).
// ============================================================================

/// Rig identifier a radio action acquires the arbiter lease for, when the
/// step's params don't name one explicitly. v1 has exactly one physical
/// station; spec §9 ("Leases are per-radio: multi-rig stations run
/// concurrent routines on different rigs") already models multi-rig at the
/// arbiter layer (it is keyed by an arbitrary rig-id string), so this
/// default is a placeholder identifier, not a load-bearing single-rig
/// assumption baked into the arbiter itself.
pub(crate) const DEFAULT_RIG_ID: &str = "default";

/// Fallback acquire-wait timeout (seconds) when the engine glue's
/// `"_step_timeout_s"` envelope key (see [`step_timeout_from_params`]) is
/// absent — matches the plan's `EngineConfig::default_timeout_s` (Task 5).
const DEFAULT_TIMEOUT_S: u64 = 300;

/// Reads the `on_radio_busy` policy Task 5's engine glue injects into a
/// radio step's resolved params under `"_radio_busy_policy"`. The
/// `ActionStep.on_radio_busy` field itself is NOT part of `params` — the
/// `Action` trait (plan 1, locked) only hands `execute` a `params: Value` —
/// so this documented envelope key is how the policy crosses that
/// boundary (plan Task 4's explicit instruction). Defaults to
/// `BusyPolicy::Wait` (matches `BusyPolicy::default()`) when absent, so an
/// action driven directly by a unit test (no engine glue) behaves sanely
/// rather than panicking on a missing key.
pub(crate) fn busy_policy_from_params(params: &serde_json::Value) -> BusyPolicy {
    params
        .get("_radio_busy_policy")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Reads the acquire-wait timeout Task 5's engine glue SHOULD inject as
/// `"_step_timeout_s"` (the step's own `timeout_s`, so a `Wait`-policy
/// acquire never outlives the step it belongs to). Defaults to
/// [`DEFAULT_TIMEOUT_S`] when absent.
pub(crate) fn step_timeout_from_params(params: &serde_json::Value) -> Duration {
    let secs = params
        .get("_step_timeout_s")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(DEFAULT_TIMEOUT_S);
    Duration::from_secs(secs)
}

/// Builds the [`Holder::Run`] a radio action acquires the arbiter lease
/// under. `run_id`/`step` come from Task 5's engine glue via
/// `"_run_id"`/`"_step_id"` envelope keys — `execute(params, cancel)` has no
/// other way to learn which run/step is executing it (the `Action` port is
/// already locked by plan 1). Defaults let an action run standalone (unit
/// tests here, a future dry-run harness) without the full engine envelope:
/// `run_id` defaults to `"adhoc"`, `step` to the action's own catalog name —
/// still a meaningful `render_holder` string (`"run adhoc step
/// radio.connect"`), never a blank/placeholder that would read as a bug in
/// an arbiter status or journal line.
pub(crate) fn run_holder_from_params(params: &serde_json::Value, action_name: &str) -> Holder {
    let run_id = params
        .get("_run_id")
        .and_then(|v| v.as_str())
        .unwrap_or("adhoc")
        .to_string();
    let step = params
        .get("_step_id")
        .and_then(|v| v.as_str())
        .unwrap_or(action_name)
        .to_string();
    Holder::Run { run_id, step }
}

/// Reads the optional `"rig"` param a radio action accepts to target a
/// specific arbiter lease instead of [`DEFAULT_RIG_ID`]. No action's spec'd
/// param table names this today (v1 ships one physical rig) — this is
/// forward-compat plumbing for the multi-rig case spec §9 already reasons
/// about at the arbiter layer, kept intentionally cheap (a param read, not a
/// config lookup) since Task 4a cannot ground a real multi-rig config
/// surface.
pub(crate) fn rig_id_from_params(params: &serde_json::Value) -> String {
    params
        .get("rig")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_RIG_ID)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn busy_policy_defaults_to_wait_when_envelope_key_absent() {
        assert_eq!(busy_policy_from_params(&json!({})), BusyPolicy::Wait);
    }

    #[test]
    fn busy_policy_reads_injected_envelope_key() {
        assert_eq!(
            busy_policy_from_params(&json!({"_radio_busy_policy": "fail"})),
            BusyPolicy::Fail
        );
    }

    #[test]
    fn step_timeout_defaults_when_absent() {
        assert_eq!(
            step_timeout_from_params(&json!({})),
            Duration::from_secs(DEFAULT_TIMEOUT_S)
        );
    }

    #[test]
    fn step_timeout_reads_injected_envelope_key() {
        assert_eq!(
            step_timeout_from_params(&json!({"_step_timeout_s": 45})),
            Duration::from_secs(45)
        );
    }

    #[test]
    fn run_holder_defaults_to_adhoc_and_action_name() {
        assert_eq!(
            run_holder_from_params(&json!({}), "radio.connect"),
            Holder::Run {
                run_id: "adhoc".to_string(),
                step: "radio.connect".to_string(),
            }
        );
    }

    #[test]
    fn run_holder_reads_injected_envelope_keys() {
        assert_eq!(
            run_holder_from_params(
                &json!({"_run_id": "r42", "_step_id": "s7"}),
                "radio.connect"
            ),
            Holder::Run {
                run_id: "r42".to_string(),
                step: "s7".to_string(),
            }
        );
    }

    #[test]
    fn rig_id_defaults_and_reads_override() {
        assert_eq!(rig_id_from_params(&json!({})), DEFAULT_RIG_ID);
        assert_eq!(rig_id_from_params(&json!({"rig": "g90"})), "g90");
    }
}
