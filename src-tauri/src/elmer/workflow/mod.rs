//! Reusable mechanical pieces salvaged from the discarded "Routine CI"
//! workflow engine (bd tuxlink-t3jci). The hidden multi-phase cognition
//! (engine / phases / model / manifest / router / present / scorers) was torn
//! out; what remains is the deterministic harness the new agent-driven
//! routine-authoring scaffold equips the agent with:
//!
//! - `catalog`: the deterministic affordance-catalog builder.
//! - `ci`: the routine-validator wrapper (deterministic, no model call).
//! - `artifacts`: the two value types those two produce
//!   ([`Affordances`] / [`AffordanceAction`], [`CiReport`] / [`CiVerdict`] /
//!   [`CiFinding`]).

pub mod artifacts;
pub mod catalog;
pub mod ci;

pub use artifacts::{AffordanceAction, Affordances, CiFinding, CiReport, CiVerdict};
pub use catalog::{build_affordance_catalog, CatalogError};
pub use ci::run_routine_ci;
