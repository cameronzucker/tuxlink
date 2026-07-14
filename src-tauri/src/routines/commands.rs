//! Tauri command surface for Routines (plan 2 Task 6) — the layer that makes
//! the feature reachable: authoring (save/get/list/delete), the enable gate,
//! running (run/dry-run/cancel/status/journal/consent), and CRUD for the two
//! authorable entities a routine references (`@preset:`, `@station-set:`).
//!
//! ## Shape: testable service fns + thin command shims
//!
//! Every command is a one-line forward to a free function taking
//! `&RoutinesState` (the `search/commands.rs` precedent). The service fns are
//! what the tests exercise — tempdir stores, a fake registry, a recording event
//! sink, no Tauri runtime. The `#[tauri::command]` wrappers add nothing but the
//! `State` extraction, so there is no untested logic behind the boundary.
//!
//! ## The validation contract (spec §10) — where errors bite, and where they don't
//!
//! One validator, no privileged path. Every write goes through
//! `tuxlink_routines::validate::validate` against a
//! [`MonolithValidationContext`] (which answers from the same stores and the
//! same action registry the executor uses — see `validation.rs`). What differs
//! is what a finding *does*:
//!
//! * **[`save_routine`] NEVER blocks on findings.** A half-written routine with
//!   an unresolved `@station-set:` is still saved — it is a draft, and an
//!   authoring surface that refuses to save your work-in-progress is a bad
//!   authoring surface. The findings come back IN the save response
//!   ([`SaveResult`]) so the builder can render them inline.
//! * **[`set_routine_enabled`] REFUSES to enable a routine with errors** (spec
//!   §10: "errors block enable/run, never save"), and layers the FLEET check on
//!   top: `validate_fleet(every currently-enabled routine + this one)`, so a
//!   schedule collision or same-effect overlap with an ALREADY-enabled routine
//!   surfaces at the moment the operator arms the second one — which is the only
//!   moment the collision becomes real. Disabling is never blocked.
//! * **[`run_routine`] REFUSES to run a routine with errors**, same rule, same
//!   findings, checked at the moment of the run (the library may have changed
//!   since enable-time).
//! * **[`dry_run_routine`] refuses NOTHING.** A dry-run's whole purpose is to
//!   rehearse a routine that is not yet fit to run — including an unacknowledged
//!   automatic one (the consent gate lets dry-runs through for exactly this
//!   reason). Findings still come back so the panel can show them; nothing is
//!   blocked, and nothing real is touched: [`RoutinesState::start_dry_run`]
//!   routes through the engine's registry swap, so every action executed is a
//!   capability-mirroring fake.
//!
//! ## Names are validated before they become entities
//!
//! `EntityRef::parse` gives a routine `@preset:<name>`. A preset named `""` or
//! `"a:b"` is therefore either unreferenceable or ambiguous — an entity no
//! routine can name. [`valid_entity_name`] rejects those at the command
//! boundary (plan amendment, 2026-07-13), so the stores cannot accumulate
//! junk-named records that the authoring UI offers and the resolver can never
//! find.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use tuxlink_routines::dryrun::{DryRunDefault, DryRunOutcome, DryRunScript};
use tuxlink_routines::error::EngineError;
use tuxlink_routines::journal::{JournalEntry, RunState};
use tuxlink_routines::types::RoutineDef;
use tuxlink_routines::validate::{
    validate, validate_fleet, Finding, Severity, ValidationContext as _,
};

use super::events::{LibraryEntity, RoutinesEvent};
use super::presets::{PresetError, RadioPreset};
use super::scheduler::{anchor_on_enable, schedule_status, ScheduleStatus};
use super::session::{local_utc_offset_seconds, unix_now_secs, RoutineStartError, RoutinesState};
use super::station_sets::{StationSet, StationSetError};
use super::store::{RoutineSummary, StoreError};
use super::validation::MonolithValidationContext;
use crate::ui_commands::UiError;

// ============================================================================
// DTOs
// ============================================================================

/// [`save_routine`]'s response. The routine IS saved (the never-block-save
/// rule); `findings` is what the validator says about it, and `blocked` is the
/// pre-computed "this cannot be enabled or run as it stands" bit so the UI
/// doesn't have to re-derive severity semantics.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub routine: String,
    pub findings: Vec<Finding>,
    pub blocked: bool,
}

/// [`set_routine_enabled`]'s response. `enabled` is the state the routine is
/// ACTUALLY in after the call (not what was asked for): a refused enable
/// reports `enabled: false, blocked: true` plus the blocking findings.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableResult {
    pub routine: String,
    pub enabled: bool,
    pub blocked: bool,
    pub findings: Vec<Finding>,
}

/// [`run_status`]'s response — the fast answer to "what is this run doing".
/// The step-by-step record is [`run_journal`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatusDto {
    pub run_id: String,
    pub routine: String,
    pub dry_run: bool,
    pub state: RunState,
}

/// A dry-run's start response: the run id to poll, plus the validator's
/// findings (informational — a dry run is never blocked by them).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunStarted {
    pub run_id: String,
    pub findings: Vec<Finding>,
}

/// Caller-supplied dry-run script (all fields optional — the default is an
/// optimistic fake world where every action succeeds).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DryRunScriptDto {
    /// What an action with no scripted queue returns.
    pub default_outcome: DryRunDefaultDto,
    /// Per-action outcome queues, keyed by catalog action name. Replayed in
    /// order; the last queued outcome repeats once the queue is exhausted.
    pub outcomes: HashMap<String, Vec<DryRunOutcomeDto>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DryRunDefaultDto {
    /// Every unscripted action succeeds (and radio actions report connected).
    #[default]
    Optimistic,
    /// Every unscripted action fails — rehearses the routine's error handling.
    Pessimistic,
}

/// One scripted step outcome. `{"kind":"ok","output":{…}}` /
/// `{"kind":"err","cause":"VARA: BUSY channel occupied"}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DryRunOutcomeDto {
    Ok {
        #[serde(default)]
        output: serde_json::Value,
    },
    Err {
        cause: String,
    },
}

impl From<DryRunScriptDto> for DryRunScript {
    fn from(dto: DryRunScriptDto) -> Self {
        let mut script = DryRunScript::new().with_default(match dto.default_outcome {
            DryRunDefaultDto::Optimistic => DryRunDefault::Optimistic,
            DryRunDefaultDto::Pessimistic => DryRunDefault::Pessimistic,
        });
        for (action, outcomes) in dto.outcomes {
            script = script.with_outcomes(
                &action,
                outcomes
                    .into_iter()
                    .map(|o| match o {
                        DryRunOutcomeDto::Ok { output } => DryRunOutcome::Ok(output),
                        DryRunOutcomeDto::Err { cause } => DryRunOutcome::Err(cause),
                    })
                    .collect(),
            );
        }
        script
    }
}

// ============================================================================
// Name validation (plan amendment 2026-07-13)
// ============================================================================

/// Is `name` a name a routine can actually REFERENCE? A preset or station-set
/// is reachable from a routine only as an `@<kind>:<name>` token, so a name that
/// `EntityRef::parse` cannot express — empty, or carrying the `:` / `@`
/// structure characters — would create an entity that shows up in the authoring
/// UI and can never be resolved by the engine.
///
/// The rule: `[a-z0-9][a-z0-9-]{0,63}`. A leading digit is allowed (the canonical
/// preset name is `40m-ardop`), which is the one place this differs from
/// `store::valid_name`'s routine rule. Uppercase is rejected rather than folded:
/// a store keyed by exact string match would otherwise let `OR-Gateways` and
/// `or-gateways` coexist as two entities the operator reads as one.
pub fn valid_entity_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn reject_invalid_name(kind: &str, name: &str) -> Result<(), UiError> {
    if valid_entity_name(name) {
        return Ok(());
    }
    Err(UiError::Rejected(format!(
        "{kind} name {name:?} is invalid — use lowercase letters, digits, and hyphens \
         (1-64 chars, e.g. \"or-gateways\"); a routine references it as \
         \"@{kind}:{name}\", which must be an unambiguous token"
    )))
}

// ============================================================================
// Error projection
// ============================================================================

impl From<StoreError> for UiError {
    fn from(e: StoreError) -> Self {
        // Render the Display ONCE, up front: `StoreError`'s own messages already
        // carry the verbatim detail (the serde parse error's "missing field
        // `tracks` at line 4 column 3", the offending name), and the operator
        // sees them unparaphrased.
        let text = e.to_string();
        match e {
            StoreError::NotFound(name) => UiError::NotFound(name),
            StoreError::Parse(_) | StoreError::InvalidName(_) => UiError::Rejected(text),
            StoreError::Io(_) | StoreError::Serialize(_) => UiError::Internal { detail: text },
        }
    }
}

impl From<PresetError> for UiError {
    fn from(e: PresetError) -> Self {
        match e {
            PresetError::NotFound(name) => UiError::NotFound(name),
            other => UiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<StationSetError> for UiError {
    fn from(e: StationSetError) -> Self {
        match e {
            StationSetError::NotFound(name) => UiError::NotFound(name),
            other => UiError::Internal {
                detail: other.to_string(),
            },
        }
    }
}

impl From<RoutineStartError> for UiError {
    fn from(e: RoutineStartError) -> Self {
        let text = e.to_string();
        match e {
            RoutineStartError::UnknownRoutine(name) => UiError::NotFound(name),
            // The consent refusal's message IS the operator-facing instruction
            // (spec §4: "acknowledge automatic-transmission responsibility…") —
            // pass it through verbatim rather than flattening it to "rejected".
            RoutineStartError::UnacknowledgedAutomatic { .. } => UiError::Rejected(text),
            RoutineStartError::Engine(engine) => match engine {
                // An unresolved `@ref` is the operator's to fix, not an internal
                // fault — and the offending token is named verbatim.
                EngineError::Snapshot(s) => UiError::Rejected(s.to_string()),
                EngineError::UnknownRoutine(name) => UiError::NotFound(name),
                other => UiError::Internal {
                    detail: other.to_string(),
                },
            },
        }
    }
}

// ============================================================================
// Validation helpers
// ============================================================================

/// Does any finding BLOCK enable/run (spec §10)? Only errors do; warnings are
/// informational and never stop the operator.
fn has_blocking(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Error)
}

/// Render the blocking findings as one operator-facing refusal, each finding's
/// `message` VERBATIM (spec §10: the message names the offending entity — a
/// summary that says "2 errors" and drops the station-set's name is exactly the
/// paraphrase this feature forbids).
fn refusal(action: &str, routine: &str, findings: &[Finding]) -> UiError {
    let lines: Vec<String> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| match &f.step {
            Some(step) => format!("  [{}] step \"{}\": {}", f.code, step.0, f.message),
            None => format!("  [{}] {}", f.code, f.message),
        })
        .collect();
    UiError::Rejected(format!(
        "cannot {action} \"{routine}\" — validation errors must be fixed first:\n{}",
        lines.join("\n")
    ))
}

/// Validate one definition against the live station (stores + registry +
/// config). Fresh context per call: a preset created a second ago must count.
fn validate_def(state: &RoutinesState, def: &RoutineDef) -> Vec<Finding> {
    let ctx = MonolithValidationContext::for_state(state);
    validate(def, &ctx)
}

// ============================================================================
// Service fns — routine library
// ============================================================================

pub fn list_routines(state: &RoutinesState) -> Vec<RoutineSummary> {
    state.store.list()
}

pub fn get_routine(state: &RoutinesState, name: &str) -> Result<RoutineDef, UiError> {
    state
        .store
        .get(name)
        .ok_or_else(|| UiError::NotFound(name.to_string()))
}

/// Parse → validate → SAVE (always, if it parsed) → return the findings.
///
/// Parse failure is the ONLY way this refuses: an unparseable body is not a
/// draft, it is not a routine at all, and there is nothing to store. The serde
/// error is passed through verbatim so the builder can point at the line.
pub fn save_routine(state: &RoutinesState, def_json: &str) -> Result<SaveResult, UiError> {
    let def = RoutineDef::parse(def_json).map_err(|e| UiError::Rejected(e.to_string()))?;
    let findings = validate_def(state, &def);
    // Save even with errors (spec §10: errors block enable/run, never save).
    // `store.save` still rejects a name that would escape the store directory —
    // that is a safety boundary, not a validation finding.
    state.store.save(&def)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::Routine,
        name: def.routine.clone(),
    });
    Ok(SaveResult {
        blocked: has_blocking(&findings),
        routine: def.routine,
        findings,
    })
}

pub fn delete_routine(state: &RoutinesState, name: &str) -> Result<(), UiError> {
    state.store.delete(name)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::Routine,
        name: name.to_string(),
    });
    Ok(())
}

/// Enable/disable a routine. Enabling runs the FULL gate: the routine's own
/// findings PLUS the fleet's — `validate_fleet` over every currently-enabled
/// routine together with this one, which is what surfaces `SCHEDULE_COLLISION` /
/// `SAME_EFFECT_OVERLAP` against routines that are ALREADY armed. Errors refuse
/// the enable (spec §10); warnings are reported and the routine still arms.
///
/// Disabling always succeeds. A routine you cannot turn OFF because it fails
/// validation would be an absurd trap — and an unsafe one.
///
/// ### Why the findings are a DELTA, not a filter
///
/// `validate_fleet` re-validates every routine in the set, so its output mixes
/// this routine's findings with every sibling's. Two naive scopings both fail:
/// filtering by `Finding.routine == name` DROPS the fleet findings (a
/// `SCHEDULE_COLLISION` is attributed to the alphabetically-first routine of the
/// pair, `fleet.rs` — enabling `second` against `first` yields a finding whose
/// `routine` is `"first"`), and NOT filtering blocks this enable on a sibling's
/// long-tolerated pre-existing error.
///
/// So: validate the fleet WITHOUT this routine (the baseline — whatever is
/// already wrong out there, and already tolerated), validate it WITH, and report
/// the difference. That is exactly "what arming this routine introduces",
/// however the underlying check chose to attribute it.
///
/// `utc_offset_seconds` (`local - utc`) is the SAME seam `next_fire` and the
/// scheduler's tick loop use (Task 6c) — a `SCHEDULE_COLLISION`/
/// `SAME_EFFECT_OVERLAP` finding is computed over fire sequences a
/// window-gated `Trigger::Schedule` produces in the operator's LOCAL clock,
/// not UTC, so this check must be told the same offset the routine will
/// actually fire under. The Tauri command wrapper resolves it via
/// `session::local_utc_offset_seconds`; tests pass a literal.
pub fn set_routine_enabled(
    state: &RoutinesState,
    name: &str,
    enabled: bool,
    now_unix: i64,
    utc_offset_seconds: i32,
) -> Result<EnableResult, UiError> {
    let def = get_routine(state, name)?;

    if !enabled {
        state.store.set_enabled(name, false)?;
        state.emit(&RoutinesEvent::LibraryChanged {
            entity: LibraryEntity::Routine,
            name: name.to_string(),
        });
        return Ok(EnableResult {
            routine: def.routine,
            enabled: false,
            blocked: false,
            findings: Vec::new(),
        });
    }

    let ctx = MonolithValidationContext::for_state(state);

    // The fleet as it stands, minus this routine (re-enabling an already-enabled
    // routine must not make it collide with ITSELF).
    let baseline_defs: Vec<RoutineDef> = ctx
        .enabled_routines()
        .into_iter()
        .filter(|d| d.routine != def.routine)
        .collect();
    let baseline = validate_fleet(&baseline_defs, &ctx, now_unix, utc_offset_seconds);

    let mut armed_defs = baseline_defs;
    armed_defs.push(def.clone());
    let armed = validate_fleet(&armed_defs, &ctx, now_unix, utc_offset_seconds);

    let findings = introduced_findings(armed, baseline);

    if has_blocking(&findings) {
        return Ok(EnableResult {
            routine: def.routine,
            enabled: false,
            blocked: true,
            findings,
        });
    }

    // Anchor the cadence at the ENABLE instant, before the routine is armed.
    //
    // Only on a real disabled → enabled transition: re-enabling an
    // already-enabled routine (a no-op the UI can emit on a double-click) must
    // not shift a live schedule. The scheduler measures a routine's next fire
    // from its anchor, and without this write the anchor would be whatever the
    // routine's PREVIOUS enabled period left behind — for an unaligned schedule
    // that is an instant in the past, so the routine's first act on being
    // re-enabled would be an immediate catch-up fire. `if_missed: skip` says not
    // to do that, and on an automatic-transmit routine it would be a
    // transmission the operator never asked for. Structural, not incidental:
    // "a freshly-enabled routine has no misses" is now true because the anchor
    // says so, rather than because no record happened to exist yet.
    if !state.store.is_enabled(name) {
        anchor_on_enable(state, name, now_unix);
    }

    state.store.set_enabled(name, true)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::Routine,
        name: name.to_string(),
    });
    Ok(EnableResult {
        routine: def.routine,
        enabled: true,
        blocked: false,
        findings,
    })
}

/// Multiset difference `armed - baseline`: the findings that exist only because
/// the routine being enabled was added to the fleet. Multiset (not set) so two
/// identical findings from two different pairs are not collapsed into one.
/// Quadratic, over a handful of findings across a handful of routines.
fn introduced_findings(armed: Vec<Finding>, baseline: Vec<Finding>) -> Vec<Finding> {
    let mut unmatched = baseline;
    let mut introduced = Vec::new();
    for f in armed {
        match unmatched.iter().position(|b| *b == f) {
            Some(i) => {
                unmatched.remove(i);
            }
            None => introduced.push(f),
        }
    }
    introduced
}

// ============================================================================
// Service fns — runs
// ============================================================================

/// Start a real run. Refuses on validation errors (spec §10) BEFORE the consent
/// gate ever sees it, then hands off to [`RoutinesState::start_routine`], whose
/// transmit-consent start gate is the authority on whether an automatic
/// transmitting routine may run at all.
pub async fn run_routine(
    state: &Arc<RoutinesState>,
    name: &str,
    args: serde_json::Value,
) -> Result<String, UiError> {
    let def = get_routine(state, name)?;
    let findings = validate_def(state, &def);
    if has_blocking(&findings) {
        return Err(refusal("run", name, &findings));
    }
    Ok(state.start_routine(name, args).await?)
}

/// Start a DRY run — the fake world (spec §10 layer 3). Blocked by nothing:
/// rehearsing a routine that is not yet fit to run is the entire point. Every
/// action is swapped for a capability-mirroring fake by
/// [`RoutinesState::start_dry_run`], so no rig is seized, no carrier is keyed,
/// no message is queued, and no gateway is dialed.
pub async fn dry_run_routine(
    state: &Arc<RoutinesState>,
    name: &str,
    args: serde_json::Value,
    script: Option<DryRunScriptDto>,
) -> Result<DryRunStarted, UiError> {
    let def = get_routine(state, name)?;
    let findings = validate_def(state, &def);
    let script: DryRunScript = script.unwrap_or_default().into();
    let run_id = state.start_dry_run(name, args, script).await?;
    Ok(DryRunStarted { run_id, findings })
}

/// Cancel a live run. `false` = no such live run (already finished, or never
/// existed) — not an error: the caller's intent ("this must not be running") is
/// satisfied either way.
pub fn cancel_run(state: &RoutinesState, run_id: &str) -> bool {
    state.cancel_run(run_id)
}

pub fn run_status(state: &RoutinesState, run_id: &str) -> Option<RunStatusDto> {
    state.run_status(run_id).map(|s| RunStatusDto {
        run_id: s.run_id,
        routine: s.routine,
        dry_run: s.dry_run,
        state: s.state,
    })
}

/// A run's journal, verbatim (spec §11): every `StepErr.cause` is the actual
/// VARA/CAT/HTTP failure text the action surfaced, passed through untouched.
pub fn run_journal(state: &RoutinesState, run_id: &str) -> Result<Vec<JournalEntry>, UiError> {
    state
        .journal_entries(run_id)
        .ok_or_else(|| UiError::NotFound(run_id.to_string()))
}

/// Grant per-transmission consent for a parked attended-mode transmit step
/// (spec §4). `false` = nothing was parked under that `(run_id, step_id)`.
///
/// **Operator-only.** This is a UI command; the MCP surface has no path to it.
pub fn grant_consent(state: &RoutinesState, run_id: &str, step_id: &str) -> bool {
    state.grant_consent(run_id, step_id)
}

/// Everything the scheduler has to say about why fires did not happen, per
/// routine: fires that elapsed while the app was CLOSED (spec §8: "misses are
/// recorded visibly either way"), the last fire the gate REFUSED (verbatim), and
/// the last fire SKIPPED because the previous run was still going.
///
/// The scheduler emits an event for each of these as it happens, but an event
/// only reaches a listener that was already listening — a window opened
/// afterwards (or an operator who was asleep at 03:00) reads the persisted record
/// through here. Empty when there is nothing to report.
pub fn schedule_status_report(state: &RoutinesState) -> Vec<ScheduleStatus> {
    schedule_status(state)
}

// ============================================================================
// Service fns — presets + station sets (the authorable @-entities)
// ============================================================================

pub fn list_presets(state: &RoutinesState) -> Vec<RadioPreset> {
    state.presets.list()
}

pub fn save_preset(state: &RoutinesState, preset: RadioPreset) -> Result<(), UiError> {
    reject_invalid_name("preset", &preset.name)?;
    state.presets.save(&preset)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::Preset,
        name: preset.name,
    });
    Ok(())
}

pub fn delete_preset(state: &RoutinesState, name: &str) -> Result<(), UiError> {
    state.presets.delete(name)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::Preset,
        name: name.to_string(),
    });
    Ok(())
}

pub fn list_station_sets(state: &RoutinesState) -> Vec<StationSet> {
    state.station_sets.list()
}

pub fn save_station_set(state: &RoutinesState, set: StationSet) -> Result<(), UiError> {
    reject_invalid_name("station-set", &set.name)?;
    state.station_sets.save(&set)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::StationSet,
        name: set.name,
    });
    Ok(())
}

pub fn delete_station_set(state: &RoutinesState, name: &str) -> Result<(), UiError> {
    state.station_sets.delete(name)?;
    state.emit(&RoutinesEvent::LibraryChanged {
        entity: LibraryEntity::StationSet,
        name: name.to_string(),
    });
    Ok(())
}

// ============================================================================
// Tauri command shims — one line each, no logic.
//
// `State<'_, Arc<RoutinesState>>` is spelled out in full on every command:
// `#[tauri::command]` inspects parameter types SYNTACTICALLY to tell a managed-
// state param from a deserialized one, so a type alias would make it try to
// deserialize the state out of the IPC payload.
// ============================================================================

#[tauri::command]
pub async fn routines_list(
    state: State<'_, Arc<RoutinesState>>,
) -> Result<Vec<RoutineSummary>, UiError> {
    Ok(list_routines(&state))
}

#[tauri::command]
pub async fn routines_get(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
) -> Result<RoutineDef, UiError> {
    get_routine(&state, &name)
}

#[tauri::command]
pub async fn routines_save(
    state: State<'_, Arc<RoutinesState>>,
    def_json: String,
) -> Result<SaveResult, UiError> {
    save_routine(&state, &def_json)
}

#[tauri::command]
pub async fn routines_delete(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
) -> Result<(), UiError> {
    delete_routine(&state, &name)
}

#[tauri::command]
pub async fn routines_set_enabled(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
    enabled: bool,
) -> Result<EnableResult, UiError> {
    set_routine_enabled(
        &state,
        &name,
        enabled,
        unix_now_secs(),
        local_utc_offset_seconds(),
    )
}

#[tauri::command]
pub async fn routines_run(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
    args: serde_json::Value,
) -> Result<String, UiError> {
    let state = Arc::clone(&state);
    run_routine(&state, &name, args).await
}

#[tauri::command]
pub async fn routines_dry_run(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
    args: serde_json::Value,
    script: Option<DryRunScriptDto>,
) -> Result<DryRunStarted, UiError> {
    let state = Arc::clone(&state);
    dry_run_routine(&state, &name, args, script).await
}

#[tauri::command]
pub async fn routines_cancel(
    state: State<'_, Arc<RoutinesState>>,
    run_id: String,
) -> Result<bool, UiError> {
    Ok(cancel_run(&state, &run_id))
}

#[tauri::command]
pub async fn routines_run_status(
    state: State<'_, Arc<RoutinesState>>,
    run_id: String,
) -> Result<Option<RunStatusDto>, UiError> {
    Ok(run_status(&state, &run_id))
}

#[tauri::command]
pub async fn routines_journal(
    state: State<'_, Arc<RoutinesState>>,
    run_id: String,
) -> Result<Vec<JournalEntry>, UiError> {
    run_journal(&state, &run_id)
}

#[tauri::command]
pub async fn routines_consent_grant(
    state: State<'_, Arc<RoutinesState>>,
    run_id: String,
    step_id: String,
) -> Result<bool, UiError> {
    Ok(grant_consent(&state, &run_id, &step_id))
}

/// The schedule-status read path. Keeps its historical `missed_fires` name — it
/// is the registered command surface, and `lib.rs`'s `invoke_handler` names it —
/// but it now answers for all three ways a fire fails to happen (missed while
/// closed, refused at the gate, skipped over a still-running previous run), not
/// just the first. See [`ScheduleStatus`].
#[tauri::command]
pub async fn routines_missed_fires(
    state: State<'_, Arc<RoutinesState>>,
) -> Result<Vec<ScheduleStatus>, UiError> {
    Ok(schedule_status_report(&state))
}

#[tauri::command]
pub async fn routines_presets_list(
    state: State<'_, Arc<RoutinesState>>,
) -> Result<Vec<RadioPreset>, UiError> {
    Ok(list_presets(&state))
}

#[tauri::command]
pub async fn routines_presets_save(
    state: State<'_, Arc<RoutinesState>>,
    preset: RadioPreset,
) -> Result<(), UiError> {
    save_preset(&state, preset)
}

#[tauri::command]
pub async fn routines_presets_delete(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
) -> Result<(), UiError> {
    delete_preset(&state, &name)
}

#[tauri::command]
pub async fn routines_station_sets_list(
    state: State<'_, Arc<RoutinesState>>,
) -> Result<Vec<StationSet>, UiError> {
    Ok(list_station_sets(&state))
}

#[tauri::command]
pub async fn routines_station_sets_save(
    state: State<'_, Arc<RoutinesState>>,
    set: StationSet,
) -> Result<(), UiError> {
    save_station_set(&state, set)
}

#[tauri::command]
pub async fn routines_station_sets_delete(
    state: State<'_, Arc<RoutinesState>>,
    name: String,
) -> Result<(), UiError> {
    delete_station_set(&state, &name)
}

// ============================================================================
// Tests — service-fn level: tempdir stores, a fake registry, a recording sink.
// No AppHandle, no Tauri runtime (the shims above add no logic to cover).
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::arbiter::RadioArbiter;
    use crate::routines::events::RoutinesEventSink;
    use crate::routines::session::build_routines_state;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;
    use tempfile::TempDir;
    use tuxlink_routines::action::ActionRegistry;
    use tuxlink_routines::fakes::FakeAction;
    use tuxlink_routines::journal::RunEvent;

    const NOW: i64 = 1_752_400_000;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<RoutinesEvent>>,
    }

    impl RecordingSink {
        fn events(&self) -> Vec<RoutinesEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl RoutinesEventSink for RecordingSink {
        fn emit(&self, event: &RoutinesEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn fixed_now() -> i64 {
        NOW
    }

    /// A state over a tempdir config with a fake catalog:
    /// `local.log` (inert), `radio.connect` (needs_radio + transmits).
    fn test_state() -> (
        TempDir,
        Arc<RoutinesState>,
        Arc<RecordingSink>,
        Arc<FakeAction>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let connect = Arc::new(
            FakeAction::new("radio.connect")
                .with_capabilities(true, true, false)
                .ok(json!({"connected": true})),
        );
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        reg.register(connect.clone());

        let sink = Arc::new(RecordingSink::default());
        let sink_dyn: Arc<dyn RoutinesEventSink> = sink.clone();
        let state = Arc::new(build_routines_state(
            dir.path().to_path_buf(),
            reg,
            Arc::new(RadioArbiter::new(fixed_now)),
            sink_dyn,
        ));
        (dir, state, sink, connect)
    }

    /// A routine JSON body with the given name and steps (spec §14 shape).
    fn def_json(name: &str, steps: serde_json::Value) -> String {
        json!({
            "routine": name,
            "schema_version": 1,
            "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "t", "steps": steps}]
        })
        .to_string()
    }

    /// An inert, structurally COMPLETE track: one `local.log` action and an
    /// explicit `End`. The `End` matters — a track that can run past its last
    /// step earns a `NO_TERMINAL_PATH` warning (`validate::structure`), and the
    /// tests below that assert "no findings" mean a genuinely clean routine, not
    /// one whose only finding we chose to ignore.
    fn log_step() -> serde_json::Value {
        json!([
            {"id": "s1", "action": "local.log", "params": {}},
            {"id": "e1", "control": "end"}
        ])
    }

    async fn wait_until<F: Fn() -> bool>(p: F) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if p() {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "wait_until timed out");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // ── save: never blocked, always reports findings ─────────────────────

    #[test]
    fn save_returns_findings_and_still_saves_the_draft() {
        let (_dir, state, sink, _c) = test_state();

        // References a station-set that does not exist AND an action that is
        // not in the catalog: two ERROR findings.
        let body = def_json(
            "half-written",
            json!([
                {"id": "s1", "action": "radio.connect",
                 "params": {"stations": "@station-set:or-gateways"}},
                {"id": "s2", "action": "radio.mystery", "params": {}}
            ]),
        );

        let result = save_routine(&state, &body).expect("a parseable draft always saves");
        assert_eq!(result.routine, "half-written");
        assert!(result.blocked, "errors must mark the draft un-runnable");

        let codes: Vec<&str> = result.findings.iter().map(|f| f.code).collect();
        assert!(
            codes.contains(&"UNRESOLVED_REF"),
            "the unresolved @station-set must be reported: {codes:?}"
        );
        assert!(
            codes.contains(&"UNKNOWN_ACTION"),
            "the unknown action must be reported: {codes:?}"
        );

        // The draft IS on disk despite the errors (never-block-save).
        assert!(
            state.store.get("half-written").is_some(),
            "a draft with validation errors must still be saved"
        );
        assert!(sink.events().iter().any(|e| matches!(e,
            RoutinesEvent::LibraryChanged { entity: LibraryEntity::Routine, name }
                if name == "half-written")));
    }

    #[test]
    fn save_returns_no_findings_for_a_clean_routine() {
        let (_dir, state, _sink, _c) = test_state();
        let result = save_routine(&state, &def_json("clean", log_step())).unwrap();
        assert_eq!(
            result.findings,
            Vec::new(),
            "a clean routine has no findings"
        );
        assert!(!result.blocked);
    }

    #[test]
    fn save_refuses_unparseable_json_with_the_verbatim_parse_error() {
        let (_dir, state, _sink, _c) = test_state();
        let err = save_routine(&state, "{ not json").unwrap_err();
        match err {
            UiError::Rejected(msg) => {
                assert!(
                    msg.contains("malformed"),
                    "the verbatim parse error must survive: {msg}"
                );
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
        assert!(state.store.list().is_empty(), "nothing was written");
    }

    #[test]
    fn save_refuses_a_routine_name_that_would_escape_the_store_directory() {
        let (_dir, state, _sink, _c) = test_state();
        let err = save_routine(&state, &def_json("../config", log_step())).unwrap_err();
        assert!(matches!(err, UiError::Rejected(_)));
    }

    // ── enable: errors refuse; the fleet check runs ──────────────────────

    #[test]
    fn enable_refuses_a_routine_with_validation_errors() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(
            &state,
            &def_json(
                "broken",
                json!([{"id": "s1", "action": "radio.mystery", "params": {}}]),
            ),
        )
        .unwrap();

        let result = set_routine_enabled(&state, "broken", true, NOW, 0).unwrap();
        assert!(!result.enabled, "a routine with errors must not enable");
        assert!(result.blocked);
        assert!(result
            .findings
            .iter()
            .any(|f| f.code == "UNKNOWN_ACTION" && f.severity == Severity::Error));
        assert!(
            !state.store.is_enabled("broken"),
            "the store must not have been flipped"
        );
    }

    #[test]
    fn enable_succeeds_for_a_clean_routine_and_disable_is_never_blocked() {
        let (_dir, state, sink, _c) = test_state();
        save_routine(&state, &def_json("clean", log_step())).unwrap();

        let result = set_routine_enabled(&state, "clean", true, NOW, 0).unwrap();
        assert!(result.enabled);
        assert!(!result.blocked);
        assert!(state.store.is_enabled("clean"));

        // Now break it (the library is editable after enable), then disable:
        // disabling must never be refused, however invalid the routine is.
        save_routine(
            &state,
            &def_json(
                "clean",
                json!([{"id": "s1", "action": "radio.mystery", "params": {}}]),
            ),
        )
        .unwrap();
        let off = set_routine_enabled(&state, "clean", false, NOW, 0).unwrap();
        assert!(!off.enabled);
        assert!(!off.blocked, "disable is never blocked");
        assert!(!state.store.is_enabled("clean"));

        assert!(sink.events().iter().any(|e| matches!(
            e,
            RoutinesEvent::LibraryChanged {
                entity: LibraryEntity::Routine,
                ..
            }
        )));
    }

    /// A scheduled, radio-touching routine — both properties are required for
    /// `SCHEDULE_COLLISION` (`fleet.rs`: the collision is about contending for
    /// the station's single rig at the same instant).
    fn scheduled_radio_routine(name: &str) -> String {
        json!({
            "routine": name,
            "schema_version": 1,
            "transmit_mode": "attended",
            "triggers": [{"type": "schedule", "every": "30m", "align": "hour"}],
            "tracks": [{"name": "t", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}},
                {"id": "e1", "control": "end"}
            ]}]
        })
        .to_string()
    }

    /// The fleet check fires at the moment the SECOND routine is armed: two
    /// schedules that fire at the same instant only collide once both are
    /// enabled.
    ///
    /// Note the finding is a WARNING, so the enable still succeeds — "errors
    /// block enable/run" and a collision is not an error. What matters is that
    /// the operator is TOLD, at the moment the collision becomes real.
    #[test]
    fn enable_surfaces_a_fleet_collision_with_an_already_enabled_routine() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(&state, &scheduled_radio_routine("first")).unwrap();
        save_routine(&state, &scheduled_radio_routine("second")).unwrap();

        // The first arms with nothing to collide with.
        let a = set_routine_enabled(&state, "first", true, NOW, 0).unwrap();
        assert!(a.enabled);
        assert!(
            !a.findings.iter().any(|f| f.code == "SCHEDULE_COLLISION"),
            "a lone routine cannot collide"
        );

        // The second has the identical aligned schedule and also touches the
        // radio → collision, surfaced against the routine being enabled.
        let b = set_routine_enabled(&state, "second", true, NOW, 0).unwrap();
        let collision = b
            .findings
            .iter()
            .find(|f| f.code == "SCHEDULE_COLLISION")
            .unwrap_or_else(|| {
                panic!(
                    "the fleet check must surface the collision: {:?}",
                    b.findings.iter().map(|f| f.code).collect::<Vec<_>>()
                )
            });
        // The message names BOTH routines verbatim (fleet.rs attributes the
        // finding to the alphabetically-first of the pair — which is precisely
        // why the enable gate reports the fleet DELTA rather than filtering by
        // `Finding.routine`; a name filter would have dropped this).
        assert!(collision.message.contains("first"), "{}", collision.message);
        assert!(
            collision.message.contains("second"),
            "{}",
            collision.message
        );
        assert!(b.enabled, "a collision is a warning, not a block");
    }

    /// Re-enabling an already-enabled routine must not make it collide with
    /// ITSELF (the baseline fleet excludes the routine being enabled).
    #[test]
    fn re_enabling_an_enabled_routine_does_not_collide_with_itself() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(&state, &scheduled_radio_routine("solo")).unwrap();

        assert!(
            set_routine_enabled(&state, "solo", true, NOW, 0)
                .unwrap()
                .enabled
        );
        let again = set_routine_enabled(&state, "solo", true, NOW, 0).unwrap();
        assert!(again.enabled);
        assert!(
            !again
                .findings
                .iter()
                .any(|f| f.code == "SCHEDULE_COLLISION"),
            "re-enabling must not report a self-collision: {:?}",
            again.findings.iter().map(|f| f.code).collect::<Vec<_>>()
        );
    }

    /// A sibling routine's PRE-EXISTING error must not block an unrelated
    /// routine's enable — the gate reports what THIS enable introduces.
    #[test]
    fn a_broken_sibling_does_not_block_an_unrelated_enable() {
        let (_dir, state, _sink, _c) = test_state();

        // A broken routine that somehow got enabled (saved clean, then edited
        // to reference an unknown action — the library is editable after enable).
        save_routine(&state, &def_json("sibling", log_step())).unwrap();
        assert!(
            set_routine_enabled(&state, "sibling", true, NOW, 0)
                .unwrap()
                .enabled
        );
        save_routine(
            &state,
            &def_json(
                "sibling",
                json!([{"id": "s1", "action": "radio.mystery", "params": {}}]),
            ),
        )
        .unwrap();

        // An unrelated, clean routine must still arm.
        save_routine(&state, &def_json("mine", log_step())).unwrap();
        let result = set_routine_enabled(&state, "mine", true, NOW, 0).unwrap();
        assert!(
            result.enabled,
            "a sibling's pre-existing error must not block this enable: {:?}",
            result.findings
        );
        assert!(result.findings.is_empty());
    }

    // ── run: errors block; status + journal round-trip ───────────────────

    #[tokio::test]
    async fn run_refuses_a_routine_with_validation_errors() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(
            &state,
            &def_json(
                "broken",
                json!([{"id": "s1", "action": "radio.mystery", "params": {}}]),
            ),
        )
        .unwrap();

        let err = run_routine(&state, "broken", json!({})).await.unwrap_err();
        match err {
            UiError::Rejected(msg) => {
                assert!(msg.contains("UNKNOWN_ACTION"), "names the finding: {msg}");
                assert!(msg.contains("radio.mystery"), "verbatim message: {msg}");
            }
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_then_status_then_journal_round_trips() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(&state, &def_json("quick", log_step())).unwrap();

        let run_id = run_routine(&state, "quick", json!({})).await.unwrap();

        // status: live, then terminal.
        let live = run_status(&state, &run_id).expect("the run is registered");
        assert_eq!(live.routine, "quick");
        assert!(!live.dry_run);

        wait_until(|| run_status(&state, &run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;

        // journal: the durable step-by-step record.
        let entries = run_journal(&state, &run_id).expect("a started run has a journal");
        assert!(
            entries.iter().any(
                |e| matches!(&e.event, RunEvent::RunStarted { routine, .. } if routine == "quick")
            ),
            "journal opens with RunStarted"
        );
        assert!(
            entries
                .iter()
                .any(|e| matches!(&e.event, RunEvent::StepOk { step, .. } if step.0 == "s1")),
            "the step's result is journalled"
        );
        assert!(
            entries.iter().any(|e| matches!(
                &e.event,
                RunEvent::RunFinished {
                    state: RunState::Completed,
                    ..
                }
            )),
            "the terminus is journalled"
        );
    }

    #[tokio::test]
    async fn journal_of_an_unknown_run_is_not_found_and_a_junk_run_id_cannot_traverse() {
        let (_dir, state, _sink, _c) = test_state();
        assert!(matches!(
            run_journal(&state, "run-does-not-exist"),
            Err(UiError::NotFound(_))
        ));
        // A run id that would escape the journal directory is refused by the
        // id validator before any path is built.
        assert!(matches!(
            run_journal(&state, "../../etc/passwd"),
            Err(UiError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn cancel_of_an_unknown_run_is_false_not_an_error() {
        let (_dir, state, _sink, _c) = test_state();
        assert!(!cancel_run(&state, "run-nope"));
    }

    // ── consent: a parked attended transmit step resumes on grant ────────

    #[tokio::test]
    async fn consent_grant_resumes_a_parked_attended_transmit_step() {
        let (_dir, state, sink, connect) = test_state();

        // Attended + a transmitting step: the executor parks it on the consent
        // registry and emits AwaitingConsent.
        save_routine(
            &state,
            &def_json(
                "tx",
                json!([{"id": "s1", "action": "radio.connect", "params": {}}]),
            ),
        )
        .unwrap();

        let run_id = run_routine(&state, "tx", json!({})).await.unwrap();

        wait_until(|| {
            sink.events()
                .iter()
                .any(|e| matches!(e, RoutinesEvent::AwaitingConsent { .. }))
        })
        .await;
        assert!(
            connect.calls().is_empty(),
            "the transmit action must NOT have run before consent"
        );

        assert!(
            grant_consent(&state, &run_id, "s1"),
            "the step was parked, so the grant lands"
        );
        wait_until(|| !connect.calls().is_empty()).await;
        wait_until(|| run_status(&state, &run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;

        // A grant for a step that is not parked is false, not an error.
        assert!(!grant_consent(&state, &run_id, "s1"));
    }

    // ── dry run: touches nothing real ────────────────────────────────────

    #[tokio::test]
    async fn dry_run_executes_no_real_action_and_is_never_blocked_by_findings() {
        let (_dir, state, _sink, connect) = test_state();

        // Automatic + transmitting + NO ack: this routine cannot legally RUN
        // (the consent gate refuses it) — but it must still DRY-run, and the
        // real radio.connect must never execute.
        let body = json!({
            "routine": "auto-tx",
            "schema_version": 1,
            "transmit_mode": "automatic",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "t", "steps": [
                {"id": "s1", "action": "radio.connect", "params": {}}
            ]}]
        })
        .to_string();
        save_routine(&state, &body).unwrap();

        // The real run is refused by the consent gate...
        let err = run_routine(&state, "auto-tx", json!({})).await.unwrap_err();
        assert!(matches!(err, UiError::Rejected(_)));
        assert!(connect.calls().is_empty());

        // ...but the dry run proceeds, and completes.
        let started = dry_run_routine(&state, "auto-tx", json!({}), None)
            .await
            .expect("a dry run is never gated");
        wait_until(|| {
            run_status(&state, &started.run_id).map(|s| s.state) == Some(RunState::Completed)
        })
        .await;

        // THE invariant: the REAL action was never executed. The engine swapped
        // the whole registry for capability-mirroring fakes.
        assert!(
            connect.calls().is_empty(),
            "a dry run must never execute a real action"
        );
        assert!(
            run_status(&state, &started.run_id).unwrap().dry_run,
            "the run is stamped as a dry run"
        );
    }

    #[tokio::test]
    async fn dry_run_script_drives_the_fake_world() {
        let (_dir, state, _sink, connect) = test_state();
        save_routine(
            &state,
            &def_json(
                "tx",
                json!([{"id": "s1", "action": "radio.connect", "params": {}}]),
            ),
        )
        .unwrap();

        // Script radio.connect to FAIL with a verbatim VARA cause.
        let script = DryRunScriptDto {
            default_outcome: DryRunDefaultDto::Optimistic,
            outcomes: HashMap::from([(
                "radio.connect".to_string(),
                vec![DryRunOutcomeDto::Err {
                    cause: "VARA: BUSY channel occupied".into(),
                }],
            )]),
        };

        let started = dry_run_routine(&state, "tx", json!({}), Some(script))
            .await
            .unwrap();
        wait_until(|| {
            matches!(
                run_status(&state, &started.run_id).map(|s| s.state),
                Some(RunState::Failed)
            )
        })
        .await;

        // The scripted failure reached the journal VERBATIM.
        let entries = run_journal(&state, &started.run_id).unwrap();
        let causes: Vec<String> = entries
            .iter()
            .filter_map(|e| match &e.event {
                RunEvent::StepErr {
                    error: tuxlink_routines::error::StepError::Action { cause, .. },
                    ..
                } => Some(cause.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(causes, vec!["VARA: BUSY channel occupied".to_string()]);
        assert!(connect.calls().is_empty(), "still no real action");
    }

    // ── name validation (plan amendment) ─────────────────────────────────

    #[test]
    fn entity_name_validation_accepts_real_names_and_rejects_junk() {
        for good in ["or-gateways", "40m-ardop", "a", "x1-y2"] {
            assert!(valid_entity_name(good), "{good} must be accepted");
        }
        for bad in [
            "",
            "a:b",            // EntityRef::parse would split it
            "@preset:nested", // structure characters
            "OR-Gateways",    // uppercase → case-collision ambiguity
            "with space",
            "../escape",
            "trailing/slash",
        ] {
            assert!(!valid_entity_name(bad), "{bad:?} must be rejected");
        }
        assert!(valid_entity_name(&"a".repeat(64)));
        assert!(!valid_entity_name(&"a".repeat(65)));
    }

    #[test]
    fn preset_and_station_set_save_reject_names_a_routine_could_not_reference() {
        let (_dir, state, _sink, _c) = test_state();

        let bad_preset = RadioPreset {
            name: "40m:ardop".into(),
            frequency_hz: 7_070_000,
            mode: "ARDOP".into(),
            power_w: None,
            atu: None,
        };
        assert!(matches!(
            save_preset(&state, bad_preset),
            Err(UiError::Rejected(_))
        ));
        assert!(list_presets(&state).is_empty(), "nothing was written");

        let bad_set = StationSet {
            name: "".into(),
            callsigns: vec!["W7DEF-10".into()],
        };
        assert!(matches!(
            save_station_set(&state, bad_set),
            Err(UiError::Rejected(_))
        ));
        assert!(list_station_sets(&state).is_empty());
    }

    #[test]
    fn preset_and_station_set_crud_round_trip_and_emit_library_events() {
        let (_dir, state, sink, _c) = test_state();

        save_preset(
            &state,
            RadioPreset {
                name: "40m-ardop".into(),
                frequency_hz: 7_070_000,
                mode: "ARDOP".into(),
                power_w: Some(20),
                atu: Some(true),
            },
        )
        .unwrap();
        save_station_set(
            &state,
            StationSet {
                name: "or-gateways".into(),
                callsigns: vec!["W7DEF-10".into(), "K7ABC-10".into()],
            },
        )
        .unwrap();

        assert_eq!(list_presets(&state).len(), 1);
        assert_eq!(list_station_sets(&state)[0].callsigns.len(), 2);

        // A routine can now REFERENCE them: the same names resolve as entities,
        // so a definition using @preset:40m-ardop validates clean.
        let body = def_json(
            "uses-entities",
            json!([{"id": "s1", "action": "radio.connect",
                    "params": {"preset": "@preset:40m-ardop",
                               "stations": "@station-set:or-gateways"}}]),
        );
        let result = save_routine(&state, &body).unwrap();
        assert!(
            !result.findings.iter().any(|f| f.code == "UNRESOLVED_REF"),
            "the entities created above must resolve: {:?}",
            result.findings
        );

        delete_preset(&state, "40m-ardop").unwrap();
        delete_station_set(&state, "or-gateways").unwrap();
        assert!(list_presets(&state).is_empty());
        assert!(list_station_sets(&state).is_empty());

        let events = sink.events();
        assert!(events.iter().any(|e| matches!(e,
            RoutinesEvent::LibraryChanged { entity: LibraryEntity::Preset, name } if name == "40m-ardop")));
        assert!(events.iter().any(|e| matches!(e,
            RoutinesEvent::LibraryChanged { entity: LibraryEntity::StationSet, name } if name == "or-gateways")));
    }

    #[test]
    fn deleting_a_missing_preset_or_station_set_is_not_found() {
        let (_dir, state, _sink, _c) = test_state();
        assert!(matches!(
            delete_preset(&state, "ghost"),
            Err(UiError::NotFound(_))
        ));
        assert!(matches!(
            delete_station_set(&state, "ghost"),
            Err(UiError::NotFound(_))
        ));
    }

    // ── list / get / delete ──────────────────────────────────────────────

    #[test]
    fn list_get_delete_round_trip() {
        let (_dir, state, _sink, _c) = test_state();
        save_routine(&state, &def_json("alpha", log_step())).unwrap();
        save_routine(&state, &def_json("zeta", log_step())).unwrap();

        let names: Vec<String> = list_routines(&state)
            .into_iter()
            .map(|s| s.routine)
            .collect();
        assert_eq!(names, vec!["alpha".to_string(), "zeta".to_string()]);

        assert_eq!(get_routine(&state, "alpha").unwrap().routine, "alpha");
        assert!(matches!(
            get_routine(&state, "nope"),
            Err(UiError::NotFound(_))
        ));

        delete_routine(&state, "alpha").unwrap();
        assert_eq!(list_routines(&state).len(), 1);
        assert!(matches!(
            delete_routine(&state, "alpha"),
            Err(UiError::NotFound(_))
        ));
    }

    #[test]
    fn dry_run_script_dto_defaults_to_optimistic() {
        // The frontend may send `undefined`/`{}` for the script.
        let dto: DryRunScriptDto = serde_json::from_value(json!({})).unwrap();
        assert_eq!(dto.default_outcome, DryRunDefaultDto::Optimistic);
        assert!(dto.outcomes.is_empty());

        let dto: DryRunScriptDto = serde_json::from_value(json!({
            "defaultOutcome": "pessimistic",
            "outcomes": {"radio.connect": [{"kind": "err", "cause": "boom"}]}
        }))
        .unwrap();
        assert_eq!(dto.default_outcome, DryRunDefaultDto::Pessimistic);
        assert_eq!(
            dto.outcomes["radio.connect"],
            vec![DryRunOutcomeDto::Err {
                cause: "boom".into()
            }]
        );

        let script: DryRunScript = dto.into();
        assert_eq!(script.default, DryRunDefault::Pessimistic);
    }

    /// A path a real UI takes: build the state over a config dir that has no
    /// routines dir yet, list (empty), save, list (one).
    #[test]
    fn a_fresh_station_lists_no_routines() {
        let dir = tempfile::tempdir().unwrap();
        let sink: Arc<dyn RoutinesEventSink> = Arc::new(RecordingSink::default());
        let state = Arc::new(build_routines_state(
            PathBuf::from(dir.path()),
            ActionRegistry::default(),
            Arc::new(RadioArbiter::new(fixed_now)),
            sink,
        ));
        assert!(list_routines(&state).is_empty());
        assert!(list_presets(&state).is_empty());
        assert!(list_station_sets(&state).is_empty());
    }
}
