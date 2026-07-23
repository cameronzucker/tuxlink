//! Versioned workflow-definition manifest + loader for Elmer's multi-phase
//! "build me a routine" workflow (Routine CI slice 1a, Task 2).
//!
//! Spec: `docs/superpowers/plans/2026-07-22-routine-ci-slice-1a.md`, Task 2.
//! A [`WorkflowManifest`] is the static, on-disk description of one workflow
//! (e.g. the Task 10 `resources/workflows/build-routine.manifest.json`): its
//! entry/exit phases, the inputs it needs, which tool families the emit
//! phase may call, which deterministic gates run as "Routine CI," and eval
//! metadata a later task's scorer consumes. [`load_manifest`] reads +
//! parses + version-gates one manifest file.
//!
//! **Field-count note (judgment call — flag for review):** the brief (plan
//! Task 2, Step 3) names 11 fields verbatim — `schema_version, name,
//! version, entry, exit, required_inputs, optional_inputs,
//! allowed_tool_families, expected_artifacts, deterministic_gates,
//! failure_escalation` — then lists 4 more (`compatible_capability_versions,
//! eval_scenarios, known_model_compat, traceable_outputs`) with the
//! instruction "fold provenance fields into a `provenance` sub-object if
//! cleaner." Folding those 4 into 1 `provenance` field on top of the
//! already-11-item first list yields 12 top-level wire fields, not 11 — the
//! two counts in the brief do not fully reconcile (flattening all 15 instead
//! would defeat the brief's own fold instruction). This impl keeps the
//! first 11 flat, exactly as named, and adds `provenance:
//! WorkflowProvenance` as a 12th field bundling the remaining 4. None of
//! the brief's required tests touch the provenance fields, so this choice
//! does not affect test correctness; flagging here (and in the Task 2
//! report) for parent/operator review in case a different fold was
//! intended.
//!
//! `entry`/`exit` are typed [`PhaseName`] (defined in `super::artifacts`,
//! Task 1) rather than bare `String` — the manifest names real pipeline
//! phases, so reusing the existing enum catches a typo'd phase name at
//! parse time instead of at first `engine.rs` dispatch. [`Depth`] (also
//! Task 1) is re-exported here rather than redefined — it is NOT a
//! `WorkflowManifest` field (the router, Task 8, selects it per-run, not
//! the manifest), but the Task 2 interface list names it as produced by
//! this module, so downstream code can reach it as
//! `workflow::manifest::Depth` alongside `WorkflowManifest`.
//!
//! Convention mirrors the `ActionInfo` DTO pattern in
//! `crate::routines::commands`: `Debug, Clone, PartialEq, Serialize,
//! Deserialize` plus `#[serde(rename_all = "camelCase")]`. The
//! `schema_version` version-gate shape (a single supported version,
//! reject-if-unrecognized) follows `crate::config`'s `CONFIG_SCHEMA_VERSION`
//! idiom, adapted to return a typed `ManifestError::UnsupportedVersion`
//! (see [`load_manifest`]'s doc comment for why the gate is a post-parse
//! branch here rather than a `#[serde(deserialize_with = ...)]` hook like
//! `config.rs`'s `deserialize_schema_version`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::artifacts::PhaseName;

/// Re-exported from [`super::artifacts`] (Task 1) rather than redefined —
/// see the module doc's note on why `Depth` lives here even though it is
/// not a `WorkflowManifest` field.
pub use super::artifacts::Depth;

/// The only `schema_version` this binary accepts. Routine CI slice 1a has a
/// single manifest generation; [`load_manifest`] rejects any other value
/// via `ManifestError::UnsupportedVersion` — a typed reject-if-unrecognized
/// check rather than best-effort loading of a manifest shape this binary
/// cannot vouch for (mirrors the posture of `crate::config::detect_schema_action`'s
/// `Unsupported` arm, without that function's additive-migration arms since
/// slice 1a has nothing to migrate from).
pub const WORKFLOW_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The static, on-disk description of one Elmer workflow.
/// `deny_unknown_fields` so a manifest referencing a field a newer build
/// added (and this older build doesn't know) fails loudly instead of
/// silently dropping it — same rationale as `Config`'s
/// `deny_unknown_fields` in `config.rs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowManifest {
    /// Gated in [`load_manifest`] against
    /// [`WORKFLOW_MANIFEST_SCHEMA_VERSION`] (currently `1`); any other
    /// value produces `ManifestError::UnsupportedVersion`.
    pub schema_version: u32,
    /// The workflow's stable identifier (e.g. `"build-routine"`) — the name
    /// a caller looks the manifest resource file up by.
    pub name: String,
    /// The workflow definition's own semver, independent of
    /// `schema_version` (the wire-shape version): bumping `version` marks a
    /// behavior change to the workflow's phases/prompts, not a shape
    /// change to the manifest file itself.
    pub version: String,
    /// The pipeline phase a run starts at.
    pub entry: PhaseName,
    /// The pipeline phase a successful run ends at.
    pub exit: PhaseName,
    /// Named input keys the intent MUST supply before a run starts.
    pub required_inputs: Vec<String>,
    /// Named input keys a run may supply but does not require.
    pub optional_inputs: Vec<String>,
    /// Which `ActionInfo` tool families (`routines::commands::list_actions`
    /// output) the emit phase's tool set is filtered to, per the plan's
    /// "Affordance catalog" constraint. A workflow that only builds
    /// DISABLED/attended routine drafts never includes `"transmit"` here —
    /// asserted by this module's `allowed_tool_families` fixture test.
    pub allowed_tool_families: Vec<String>,
    /// Which typed phase artifacts (`Intent`, `Affordances`, `Draft`,
    /// `CiReport`, `Present`, ...) a run is expected to capture.
    pub expected_artifacts: Vec<String>,
    /// Named deterministic checks "Routine CI" runs against the draft
    /// before a run can go Green.
    pub deterministic_gates: Vec<String>,
    /// Free-form escalation policy for a Red CI verdict. Defined but
    /// unexercised in slice 1a (no auto-repair loop yet — the plan scopes
    /// "Slice 1b (CI repair loop) ... OUT of scope"); kept as
    /// `serde_json::Value` rather than a typed shape so a later slice can
    /// add structure without another `schema_version` bump.
    pub failure_escalation: serde_json::Value,
    /// Eval/compat metadata about this manifest — grouped into one
    /// sub-object per the module doc's field-count note.
    pub provenance: WorkflowProvenance,
}

/// Eval/compat metadata folded out of [`WorkflowManifest`]'s flat field list
/// (see the module doc's field-count note). `deny_unknown_fields` for the
/// same reason as the parent struct.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowProvenance {
    /// `tuxlink_routines` capability-surface versions this manifest was
    /// authored/validated against.
    pub compatible_capability_versions: Vec<String>,
    /// Identifiers of the eval-corpus scenarios (Task 12 fixtures) this
    /// workflow is scored against.
    pub eval_scenarios: Vec<String>,
    /// Models this workflow is known to run acceptably on (the cross-model
    /// viability experiment's tracked roster).
    pub known_model_compat: Vec<String>,
    /// Which `WorkflowRun` outputs are expected to be traceable back to a
    /// specific phase record for audit (e.g. `"savedRoutine"`).
    pub traceable_outputs: Vec<String>,
}

/// Errors [`load_manifest`] can return. Shape mirrors `crate::config`'s
/// `ConfigReadError` (`NotFound` / `Io` / `Serde`), plus the
/// manifest-specific `UnsupportedVersion` schema gate the brief's test
/// asserts against.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("workflow manifest not found at {path}")]
    NotFound { path: PathBuf },
    #[error("io error reading workflow manifest {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("workflow manifest deserialize failed: {source}")]
    Serde {
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported workflow manifest schema_version {0} (this binary supports v1)")]
    UnsupportedVersion(u32),
}

/// Read + parse + version-gate the workflow manifest at `path`.
///
/// The `schema_version` gate runs here, post-parse, rather than via a
/// `#[serde(deserialize_with = ...)]` field hook like `config.rs`'s
/// `deserialize_schema_version`: that hook can only raise a generic
/// `serde::de::Error` (a string), but the brief's contract needs the typed
/// `ManifestError::UnsupportedVersion(found)` its test asserts against, so
/// the check is a plain branch after `serde_json` has already parsed
/// `schema_version` as an ordinary `u32` field.
pub fn load_manifest(path: &Path) -> Result<WorkflowManifest, ManifestError> {
    let bytes = std::fs::read(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            ManifestError::NotFound { path: path.to_path_buf() }
        } else {
            ManifestError::Io { path: path.to_path_buf(), source }
        }
    })?;
    let manifest: WorkflowManifest =
        serde_json::from_slice(&bytes).map_err(|source| ManifestError::Serde { source })?;
    if manifest.schema_version != WORKFLOW_MANIFEST_SCHEMA_VERSION {
        return Err(ManifestError::UnsupportedVersion(manifest.schema_version));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A valid manifest fixture (parameterized only on `schema_version` so
    /// the unsupported-version test reuses the same shape), embedded inline
    /// per the brief. `entry`/`exit` use real `PhaseName` values;
    /// `allowedToolFamilies` carries `"routines"` and deliberately omits
    /// `"transmit"` — Part-97: the emit phase never gets a transmit-capable
    /// tool (plan §Global Constraints).
    fn fixture_json(schema_version: u32) -> String {
        format!(
            r#"{{
                "schemaVersion": {schema_version},
                "name": "build-routine",
                "version": "1.0.0",
                "entry": "router",
                "exit": "present",
                "requiredInputs": ["outcome"],
                "optionalInputs": ["priorRoutineId"],
                "allowedToolFamilies": ["routines"],
                "expectedArtifacts": ["intent", "affordances", "draft", "ciReport", "present"],
                "deterministicGates": ["structure", "validate"],
                "failureEscalation": {{ "onRed": "quarantineDraft" }},
                "provenance": {{
                    "compatibleCapabilityVersions": ["routines-v1"],
                    "evalScenarios": ["task1", "task2", "task3"],
                    "knownModelCompat": ["stub"],
                    "traceableOutputs": ["savedRoutine"]
                }}
            }}"#
        )
    }

    fn write_fixture(schema_version: u32) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("create temp manifest file");
        file.write_all(fixture_json(schema_version).as_bytes())
            .expect("write fixture manifest");
        file
    }

    #[test]
    fn load_manifest_reads_v1_fixture_with_expected_tool_families() {
        let file = write_fixture(1);
        let manifest = load_manifest(file.path()).expect("v1 fixture loads");
        assert_eq!(manifest.schema_version, 1);
        assert!(manifest.allowed_tool_families.iter().any(|f| f == "routines"));
        assert!(!manifest.allowed_tool_families.iter().any(|f| f == "transmit"));
    }

    #[test]
    fn load_manifest_rejects_unsupported_schema_version() {
        let file = write_fixture(99);
        let err = load_manifest(file.path()).expect_err("v99 fixture must be rejected");
        assert!(matches!(err, ManifestError::UnsupportedVersion(99)));
    }

    #[test]
    fn load_manifest_returns_not_found_for_missing_path() {
        let missing = std::env::temp_dir().join("tuxlink-workflow-manifest-does-not-exist.json");
        let err = load_manifest(&missing).expect_err("missing path must error");
        assert!(matches!(err, ManifestError::NotFound { .. }));
    }
}
