//! Agent-native `find_stations` boundary types (bd tuxlink-m0n38).
//!
//! This module houses the **tool-boundary** shapes for the redesigned
//! `find_stations` MCP tool: the intent-tagged request enum, the
//! snapshot+population response envelope with its tagged `result` union, and the
//! bounded primitive newtypes that make the spec's invariant true *by
//! construction* — a `find_stations` call can never emit output fatal to (or
//! silently misleading to) the agent.
//!
//! **Why these types live in `tuxlink-mcp-core`, not the app crate.** The
//! [`crate::ports::StationPort`] trait and the rmcp `#[tool]` macro that
//! advertises the tool's input/output JSON Schema both live here, and this crate
//! cannot see app-crate types (the dependency flows app → mcp-core, never back).
//! The types that cross the trait boundary must therefore be defined alongside
//! it. The impl that *builds* a response from live app state (`AppHandle`,
//! catalog, FT-8, connection history) — the `StationQueryEngine` — stays in the
//! app crate; this module only defines the wire shapes and their bounds.
//!
//! Design source of truth (do not re-derive):
//! `docs/superpowers/specs/2026-07-23-find-stations-agent-native-redesign.md`.

pub mod bounded;

pub use bounded::{BoundedU8, BoundedVec, CapExceeded, CappedString, OutOfRange};
