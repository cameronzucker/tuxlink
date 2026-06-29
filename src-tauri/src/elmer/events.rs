//! Elmer ↔ TypeScript event contract (Task 8b, tuxlink-13v2l).
//!
//! This file is the **single source of truth** for the three Elmer event
//! channels. The TypeScript pane listens on these string constants via
//! `tauriListen(EV_TURN, ...)` etc.
//!
//! ## Serde shape
//!
//! All three variants use `#[serde(tag = "kind", rename_all = "camelCase")]`
//! so the TS listener receives `{ kind: "turn", role: "...", text: "..." }` etc.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Event-name constants (Rust ↔ TypeScript contract)
// ---------------------------------------------------------------------------

/// Streaming text token from the model (role + text snippet).
pub const EV_TURN: &str = "elmer-turn";

/// A tool was called or returned (tool name + status string).
pub const EV_CHIP: &str = "elmer-chip";

/// The run reached a terminal outcome (kind string + optional detail).
pub const EV_OUTCOME: &str = "elmer-outcome";

// ---------------------------------------------------------------------------
// Event payload
// ---------------------------------------------------------------------------

/// The payload emitted on the three Elmer event channels.
///
/// `#[serde(tag = "kind")]` produces an externally-tagged enum: the
/// discriminant is a `"kind"` field alongside the variant fields. TypeScript
/// can switch on `event.payload.kind`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ElmerEvent {
    /// A text token (or full assistant message) from the model.
    Turn {
        /// `"user"` | `"assistant"` | `"system"`.
        role: String,
        /// The text content of the turn.
        text: String,
    },
    /// A tool was invoked or returned.
    Chip {
        /// The MCP tool name (e.g. `"find_stations"`).
        tool: String,
        /// Human-readable status string (e.g. `"calling"` / `"ok"` / `"denied"`).
        status: String,
    },
    /// The run loop exited with a terminal outcome.
    Outcome {
        /// One of: `"done"` | `"cancelled"` | `"needsOperator"` | `"error"`.
        ///
        /// Named `outcome_kind` (not `kind`) to avoid a serde name collision with
        /// the internally-tagged enum's `"kind"` discriminant field.  TypeScript
        /// receives this as `event.payload.outcomeKind` (camelCase rename).
        outcome_kind: String,
        /// Detail string (operator-facing, already redacted at the session layer).
        detail: String,
    },
}
