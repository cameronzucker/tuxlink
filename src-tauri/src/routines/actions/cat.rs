//! `rig.read_state` / `rig.validate_preset` / `rig.apply_preset` /
//! `rig.switch_vfo` / `rig.tune_atu` — spec §6 "Radio actions" CAT verbs
//! (plan Task 4b). Every impl here delegates through the narrow
//! [`super::RigService`] port declared in `actions/mod.rs`; NONE of this
//! file re-implements rigctld protocol handling — that lives in the
//! `tux-rig` crate, which [`MonolithRigService`] below wraps.
//!
//! ## Recon: the real CAT seam (plan Task 4b)
//!
//! - **`tux_rig::Rig`** (`src-tauri/tux-rig/src/lib.rs`) exposes exactly
//!   FOUR verbs: `set_freq`, `set_mode`, `ptt`, `read_status`. There is no
//!   VFO-select verb (Hamlib's `V`/`vfo_op`) and no ATU-tune verb (Hamlib's
//!   antenna-tuner start/tune command). `rig.switch_vfo` and `rig.tune_atu`
//!   below return a verbatim unsupported error naming this exact gap —
//!   per plan Task 4's explicit instruction, an unsupported CAT verb is
//!   DATA (something the validator/UI can flag), never a stub that quietly
//!   does nothing or a side-path fake. `rig.tune_atu` in particular could
//!   be "faked" by calling `Rig::ptt(true)` to key a raw carrier — the
//!   Rig trait literally has that verb — but doing so outside the
//!   ARQ/VARA transport stack (no timeout, no abort path, no busy-detector
//!   coordination) is exactly the kind of side-path transmitter keying
//!   this project's `Never VOX` / real-keyed-PTT discipline exists to
//!   prevent. It is not implemented.
//!
//! - **`ManagedRig`** (`tux-rig::managed`) is the concrete `Rig` impl: it
//!   spawns a `rigctld` subprocess bound to the configured CAT serial
//!   (`crate::modem_commands::rig_config_from(&cfg.rig)`, the SAME config
//!   source `tune_rig_for_connect`/`ardop_tune_rig` read), does one or more
//!   CAT round-trips, and — on `Drop` (or an explicit `release_serial()`)
//!   — kills + reaps the subprocess, releasing the serial.
//!   [`MonolithRigService`] below follows the EXACT shape of
//!   `modem_commands::ardop_tune_rig` (the existing "Tune…" / MCP
//!   `rig_tune` command): spawn a fresh `ManagedRig`, do the CAT op, let it
//!   drop. `read_state`/`apply` are each their OWN independent spawn — see
//!   "Design decision: no shared rig handle" below for why this doesn't
//!   try to be cleverer than that.
//!
//! - **`ModemSession`'s live rig handle is not reusable from here.**
//!   `crate::modem_status::ModemSession` DOES hold a live `ManagedRig` for
//!   a connected ARDOP/VARA session (`inner.rig`, the DRA-100 keep-serial
//!   path) — but the only public methods touching it are
//!   [`crate::modem_status::ModemSession::set_rig`] (install/replace,
//!   write-only) and the internal live-VFO poll thread, which opens its
//!   OWN independent `RigctldClient` TCP connection to the ALREADY-RUNNING
//!   `rigctld` process (reusing the process, not the `ManagedRig` struct)
//!   rather than touching `inner.rig` directly. There is no public
//!   accessor that would let a routine action borrow or query the live
//!   session's rig handle. Wiring that up (a `ModemSession::with_rig`-style
//!   reader, or exposing the session's rigctld host/port so this file could
//!   open its own `RigctldClient` — the SAME pattern the poll thread
//!   already uses — against the ALREADY-spawned process instead of
//!   spawning a second one) is future work; see "Known gap: serial
//!   contention with a live session" below.
//!
//! - **Design decision: no shared rig handle, no single combined
//!   apply-then-verify spawn.** [`RigService::apply`] and
//!   [`RigService::read_state`] are two SEPARATE `ManagedRig` spawns, not
//!   one connection reused for "tune, then read back." This costs one
//!   extra `rigctld` spawn+connect round-trip (`ManagedRig::spawn`'s
//!   `CONNECT_TIMEOUT` poll loop, typically well under a second) on
//!   `rig.apply_preset`, in exchange for: (a) each `RigService` method
//!   being independently unit-testable with a narrow fake (a
//!   spawn-and-tune fake never needs to also implement read-back), and (b)
//!   matching spec §6's own framing of "read-state → validate → apply" as
//!   the supported CAT pre-flight *pattern* — three visibly separate
//!   operations, not one opaque one. `ManagedRig::Drop` kills + reaps its
//!   subprocess synchronously before returning, so by the time the second
//!   spawn's `Command::new(..).spawn()` runs, the OS has already released
//!   the serial fd from the first — this is not a race in practice, just
//!   two sequential round-trips instead of one.
//!
//! - **Known gap: serial contention with a live session.** If a routine's
//!   `rig.*` action runs while the operator has an ARDOP/VARA session open
//!   on the SAME CAT serial (DRA-100 keep-serial path, `ModemSession.rig`
//!   holding it), [`MonolithRigService`]'s fresh `ManagedRig::spawn` will
//!   almost certainly fail — most serial devices refuse a second exclusive
//!   open — surfacing a `RigError::Spawn`/`RigError::Io` VERBATIM as this
//!   action's `StepError::Action.cause`. This is honest data (a real,
//!   diagnosable "the serial is busy" error), not a silent wrong answer or
//!   a hang; it is not specially detected or pre-empted here because doing
//!   so needs the `ModemSession`-reuse accessor described above, which
//!   does not exist yet. The `RadioArbiter` lease this file DOES take
//!   prevents CONCURRENT routine `rig.*`/`radio.*` steps from
//!   racing each other (spec §9); it does not yet know about live
//!   INTERACTIVE sessions outside a routine run — `RadioArbiter::interactive_acquire`/
//!   `interactive_release`, wired into the operator's own connect/disconnect
//!   paths, is explicitly deferred to plan Task 5 (`arbiter.rs`'s own doc
//!   comment says the same).
//!
//! - **`rig.validate_preset`'s diff never compares `power_w`/`atu`.**
//!   `tux_rig::RigStatus` (and this file's [`super::RigStateDto`] mirror
//!   of it) carries only `freq_hz`/`mode`/`ptt` — there is no power or ATU
//!   READ verb in `tux_rig::Rig` at all, so there is no live value to
//!   compare a preset's `powerW`/`atu` fields against. [`PresetParam`]
//!   below deliberately does not even deserialize those fields (serde
//!   ignores unknown JSON keys by default) — the diff computation has
//!   nothing to reference them with, which is the literal, honest form of
//!   "skip comparison" the plan's test contract asks for: whether a
//!   preset's optional field is present or absent, there is no seam to
//!   compare it against, so it never appears in `diff`.
//!
//! - **Preset field names are camelCase on the wire, not the plan's
//!   informal `frequency_hz`/`power_w` prose.** `routines::presets::RadioPreset`
//!   (the entity `@preset:<name>` resolves to, per `resolver.rs`'s
//!   `serde_json::to_value(&preset)`) carries `#[serde(rename_all =
//!   "camelCase")]` — the resolved JSON object a routine step's `preset`
//!   param actually receives has keys `frequencyHz`/`mode`/`powerW`/`atu`,
//!   not `frequency_hz`/`power_w`. [`PresetParam`] mirrors that real wire
//!   shape (`#[serde(rename_all = "camelCase")]`), verified against
//!   `presets.rs` directly rather than the plan doc's shorthand English.
//!
//! Plan: `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 4. Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6, §9.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor};
use tuxlink_routines::error::StepError;

use crate::routines::arbiter::RadioArbiter;

use super::{
    busy_policy_from_params, rig_id_from_params, run_holder_from_params, step_timeout_from_params,
    RigService, RigStateDto,
};

/// Default frequency-match tolerance (Hz) for `rig.validate_preset` when the
/// step's `tolerance_hz` param is absent — spec §6's "structured diff"
/// wording doesn't pin a number; this matches the plan Task 4b default.
const DEFAULT_TOLERANCE_HZ: u64 = 50;

/// Frequency-match tolerance (Hz) `rig.apply_preset` uses for its own
/// post-apply re-read verification. Same value as
/// [`DEFAULT_TOLERANCE_HZ`] — both exist to absorb a rigctld readback that
/// rounds to the nearest few Hz, not a real drift.
const APPLY_VERIFY_TOLERANCE_HZ: u64 = 50;

/// The resolved `@preset:` object shape a `preset` param actually carries —
/// see this module's doc comment ("Preset field names are camelCase on the
/// wire") for why this is NOT the plan doc's informal snake_case prose.
/// Deliberately does NOT declare `name`/`powerW`/`atu` — serde ignores
/// unrecognized JSON keys by default, and this file has no live seam to
/// compare those fields against (see "rig.validate_preset's diff never
/// compares power_w/atu" above), so there is nothing gained by parsing them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetParam {
    frequency_hz: u64,
    mode: String,
}

// ============================================================================
// rig.read_state
// ============================================================================

const RIG_READ_STATE: &str = "rig.read_state";

/// `rig.read_state` — live CAT state (freq/mode/PTT) via [`RigService`].
/// `needs_radio: true`, `transmits: false`.
pub struct RigReadState {
    arbiter: Arc<RadioArbiter>,
    rig: Arc<dyn RigService>,
}

impl RigReadState {
    pub fn new(arbiter: Arc<RadioArbiter>, rig: Arc<dyn RigService>) -> Self {
        Self { arbiter, rig }
    }
}

#[async_trait]
impl Action for RigReadState {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RIG_READ_STATE,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let rig_id = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RIG_READ_STATE);

        let _lease = self
            .arbiter
            .acquire(&rig_id, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RIG_READ_STATE.to_string(),
                cause: e.to_string(),
            })?;

        let state = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.rig.read_state() => res,
        }
        .map_err(|cause| StepError::Action {
            action: RIG_READ_STATE.to_string(),
            cause,
        })?;

        serde_json::to_value(&state).map_err(|e| StepError::Action {
            action: RIG_READ_STATE.to_string(),
            cause: format!("state serialize: {e}"),
        })
        // `_lease` drops here — released after the read completes.
    }
}

// ============================================================================
// rig.validate_preset
// ============================================================================

const RIG_VALIDATE_PRESET: &str = "rig.validate_preset";

#[derive(Debug, Deserialize)]
struct ValidatePresetParams {
    preset: PresetParam,
    #[serde(default = "default_tolerance_hz")]
    tolerance_hz: u64,
}

fn default_tolerance_hz() -> u64 {
    DEFAULT_TOLERANCE_HZ
}

/// Structured comparison of live CAT state against `preset`. `mode` compares
/// case-insensitively (rigctld tokens are conventionally upper-case, but a
/// hand-authored preset shouldn't fail validation over casing alone).
///
/// Returns a tuple of (diff, compared_fields, skipped_fields):
/// - `diff`: JSON object with mismatches (empty if all compared fields match)
/// - `compared_fields`: field names that were compared against live state
///   (e.g., `["frequency_hz", "mode"]`)
/// - `skipped_fields`: field names present in the preset but not comparable
///   (e.g., `["power_w"]` if the preset declared powerW but there is no live
///   power read verb in tux_rig::Rig)
fn diff_against(
    state: &RigStateDto,
    preset: &PresetParam,
    preset_raw: &Value,
    tolerance_hz: u64,
) -> (Value, Vec<String>, Vec<String>) {
    let mut diff = serde_json::Map::new();
    let mut compared = vec![];
    let mut skipped = vec![];

    // Always compare frequency_hz (present in PresetParam)
    let freq_delta = (i128::from(state.freq_hz) - i128::from(preset.frequency_hz)).abs();
    if freq_delta > i128::from(tolerance_hz) {
        diff.insert(
            "frequency_hz".to_string(),
            json!({ "expected": preset.frequency_hz, "actual": state.freq_hz }),
        );
    }
    compared.push("frequency_hz".to_string());

    // Always compare mode (present in PresetParam)
    let mode_matches = state
        .mode
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case(preset.mode.trim()));
    if !mode_matches {
        diff.insert(
            "mode".to_string(),
            json!({ "expected": preset.mode, "actual": state.mode }),
        );
    }
    compared.push("mode".to_string());

    // Track skipped fields from the raw preset that we cannot compare
    // (no live read verb exists in tux_rig::Rig for these).
    if let Some(preset_obj) = preset_raw.as_object() {
        if preset_obj.contains_key("powerW") {
            skipped.push("power_w".to_string());
        }
        if preset_obj.contains_key("atu") {
            skipped.push("atu".to_string());
        }
    }

    (Value::Object(diff), compared, skipped)
}

/// `rig.validate_preset` — compare live CAT state against a resolved
/// `@preset:` object; outputs `{"matches": bool, "diff": {...}, "compared":
/// [...], "skipped": [...]}` (spec §6 structured diff with honest-scope
/// metadata). `needs_radio: true`, `transmits: false`.
///
/// Output keys:
/// - `matches`: bool, true if all compared fields match (within tolerance)
/// - `diff`: object, empty if matches; contains mismatches keyed by field name
/// - `compared`: array of field names compared against live state
///   (e.g., `["frequency_hz", "mode"]`)
/// - `skipped`: array of preset fields present but not comparable due to no
///   live read verb (e.g., `["power_w"]` if preset declared powerW)
pub struct RigValidatePreset {
    arbiter: Arc<RadioArbiter>,
    rig: Arc<dyn RigService>,
}

impl RigValidatePreset {
    pub fn new(arbiter: Arc<RadioArbiter>, rig: Arc<dyn RigService>) -> Self {
        Self { arbiter, rig }
    }
}

#[async_trait]
impl Action for RigValidatePreset {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RIG_VALIDATE_PRESET,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: ValidatePresetParams =
            serde_json::from_value(params.clone()).map_err(|e| StepError::Action {
                action: RIG_VALIDATE_PRESET.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let rig_id = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RIG_VALIDATE_PRESET);

        let _lease = self
            .arbiter
            .acquire(&rig_id, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RIG_VALIDATE_PRESET.to_string(),
                cause: e.to_string(),
            })?;

        let state = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.rig.read_state() => res,
        }
        .map_err(|cause| StepError::Action {
            action: RIG_VALIDATE_PRESET.to_string(),
            cause,
        })?;

        let preset_raw = params.get("preset").cloned().unwrap_or(Value::Null);
        let (diff, compared, skipped) =
            diff_against(&state, &parsed.preset, &preset_raw, parsed.tolerance_hz);
        let matches = diff.as_object().is_some_and(|m| m.is_empty());

        Ok(json!({
            "matches": matches,
            "diff": diff,
            "compared": compared,
            "skipped": skipped
        }))
        // `_lease` drops here — released after the read completes.
    }
}

// ============================================================================
// rig.apply_preset
// ============================================================================

const RIG_APPLY_PRESET: &str = "rig.apply_preset";

#[derive(Debug, Deserialize)]
struct ApplyPresetParams {
    preset: PresetParam,
    #[serde(default = "default_apply_verify_tolerance_hz")]
    tolerance_hz: u64,
}

fn default_apply_verify_tolerance_hz() -> u64 {
    APPLY_VERIFY_TOLERANCE_HZ
}

/// `rig.apply_preset` — sets freq + mode from a resolved `@preset:` object,
/// then re-reads to verify (spec §6's "read-state → validate → apply"
/// pre-flight pattern's write counterpart). Outputs the post-apply state on
/// success. `needs_radio: true`, `transmits: false` (a CAT frequency/mode
/// set is not itself a transmission).
///
/// Parameters:
/// - `preset`: the preset object to apply (required)
/// - `tolerance_hz`: frequency-match tolerance (Hz) for post-apply verification
///   (optional, defaults to `APPLY_VERIFY_TOLERANCE_HZ`)
///
/// Two distinct failure shapes, both surfaced as `StepError::Action`:
/// - The re-read call ITSELF errors (rigctld died, socket reset, …) — the
///   underlying [`RigService::read_state`] error is passed VERBATIM (Global
///   Constraints), unmodified, as `cause`.
/// - The re-read SUCCEEDS but the rig now reports a state outside the
///   `tolerance_hz` parameter/a different mode than what was applied —
///   there is no underlying system error to pass verbatim here (the CAT
///   calls all returned `Ok`), so this is this action's OWN diagnostic
///   message, naming exactly what was requested vs. what the rig reports.
pub struct RigApplyPreset {
    arbiter: Arc<RadioArbiter>,
    rig: Arc<dyn RigService>,
}

impl RigApplyPreset {
    pub fn new(arbiter: Arc<RadioArbiter>, rig: Arc<dyn RigService>) -> Self {
        Self { arbiter, rig }
    }
}

#[async_trait]
impl Action for RigApplyPreset {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RIG_APPLY_PRESET,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: ApplyPresetParams =
            serde_json::from_value(params.clone()).map_err(|e| StepError::Action {
                action: RIG_APPLY_PRESET.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        let rig_id = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RIG_APPLY_PRESET);

        let _lease = self
            .arbiter
            .acquire(&rig_id, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RIG_APPLY_PRESET.to_string(),
                cause: e.to_string(),
            })?;

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.rig.apply(parsed.preset.frequency_hz, parsed.preset.mode.clone()) => res,
        }
        .map_err(|cause| StepError::Action {
            action: RIG_APPLY_PRESET.to_string(),
            cause,
        })?;

        let post = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.rig.read_state() => res,
        }
        .map_err(|cause| StepError::Action {
            action: RIG_APPLY_PRESET.to_string(),
            cause,
        })?;

        let freq_delta = (i128::from(post.freq_hz) - i128::from(parsed.preset.frequency_hz)).abs();
        let mode_matches = post
            .mode
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case(parsed.preset.mode.trim()));
        if freq_delta > i128::from(parsed.tolerance_hz) || !mode_matches {
            return Err(StepError::Action {
                action: RIG_APPLY_PRESET.to_string(),
                cause: format!(
                    "apply_preset verification mismatch: requested {req_freq}Hz/{req_mode}, \
                     rig now reports {actual_freq}Hz/{actual_mode:?}",
                    req_freq = parsed.preset.frequency_hz,
                    req_mode = parsed.preset.mode,
                    actual_freq = post.freq_hz,
                    actual_mode = post.mode,
                ),
            });
        }

        serde_json::to_value(&post).map_err(|e| StepError::Action {
            action: RIG_APPLY_PRESET.to_string(),
            cause: format!("state serialize: {e}"),
        })
        // `_lease` drops here — released after apply + verify completes.
    }
}

// ============================================================================
// rig.switch_vfo — UNSUPPORTED (no real tux-rig seam; see module doc)
// ============================================================================

const RIG_SWITCH_VFO: &str = "rig.switch_vfo";

/// Verbatim unsupported-verb message — names the exact missing seam per
/// plan Task 4's "an unsupported CAT verb is data, not a stub" instruction.
const RIG_SWITCH_VFO_UNSUPPORTED: &str =
    "rig.switch_vfo: tux-rig's Rig trait (src-tauri/tux-rig/src/lib.rs) exposes only \
     set_freq/set_mode/ptt/read_status — there is no VFO-select verb (e.g. Hamlib's \
     `V`/`vfo_op` set-VFO command). This action is unimplemented until tux-rig grows that \
     seam; it does not fake VFO switching by any other means.";

/// `rig.switch_vfo` — spec §6 lists this CAT verb, but `tux_rig::Rig` has no
/// VFO-select verb. Acquires the arbiter lease (per plan Task 4's uniform
/// "all five actions take an RAII lease" instruction — the descriptor still
/// declares `needs_radio: true`, so the arbiter/validator treat it as a real
/// radio-seizing step even though execution always errors) then returns the
/// verbatim unsupported error. `needs_radio: true`, `transmits: false`.
pub struct RigSwitchVfo {
    arbiter: Arc<RadioArbiter>,
}

impl RigSwitchVfo {
    pub fn new(arbiter: Arc<RadioArbiter>) -> Self {
        Self { arbiter }
    }
}

#[async_trait]
impl Action for RigSwitchVfo {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RIG_SWITCH_VFO,
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let rig_id = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RIG_SWITCH_VFO);

        let _lease = self
            .arbiter
            .acquire(&rig_id, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RIG_SWITCH_VFO.to_string(),
                cause: e.to_string(),
            })?;

        Err(StepError::Action {
            action: RIG_SWITCH_VFO.to_string(),
            cause: RIG_SWITCH_VFO_UNSUPPORTED.to_string(),
        })
        // `_lease` drops here — released immediately (nothing to hold it for).
    }
}

// ============================================================================
// rig.tune_atu — UNSUPPORTED (no real tux-rig seam; see module doc)
// ============================================================================

const RIG_TUNE_ATU: &str = "rig.tune_atu";

const RIG_TUNE_ATU_UNSUPPORTED: &str =
    "rig.tune_atu: tux-rig's Rig trait (src-tauri/tux-rig/src/lib.rs) has no ATU-tune verb \
     (e.g. Hamlib's antenna-tuner start/tune command). Driving this via the trait's raw \
     ptt(true) to key a manual carrier — outside the ARQ/VARA transport stack's timeout, \
     abort path, and busy-detector coordination — is exactly the side-path transmitter \
     keying this project's real-keyed-PTT discipline exists to prevent, so it is not done. \
     This action is unimplemented until tux-rig grows a real ATU-tune seam.";

/// `rig.tune_atu` — spec §6 lists this CAT verb (`transmits: true` — it
/// keys a carrier), but `tux_rig::Rig` has no ATU-tune verb, and the only
/// way to fake one (raw `ptt(true)`) is exactly the side-path carrier-keying
/// this project refuses to freelance (see module doc + the constant above).
/// Acquires the arbiter lease for the same uniformity reason as
/// [`RigSwitchVfo`], then returns the verbatim unsupported error.
/// `needs_radio: true`, `transmits: true`.
pub struct RigTuneAtu {
    arbiter: Arc<RadioArbiter>,
}

impl RigTuneAtu {
    pub fn new(arbiter: Arc<RadioArbiter>) -> Self {
        Self { arbiter }
    }
}

#[async_trait]
impl Action for RigTuneAtu {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            name: RIG_TUNE_ATU,
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let rig_id = rig_id_from_params(&params);
        let policy = busy_policy_from_params(&params);
        let timeout = step_timeout_from_params(&params);
        let holder = run_holder_from_params(&params, RIG_TUNE_ATU);

        let _lease = self
            .arbiter
            .acquire(&rig_id, holder, policy, timeout, &cancel)
            .await
            .map_err(|e| StepError::Action {
                action: RIG_TUNE_ATU.to_string(),
                cause: e.to_string(),
            })?;

        Err(StepError::Action {
            action: RIG_TUNE_ATU.to_string(),
            cause: RIG_TUNE_ATU_UNSUPPORTED.to_string(),
        })
        // `_lease` drops here — released immediately (nothing to hold it for).
    }
}

// ============================================================================
// Real seam adapter — MonolithRigService. Follows the exact shape of
// `modem_commands::ardop_tune_rig` (spawn a fresh `ManagedRig`, do the CAT
// op, let it drop) — see this module's doc comment's "Design decision: no
// shared rig handle" for why `read_state`/`apply` are independent spawns.
// Blocking `ManagedRig` I/O runs inside `spawn_blocking`, mirroring
// `radio.rs`'s `MonolithListenService` (blocking ALSA I/O off the async
// executor).
// ============================================================================

fn rig_status_to_dto(status: tux_rig::RigStatus) -> RigStateDto {
    RigStateDto {
        freq_hz: status.freq_hz,
        mode: status.mode.map(|m| m.rigctl_str().to_string()),
        ptt: status.ptt,
    }
}

/// Real [`RigService`]: spawns a fresh `tux_rig::ManagedRig` per call against
/// the operator-configured rig (`crate::modem_commands::rig_config_from(&cfg.rig)`).
/// **Not unit-tested past its own pure conversion** ([`rig_status_to_dto`]) —
/// spawning `rigctld` needs a real (or fake) binary + serial device, matching
/// `radio.rs`'s `MonolithConnectService`/`MonolithListenService` precedent:
/// CI-compile-checked + operator-smoke territory, action-layer logic covered
/// by the fakes in this file's tests instead.
#[derive(Default)]
pub struct MonolithRigService;

impl MonolithRigService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RigService for MonolithRigService {
    async fn read_state(&self) -> Result<RigStateDto, String> {
        let handle = tokio::task::spawn_blocking(|| -> Result<RigStateDto, String> {
            let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
            let rc = crate::modem_commands::rig_config_from(&cfg.rig).ok_or_else(|| {
                "rig control not configured — set the rig model + CAT serial".to_string()
            })?;
            let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
            let status = rig.status().map_err(|e| e.to_string())?;
            Ok(rig_status_to_dto(status))
            // `rig` drops here — kills + reaps rigctld, releasing the serial.
        });
        handle
            .await
            .map_err(|e| format!("rig read task panicked: {e}"))?
    }

    async fn apply(&self, freq_hz: u64, mode: String) -> Result<(), String> {
        let handle = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
            let rc = crate::modem_commands::rig_config_from(&cfg.rig).ok_or_else(|| {
                "rig control not configured — set the rig model + CAT serial".to_string()
            })?;
            let parsed_mode = tux_rig::Mode::from_rigctl(&mode).ok_or_else(|| {
                format!(
                    "unknown CAT mode token {mode:?} — expected one of \
                     PKTUSB/USB/LSB/PKTLSB/USB-D/LSB-D"
                )
            })?;
            let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
            rig.tune(freq_hz, parsed_mode).map_err(|e| e.to_string())?;
            Ok(())
            // `rig` drops here — kills + reaps rigctld, releasing the serial.
        });
        handle
            .await
            .map_err(|e| format!("rig apply task panicked: {e}"))?
    }
}

// ============================================================================
// Tests — trait fakes, no hardware/tauri. Per plan Task 4's test contract:
// RigService fake per verb; validate_preset diff correctness (within/outside
// tolerance, missing optional fields skip comparison); apply re-read
// verification failure = verbatim error; unsupported verbs produce the
// documented error.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    thread_local! {
        static TEST_CLOCK: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }
    fn test_now() -> i64 {
        TEST_CLOCK.with(|c| c.get())
    }

    fn arbiter() -> Arc<RadioArbiter> {
        Arc::new(RadioArbiter::new(test_now))
    }

    // ---- FakeRigService ---------------------------------------------------

    #[allow(clippy::type_complexity)] // boxed FnMut fields are a test-double idiom
    struct FakeRigService {
        read_result: Mutex<Box<dyn FnMut() -> Result<RigStateDto, String> + Send>>,
        apply_result: Mutex<Box<dyn FnMut(u64, String) -> Result<(), String> + Send>>,
    }

    impl FakeRigService {
        fn always_reads(state: RigStateDto) -> Self {
            Self {
                read_result: Mutex::new(Box::new(move || Ok(state.clone()))),
                apply_result: Mutex::new(Box::new(|_hz, _mode| {
                    panic!("apply not expected in this test")
                })),
            }
        }

        fn read_fails(msg: &'static str) -> Self {
            Self {
                read_result: Mutex::new(Box::new(move || Err(msg.to_string()))),
                apply_result: Mutex::new(Box::new(|_hz, _mode| {
                    panic!("apply not expected in this test")
                })),
            }
        }

        /// Apply always succeeds; every subsequent read returns `post`.
        fn apply_ok_then_reads(post: RigStateDto) -> Self {
            Self {
                read_result: Mutex::new(Box::new(move || Ok(post.clone()))),
                apply_result: Mutex::new(Box::new(|_hz, _mode| Ok(()))),
            }
        }

        fn apply_ok_then_read_fails(msg: &'static str) -> Self {
            Self {
                read_result: Mutex::new(Box::new(move || Err(msg.to_string()))),
                apply_result: Mutex::new(Box::new(|_hz, _mode| Ok(()))),
            }
        }

        fn apply_fails(msg: &'static str) -> Self {
            Self {
                read_result: Mutex::new(Box::new(|| {
                    panic!("read_state not expected in this test")
                })),
                apply_result: Mutex::new(Box::new(move |_hz, _mode| Err(msg.to_string()))),
            }
        }
    }

    #[async_trait]
    impl RigService for FakeRigService {
        async fn read_state(&self) -> Result<RigStateDto, String> {
            (*self.read_result.lock().unwrap())()
        }

        async fn apply(&self, freq_hz: u64, mode: String) -> Result<(), String> {
            (*self.apply_result.lock().unwrap())(freq_hz, mode)
        }
    }

    fn state(freq_hz: u64, mode: Option<&str>, ptt: bool) -> RigStateDto {
        RigStateDto {
            freq_hz,
            mode: mode.map(str::to_string),
            ptt,
        }
    }

    // ======================================================================
    // rig.read_state
    // ======================================================================

    #[tokio::test]
    async fn read_state_happy_path_output_shape() {
        let action = RigReadState::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_102_000,
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out["freqHz"], json!(7_102_000u64));
        assert_eq!(out["mode"], json!("PKTUSB"));
        assert_eq!(out["ptt"], json!(false));
    }

    #[tokio::test]
    async fn read_state_verbatim_error_passthrough() {
        let action = RigReadState::new(
            arbiter(),
            Arc::new(FakeRigService::read_fails(
                "rigctld spawn failed: no such file",
            )),
        );
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("read failure must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.read_state");
                assert_eq!(cause, "rigctld spawn failed: no such file");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_state_lease_is_held_during_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        struct ObservingRig {
            arbiter: Arc<RadioArbiter>,
            rig: &'static str,
            observed: Arc<Mutex<Option<bool>>>,
        }
        #[async_trait]
        impl RigService for ObservingRig {
            async fn read_state(&self) -> Result<RigStateDto, String> {
                *self.observed.lock().unwrap() = Some(self.arbiter.status(self.rig).is_some());
                Ok(state(7_102_000, Some("PKTUSB"), false))
            }
            async fn apply(&self, _freq_hz: u64, _mode: String) -> Result<(), String> {
                panic!("apply not expected in this test")
            }
        }
        let observed = Arc::new(Mutex::new(None));
        let action = RigReadState::new(
            arb.clone(),
            Arc::new(ObservingRig {
                arbiter: arb.clone(),
                rig,
                observed: observed.clone(),
            }),
        );
        action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(*observed.lock().unwrap(), Some(true));
        assert!(arb.status(rig).is_none(), "lease released after read");
    }

    // ======================================================================
    // rig.validate_preset
    // ======================================================================

    #[tokio::test]
    async fn validate_preset_within_tolerance_and_matching_mode_matches() {
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_102_030,
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["matches"], json!(true));
        assert_eq!(out["diff"], json!({}));
        assert_eq!(out["compared"], json!(["frequency_hz", "mode"]));
        assert_eq!(out["skipped"], json!([]));
    }

    #[tokio::test]
    async fn validate_preset_outside_tolerance_reports_frequency_diff() {
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_103_000,
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({
                    "preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"},
                    "tolerance_hz": 50
                }),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["matches"], json!(false));
        assert_eq!(
            out["diff"]["frequency_hz"],
            json!({"expected": 7_102_000, "actual": 7_103_000})
        );
        assert!(
            out["diff"].get("mode").is_none(),
            "mode matched, must not appear in diff"
        );
        assert_eq!(out["compared"], json!(["frequency_hz", "mode"]));
        assert_eq!(out["skipped"], json!([]));
    }

    #[tokio::test]
    async fn validate_preset_mode_mismatch_reports_mode_diff() {
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_102_000,
                Some("USB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["matches"], json!(false));
        assert_eq!(
            out["diff"]["mode"],
            json!({"expected": "PKTUSB", "actual": "USB"})
        );
        assert_eq!(out["compared"], json!(["frequency_hz", "mode"]));
        assert_eq!(out["skipped"], json!([]));
    }

    #[tokio::test]
    async fn validate_preset_missing_live_mode_is_a_mismatch_not_a_panic() {
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(7_102_000, None, false))),
        );
        let out = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["matches"], json!(false));
        assert_eq!(
            out["diff"]["mode"],
            json!({"expected": "PKTUSB", "actual": null})
        );
        assert_eq!(out["compared"], json!(["frequency_hz", "mode"]));
        assert_eq!(out["skipped"], json!([]));
    }

    #[tokio::test]
    async fn validate_preset_power_and_atu_fields_never_appear_in_diff() {
        // A full resolved @preset object, powerW/atu present — must be
        // accepted (ignored, not a param error) and never compared, since
        // there is no live power/ATU read seam (module doc). They should
        // appear in the "skipped" array instead.
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_102_000,
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({
                    "preset": {
                        "name": "40m-ardop",
                        "frequencyHz": 7_102_000,
                        "mode": "PKTUSB",
                        "powerW": 100,
                        "atu": true
                    }
                }),
                CancellationToken::new(),
            )
            .await
            .expect("powerW/atu present must not be a param error");
        assert_eq!(out["matches"], json!(true));
        assert!(out["diff"].get("power_w").is_none());
        assert!(out["diff"].get("atu").is_none());
        assert_eq!(out["compared"], json!(["frequency_hz", "mode"]));
        // Check that both skipped fields are present (order not guaranteed)
        let skipped = out["skipped"].as_array().expect("skipped must be array");
        assert_eq!(skipped.len(), 2, "must have exactly 2 skipped fields");
        assert!(skipped.iter().any(|v| v == "power_w"));
        assert!(skipped.iter().any(|v| v == "atu"));
    }

    #[tokio::test]
    async fn validate_preset_invalid_params_is_a_step_error() {
        let action = RigValidatePreset::new(
            arbiter(),
            Arc::new(FakeRigService::always_reads(state(
                7_102_000,
                Some("PKTUSB"),
                false,
            ))),
        );
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("missing preset must error");
        assert!(matches!(err, StepError::Action { .. }));
    }

    // ======================================================================
    // rig.apply_preset
    // ======================================================================

    #[tokio::test]
    async fn apply_preset_happy_path_outputs_post_apply_state() {
        let action = RigApplyPreset::new(
            arbiter(),
            Arc::new(FakeRigService::apply_ok_then_reads(state(
                7_102_010,
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(out["freqHz"], json!(7_102_010u64));
        assert_eq!(out["mode"], json!("PKTUSB"));
    }

    #[tokio::test]
    async fn apply_preset_verify_reread_failure_is_verbatim_error() {
        let action = RigApplyPreset::new(
            arbiter(),
            Arc::new(FakeRigService::apply_ok_then_read_fails(
                "rig I/O error: connection reset",
            )),
        );
        let err = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .expect_err("re-read failure must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.apply_preset");
                assert_eq!(cause, "rig I/O error: connection reset");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_preset_value_mismatch_after_successful_reread_is_a_step_error() {
        // Apply "succeeds" and the re-read call itself succeeds, but the
        // rig now reports a frequency far outside tolerance — a distinct
        // failure shape from a re-read call error (see this action's doc).
        let action = RigApplyPreset::new(
            arbiter(),
            Arc::new(FakeRigService::apply_ok_then_reads(state(
                7_200_000,
                Some("PKTUSB"),
                false,
            ))),
        );
        let err = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .expect_err("value mismatch must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.apply_preset");
                assert!(
                    cause.contains("verification mismatch"),
                    "cause must describe the mismatch: {cause}"
                );
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_preset_custom_tolerance_hz_used_for_verification() {
        // Apply succeeds, re-read reports freq within a large custom tolerance
        // but outside the default tolerance — verify that the custom tolerance
        // (500 Hz) is used for verification, not the hardcoded default (50 Hz).
        let action = RigApplyPreset::new(
            arbiter(),
            Arc::new(FakeRigService::apply_ok_then_reads(state(
                7_102_400, // 400 Hz off from preset — inside 500 Hz tolerance,
                // outside 50 Hz default tolerance
                Some("PKTUSB"),
                false,
            ))),
        );
        let out = action
            .execute(
                json!({
                    "preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"},
                    "tolerance_hz": 500
                }),
                CancellationToken::new(),
            )
            .await
            .expect("should succeed with custom tolerance_hz=500");
        assert_eq!(out["freqHz"], json!(7_102_400u64));
        assert_eq!(out["mode"], json!("PKTUSB"));
    }

    #[tokio::test]
    async fn apply_preset_apply_call_failure_is_verbatim_error() {
        let action = RigApplyPreset::new(
            arbiter(),
            Arc::new(FakeRigService::apply_fails("CAT tune failed: RPRT -1")),
        );
        let err = action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .expect_err("apply failure must surface");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.apply_preset");
                assert_eq!(cause, "CAT tune failed: RPRT -1");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_preset_lease_is_held_during_and_released_after() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        struct ObservingRig {
            arbiter: Arc<RadioArbiter>,
            rig: &'static str,
            observed_apply: Arc<Mutex<Option<bool>>>,
        }
        #[async_trait]
        impl RigService for ObservingRig {
            async fn read_state(&self) -> Result<RigStateDto, String> {
                Ok(state(7_102_000, Some("PKTUSB"), false))
            }
            async fn apply(&self, _freq_hz: u64, _mode: String) -> Result<(), String> {
                *self.observed_apply.lock().unwrap() =
                    Some(self.arbiter.status(self.rig).is_some());
                Ok(())
            }
        }
        let observed_apply = Arc::new(Mutex::new(None));
        let action = RigApplyPreset::new(
            arb.clone(),
            Arc::new(ObservingRig {
                arbiter: arb.clone(),
                rig,
                observed_apply: observed_apply.clone(),
            }),
        );
        action
            .execute(
                json!({"preset": {"frequencyHz": 7_102_000, "mode": "PKTUSB"}}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(*observed_apply.lock().unwrap(), Some(true));
        assert!(
            arb.status(rig).is_none(),
            "lease released after apply+verify"
        );
    }

    // ======================================================================
    // rig.switch_vfo — unsupported
    // ======================================================================

    #[tokio::test]
    async fn switch_vfo_returns_documented_unsupported_error() {
        let action = RigSwitchVfo::new(arbiter());
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("switch_vfo has no real seam — must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.switch_vfo");
                assert_eq!(cause, RIG_SWITCH_VFO_UNSUPPORTED);
                assert!(cause.contains("Rig trait"), "must name the missing seam");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn switch_vfo_lease_is_acquired_and_released_even_though_unsupported() {
        let arb = arbiter();
        let rig = crate::routines::actions::DEFAULT_RIG_ID;
        let action = RigSwitchVfo::new(arb.clone());
        assert!(arb.status(rig).is_none());
        let _ = action
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap_err();
        assert!(
            arb.status(rig).is_none(),
            "lease must be released even on the unsupported-verb error path"
        );
    }

    // ======================================================================
    // rig.tune_atu — unsupported
    // ======================================================================

    #[tokio::test]
    async fn tune_atu_returns_documented_unsupported_error() {
        let action = RigTuneAtu::new(arbiter());
        let err = action
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("tune_atu has no real seam — must error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "rig.tune_atu");
                assert_eq!(cause, RIG_TUNE_ATU_UNSUPPORTED);
                assert!(
                    cause.contains("ptt(true)"),
                    "must name the refused side path"
                );
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tune_atu_descriptor_flags_transmits_true() {
        let action = RigTuneAtu::new(arbiter());
        let d = action.descriptor();
        assert!(d.needs_radio);
        assert!(
            d.transmits,
            "tune_atu keys a carrier per spec §6 — transmits must be true"
        );
    }

    #[tokio::test]
    async fn switch_vfo_descriptor_flags_transmits_false() {
        let action = RigSwitchVfo::new(arbiter());
        let d = action.descriptor();
        assert!(d.needs_radio);
        assert!(!d.transmits);
    }

    // ======================================================================
    // MonolithRigService's pure conversion
    // ======================================================================

    #[test]
    fn rig_status_to_dto_maps_all_fields() {
        let status = tux_rig::RigStatus {
            freq_hz: 7_102_000,
            mode: Some(tux_rig::Mode::PktUsb),
            ptt: true,
        };
        let dto = rig_status_to_dto(status);
        assert_eq!(dto.freq_hz, 7_102_000);
        assert_eq!(dto.mode.as_deref(), Some("PKTUSB"));
        assert!(dto.ptt);
    }

    #[test]
    fn rig_status_to_dto_maps_none_mode() {
        let status = tux_rig::RigStatus {
            freq_hz: 7_102_000,
            mode: None,
            ptt: false,
        };
        let dto = rig_status_to_dto(status);
        assert_eq!(dto.mode, None);
    }
}
