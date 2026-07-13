//! tuxlink-routines — deterministic station automation engine (Routines).
//!
//! Tauri-free leaf crate. Invariants (spec 2026-07-13-routines-design.md):
//! - Every run terminates in an explicit state; steps end ok(output) or
//!   err(verbatim cause) — no third state.
//! - Unset variable references fail the step verbatim; they never resolve
//!   to their own name.
//! - Journal writes are intent-before-effect where possible.
//! - Runs execute a snapshot resolved at run start.
//! - Prior-art engine terminology is banned from this codebase; the feature is Routines.

pub mod action;
pub mod error;
pub mod executor;
pub mod fakes;
pub mod journal;
pub mod refs;
pub mod types;
pub mod vars;
