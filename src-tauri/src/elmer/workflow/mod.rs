//! Elmer's multi-phase "build me a routine" workflow (Routine CI slice 1a).
//!
//! - Task 1 (`artifacts`): the typed phase artifacts — the type contract
//!   every later task builds against.

pub mod artifacts;

pub use artifacts::{
    AffordanceAction, Affordances, CiFinding, CiReport, CiVerdict, Depth, Draft, DraftBranch,
    DraftNode, Intent, PhaseName, PhaseRecord, Present, WorkflowRun,
};
