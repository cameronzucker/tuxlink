//! Redacted run-bundle export (plan-5 Task 4, spec §11 "definition snapshot +
//! run journal + engine context in one file").
//!
//! ## Redaction is an export-boundary property, not a journal property
//!
//! On-screen views (the run detail panel, the journal viewer) show the
//! journal RAW — verbatim `StepErr.cause` text is the whole point of a
//! diagnostic surface. This module is the one export boundary that redacts:
//! [`export_run_bundle`] runs the WHOLE assembled bundle through
//! [`redact_json`] before it ever touches disk. Redaction is field-name
//! keyed ([`crate::logging::redact::should_redact_field`]), never
//! content-guessing — a `cause` string that happens to mention "password"
//! survives untouched because `cause` is not a blocklisted field name; a
//! `password` field is scrubbed regardless of what it holds. This mirrors
//! the logging subsystem's existing redaction discipline (`logging/redact.rs`)
//! rather than inventing a second blocklist.
//!
//! `redact.rs` exports no separate "[REDACTED]" replacement constant (as of
//! this writing it only exports `should_redact_field`; its own tests inline
//! `<redacted>` as a comment example but never a `const`), so this module
//! defines its own literal replacement string.
//!
//! ## Open verifications this task closed (see task-4 brief)
//!
//! - `RunEvent::RunStarted`'s definition-snapshot field is named `snapshot`
//!   (`tuxlink-routines/src/journal.rs:43-48`).
//! - `RoutinesState::journal_entries` already refuses a traversing run id
//!   (`session.rs`'s `valid_run_id` gate) and returns `None` for both "does
//!   not exist" and "would traverse" — both map to [`UiError::NotFound`]
//!   here, same as [`super::commands::run_journal`].
//! - `RadioArbiter::operator_take(rig: &str) -> bool` (`arbiter.rs:837`) is
//!   the graceful "ask nicely" half of the take-the-radio flow; it is used
//!   verbatim by `routines_take_radio`'s shim in `commands.rs`.
//! - `super::actions::DEFAULT_RIG_ID` is `pub(crate)`, which is visible from
//!   `commands.rs` (same crate, different module) without any visibility
//!   change.

use serde::Serialize;
use serde_json::Value;

use tuxlink_routines::journal::RunEvent;

use super::session::RoutinesState;
use crate::logging::redact::should_redact_field;
use crate::ui_commands::UiError;

/// Where the bundle landed and how big it is — enough for the UI to show a
/// "saved to <path> (<n> KB)" confirmation without re-`stat`-ing the file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleResult {
    pub path: String,
    pub bytes: u64,
}

/// Export one run's full record as a single pretty-printed JSON file:
/// `kind`/`schemaVersion` (bundle format identity), `exportedAt`/`appVersion`
/// (engine context), `runId`, `definitionSnapshot` (the resolved definition
/// the run actually executed — `RunStarted.snapshot`, spec §7), and `journal`
/// (every entry, verbatim before redaction). Redaction is applied to the
/// WHOLE assembled bundle as the last step before serialization — see the
/// module doc.
///
/// Unknown run id → [`UiError::NotFound`]. `output_path` is overwritten if it
/// already exists (mirrors `logging_export`'s `std::fs::write` — this is a
/// one-shot export product to an operator-chosen path, not a routines-module
/// managed file, so it does not go through `atomic_write`, same precedent as
/// `logging/export.rs`'s `build_archive`). Any I/O failure maps to
/// [`UiError::Internal`] with the verbatim OS error message.
pub fn export_run_bundle(
    state: &RoutinesState,
    run_id: &str,
    output_path: &str,
) -> Result<BundleResult, UiError> {
    let entries = state
        .journal_entries(run_id)
        .ok_or_else(|| UiError::NotFound(run_id.to_string()))?;
    let snapshot = entries.iter().find_map(|e| match &e.event {
        RunEvent::RunStarted { snapshot, .. } => Some(snapshot.clone()),
        _ => None,
    });
    let mut bundle = serde_json::json!({
        "kind": "tuxlink-routine-run-bundle",
        "schemaVersion": 1,
        "exportedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "runId": run_id,
        "definitionSnapshot": snapshot,
        "journal": serde_json::to_value(&entries)
            .map_err(|e| UiError::Internal { detail: e.to_string() })?,
    });
    redact_json(&mut bundle); // export boundary ONLY — on-screen stays raw.

    let bytes = serde_json::to_vec_pretty(&bundle)
        .map_err(|e| UiError::Internal { detail: e.to_string() })?;
    std::fs::write(output_path, &bytes).map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;

    Ok(BundleResult {
        path: output_path.to_string(),
        bytes: bytes.len() as u64,
    })
}

/// The replacement placed at a redacted field's value. Not a secret shape
/// itself — just a human-legible marker that something was removed.
const REDACTED: &str = "[REDACTED]";

/// Walks a JSON value's objects and arrays; any object field whose NAME
/// matches [`should_redact_field`] has its value replaced with
/// `"[REDACTED]"`. Field-name keyed, recursive, content-blind: a `cause`
/// string is never inspected for what it says, only object keys are ever
/// tested.
pub(crate) fn redact_json(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if should_redact_field(key) {
                    *val = Value::String(REDACTED.to_string());
                } else {
                    redact_json(val);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::arbiter::RadioArbiter;
    use crate::routines::commands::{run_routine, run_status, save_routine};
    use crate::routines::events::{RoutinesEvent, RoutinesEventSink};
    use crate::routines::session::build_routines_state;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tuxlink_routines::action::ActionRegistry;
    use tuxlink_routines::fakes::FakeAction;
    use tuxlink_routines::journal::RunState;

    #[test]
    fn redact_json_scrubs_sensitive_fields_recursively_and_leaves_the_rest_verbatim() {
        let mut v = serde_json::json!({
            "password": "hunter2",
            "step": {"params": {"stations": "@station-set:or-gateways", "api_key": "abc"}},
            "cause": "VARA HF: DISCONNECTED — link timeout 90 s"
        });
        redact_json(&mut v);
        assert_ne!(v["password"], "hunter2");
        assert_ne!(v["step"]["params"]["api_key"], "abc");
        assert_eq!(v["step"]["params"]["stations"], "@station-set:or-gateways");
        assert_eq!(
            v["cause"], "VARA HF: DISCONNECTED — link timeout 90 s",
            "verbatim causes survive — redaction is field-name keyed, not content-guessing"
        );
    }

    fn fixed_now() -> i64 {
        1_752_400_000
    }

    /// A no-op sink — this module's tests only care about the journal on
    /// disk, not the live event stream.
    struct NullSink;
    impl RoutinesEventSink for NullSink {
        fn emit(&self, _event: &RoutinesEvent) {}
    }

    /// A state over a tempdir config with one inert action (`local.log`) —
    /// enough to run a real routine to completion and export its journal.
    fn test_state() -> (tempfile::TempDir, Arc<RoutinesState>) {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        let sink: Arc<dyn RoutinesEventSink> = Arc::new(NullSink);
        let state = Arc::new(build_routines_state(
            dir.path().to_path_buf(),
            reg,
            Arc::new(RadioArbiter::new(fixed_now)),
            sink,
        ));
        (dir, state)
    }

    fn def_json(name: &str) -> String {
        json!({
            "routine": name,
            "schema_version": 1,
            "transmit_mode": "attended",
            "triggers": [{"type": "manual"}],
            "tracks": [{"name": "t", "steps": [
                {"id": "s1", "action": "local.log", "params": {}},
                {"id": "e1", "control": "end"}
            ]}]
        })
        .to_string()
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

    #[tokio::test]
    async fn export_run_bundle_writes_runid_snapshot_journal_and_context() {
        let (_dir, state) = test_state();
        save_routine(&state, &def_json("quick")).unwrap();
        let run_id = run_routine(&state, "quick", json!({})).await.unwrap();
        wait_until(|| run_status(&state, &run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;

        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("bundle.json");
        let out_path_str = out_path.to_str().unwrap().to_string();

        let result = export_run_bundle(&state, &run_id, &out_path_str).unwrap();
        assert_eq!(result.path, out_path_str);
        assert!(result.bytes > 0, "a written bundle is never zero bytes");
        assert!(out_path.exists(), "the file is created at output_path");

        let contents = std::fs::read_to_string(&out_path).unwrap();
        let bundle: Value = serde_json::from_str(&contents).unwrap();

        assert_eq!(bundle["kind"], "tuxlink-routine-run-bundle");
        assert_eq!(bundle["schemaVersion"], 1);
        assert_eq!(bundle["runId"], run_id);
        assert!(
            bundle["exportedAt"].as_str().is_some(),
            "exportedAt is a timestamp string"
        );
        assert!(
            bundle["appVersion"].as_str().is_some(),
            "appVersion is present"
        );
        assert_eq!(
            bundle["definitionSnapshot"]["routine"], "quick",
            "the definition snapshot is RunStarted's resolved snapshot"
        );

        let live_entries = state.journal_entries(&run_id).unwrap();
        let bundled_journal = bundle["journal"].as_array().unwrap();
        assert_eq!(
            bundled_journal.len(),
            live_entries.len(),
            "every journal entry is present in the bundle"
        );
        assert!(
            bundled_journal
                .iter()
                .any(|e| e["event"]["type"] == "run_started"),
            "RunStarted is in the bundled journal"
        );
        assert!(
            bundled_journal
                .iter()
                .any(|e| e["event"]["type"] == "run_finished"),
            "RunFinished is in the bundled journal"
        );

        // Overwrite allowed: exporting again to the same path succeeds.
        export_run_bundle(&state, &run_id, &out_path_str)
            .expect("re-exporting to the same path overwrites, not errors");
    }

    #[test]
    fn export_run_bundle_refuses_an_unknown_run_id() {
        let (_dir, state) = test_state();
        let out_dir = tempfile::tempdir().unwrap();
        let out_path = out_dir.path().join("bundle.json");
        assert!(matches!(
            export_run_bundle(&state, "run-does-not-exist", out_path.to_str().unwrap()),
            Err(UiError::NotFound(_))
        ));
    }
}
