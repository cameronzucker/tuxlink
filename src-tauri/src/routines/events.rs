//! Routines → TypeScript event contract (plan 2 Task 5a).
//!
//! Every routine-run lifecycle transition the UI needs is emitted on ONE Tauri
//! channel, [`ROUTINES_EVENT`] (`"routines:event"`), as a serialized
//! [`RoutinesEvent`]. The frontend (plan 5) listens on that channel and
//! switches on the `kind` discriminant.
//!
//! ## Bridge honesty — what v1 emits, and what it does NOT
//!
//! The `tuxlink-routines` engine *journals* every step (`StepIntent`/`StepOk`/
//! `StepErr`/`StateChanged`) but it does NOT emit Tauri events — it is a
//! Tauri-free leaf crate. Task 5a bridges the engine to the UI at the RUN
//! boundary only: [`super::session::RoutinesState::start_routine`] emits
//! [`RoutinesEvent::RunStarted`], and a per-run watcher task emits
//! [`RoutinesEvent::RunFinished`] when the run's `done` handle resolves.
//!
//! Step-level granularity ([`RoutinesEvent::StepCompleted`] /
//! [`RoutinesEvent::StateChanged`]) is a variant on the wire NOW so plan 5's
//! listener can be written against the full shape, but v1 does not emit them —
//! step-by-step UI updates come from the frontend polling the run journal via
//! the `routines_run_status` / `routines_journal` commands (plan Task 6). This
//! is a deliberate "keep it simple and honest" choice over tailing the journal
//! file from a background task (spec §8, §11: the journal is the source of
//! truth; the event stream is a best-effort UI nudge).
//!
//! [`RoutinesEvent::AwaitingConsent`] likewise exists on the wire now but is
//! emitted by slice 5b (the consent enforcement wrapper), not this slice.
//!
//! ## Serde shape
//!
//! `#[serde(tag = "kind", rename_all = "camelCase")]` produces an
//! externally-tagged enum: the discriminant is a `"kind"` field alongside the
//! variant fields. Per the project's serde pitfall (`rename_all` on an enum
//! renames variant TAGS, NOT struct-variant fields), every multi-word field
//! carries an explicit `#[serde(rename = "…")]` so it ships camelCase
//! (`runId`, `stepId`, `dryRun`) — the shape tests below are the regression
//! guard, mirroring the `elmer::events` precedent.
//!
//! `state` is a [`RunState`]: its snake_case value tags (`completed`,
//! `interrupted`, `awaiting_consent`, …) are the engine's own terminology, so
//! reusing the type keeps the wire contract single-sourced rather than
//! restating the state names as bare strings.

use serde::Serialize;

use tuxlink_routines::journal::RunState;

/// The single Tauri channel every routine-run lifecycle event is emitted on.
/// The frontend listens with `tauriListen(ROUTINES_EVENT, …)` and switches on
/// `payload.kind`.
pub const ROUTINES_EVENT: &str = "routines:event";

/// A routine-run lifecycle event. One channel, discriminated by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RoutinesEvent {
    /// A run has started (snapshot resolved, journal opened). Emitted by
    /// [`super::session::RoutinesState::start_routine`] immediately after the
    /// engine hands back a `RunHandle`.
    RunStarted {
        #[serde(rename = "runId")]
        run_id: String,
        /// The routine's name (`RoutineDef.routine`).
        routine: String,
        /// Whether this run is a dry-run (plan 3). Always `false` in v1 —
        /// the field is on the wire now so plan 3 needs no shape change.
        #[serde(rename = "dryRun")]
        dry_run: bool,
    },
    /// A run reached a terminal state. Emitted by the per-run watcher task when
    /// the run's `done` handle resolves, and by launch recovery for interrupted
    /// runs. `reason` is populated only where the emitter has one in hand
    /// (recovery); the watcher path leaves it `None` because the engine's
    /// `RunOutcome` carries only the state — the verbatim terminal reason lives
    /// in the journal and surfaces via `routines_journal` (plan Task 6).
    RunFinished {
        #[serde(rename = "runId")]
        run_id: String,
        state: RunState,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// A run changed run-state (e.g. `Running` → `AwaitingRadio`). NOT emitted
    /// in v1 (see module doc); on the wire for plan 5.
    StateChanged {
        #[serde(rename = "runId")]
        run_id: String,
        state: RunState,
    },
    /// A single step finished (ok or errored). NOT emitted in v1 (see module
    /// doc); on the wire for plan 5.
    StepCompleted {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "stepId")]
        step_id: String,
        ok: bool,
    },
    /// A transmitting step in attended mode is parked awaiting operator consent.
    /// The variant exists now; slice 5b (the consent wrapper) is what emits it.
    AwaitingConsent {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "stepId")]
        step_id: String,
    },
}

/// Side-effect sink the session emits routine events into (the `ft8::events`
/// and `AprsState` `EventSink` precedent). Production: Tauri `AppHandle::emit`,
/// fire-and-forget; tests: a recording fake that captures every event.
pub trait RoutinesEventSink: Send + Sync {
    fn emit(&self, event: &RoutinesEvent);
}

/// Production sink: Tauri `AppHandle::emit` on [`ROUTINES_EVENT`],
/// fire-and-forget (the `modem:status` precedent — a failed emit is a
/// UI-absent condition, never a service error).
pub struct TauriRoutinesEventSink {
    pub app: tauri::AppHandle,
}

impl TauriRoutinesEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl RoutinesEventSink for TauriRoutinesEventSink {
    fn emit(&self, event: &RoutinesEvent) {
        use tauri::Emitter as _;
        let _ = self.app.emit(ROUTINES_EVENT, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_serializes_with_camelcase_fields() {
        let e = RoutinesEvent::RunStarted {
            run_id: "run-1-0000".into(),
            routine: "morning-ics".into(),
            dry_run: false,
        };
        let j: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(j["kind"], "runStarted");
        assert_eq!(j["runId"], "run-1-0000", "frontend reads payload.runId");
        assert_eq!(j["routine"], "morning-ics");
        assert_eq!(j["dryRun"], false, "frontend reads payload.dryRun");
        // Regression guard: no snake_case leakage.
        assert!(j.get("run_id").is_none());
        assert!(j.get("dry_run").is_none());
    }

    #[test]
    fn run_finished_serializes_state_as_snake_case_tag_and_omits_absent_reason() {
        let e = RoutinesEvent::RunFinished {
            run_id: "run-1-0000".into(),
            state: RunState::Completed,
            reason: None,
        };
        let j: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(j["kind"], "runFinished");
        assert_eq!(j["runId"], "run-1-0000");
        assert_eq!(j["state"], "completed");
        assert!(
            j.get("reason").is_none(),
            "absent reason must be omitted, not null"
        );
    }

    #[test]
    fn run_finished_interrupted_carries_reason() {
        let e = RoutinesEvent::RunFinished {
            run_id: "run-dead".into(),
            state: RunState::Interrupted,
            reason: Some("process terminated underneath this run".into()),
        };
        let j: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(j["state"], "interrupted");
        assert_eq!(j["reason"], "process terminated underneath this run");
    }

    #[test]
    fn step_completed_and_awaiting_consent_ship_camelcase_step_id() {
        let sc = RoutinesEvent::StepCompleted {
            run_id: "r".into(),
            step_id: "s1".into(),
            ok: true,
        };
        let j: serde_json::Value = serde_json::to_value(&sc).unwrap();
        assert_eq!(j["kind"], "stepCompleted");
        assert_eq!(j["stepId"], "s1", "frontend reads payload.stepId");
        assert!(j.get("step_id").is_none());

        let ac = RoutinesEvent::AwaitingConsent {
            run_id: "r".into(),
            step_id: "s2".into(),
        };
        let j: serde_json::Value = serde_json::to_value(&ac).unwrap();
        assert_eq!(j["kind"], "awaitingConsent");
        assert_eq!(j["stepId"], "s2");
    }

    #[test]
    fn state_changed_serializes_awaiting_radio_state() {
        let e = RoutinesEvent::StateChanged {
            run_id: "r".into(),
            state: RunState::AwaitingRadio,
        };
        let j: serde_json::Value = serde_json::to_value(&e).unwrap();
        assert_eq!(j["kind"], "stateChanged");
        assert_eq!(j["state"], "awaiting_radio");
    }
}
