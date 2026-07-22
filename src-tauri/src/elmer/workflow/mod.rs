//! Elmer's multi-phase "build me a routine" workflow (Routine CI slice 1a).
//!
//! - Task 1 (`artifacts`): the typed phase artifacts — the type contract
//!   every later task builds against.
//! - Task 2 (`manifest`): the versioned workflow-definition manifest + loader.
//! - Task 3 (`model`): the `PhaseModel` port + `StubModel` test double.
//! - Task 4 (`catalog`): the deterministic affordance-catalog builder.
//! - Task 5 (`ci`): the Routine CI wrapper over the routines validator.

pub mod artifacts;
pub mod catalog;
pub mod ci;
pub mod manifest;
pub mod model;
pub mod phases;
pub mod present;
pub mod router;

pub use artifacts::{
    AffordanceAction, Affordances, CiFinding, CiReport, CiVerdict, Depth, Draft, DraftBranch,
    DraftNode, Intent, PhaseName, PhaseRecord, Present, WorkflowRun,
};
pub use catalog::{build_affordance_catalog, CatalogError};
pub use ci::run_routine_ci;
// `Depth` is re-exported above from `artifacts`; `manifest` re-exports it too
// internally, so list only manifest-specific items here to avoid a duplicate.
pub use manifest::{
    load_manifest, ManifestError, WorkflowManifest, WorkflowProvenance,
    WORKFLOW_MANIFEST_SCHEMA_VERSION,
};
pub use model::{PhaseModel, PhaseTurn, StubModel};
pub use phases::{build_prompt, capture_artifact, tools_for, CapturedArtifact, PhaseError};
pub use present::build_present;
pub use router::{score_depth, select_depth};
