//! Core value types for the bounded agent loop (T1).
//!
//! These are transport-agnostic: a `Provider` adapter maps a model's wire format
//! onto [`ModelTurn`], and a `ToolInvoker` adapter maps [`ToolCall`] onto its own
//! tool transport. The loop in [`crate::runner`] only ever sees these types.

use serde::{Deserialize, Serialize};

/// Authority under which the runner invokes a tool.
///
/// **SEC-3.** This enum has EXACTLY ONE variant, [`CallAuthority::Agent`]. The
/// runner therefore has no code path that can construct an `Operator`-authority
/// call: an `Operator` call returns `Ok` before any arm/taint check (see
/// `tuxlink-security`'s `decide`), so letting the runner mint one would let an
/// automated loop bypass the operator-consent gate. By making the only
/// constructible value `Agent`, that bypass is impossible *by construction*,
/// not merely by policy.
///
/// This crate deliberately does NOT import `tuxlink-security::EgressAuthority`;
/// the runner only needs to *mean* "Agent", and the real authority mapping
/// happens in the frontend `ToolInvoker` adapter (d3zwe / Elmer), which is the
/// only layer that touches the security crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CallAuthority {
    /// An automated agent loop. The ONLY variant — see the type docs.
    #[default]
    Agent,
}

/// A tool the model may call: its name and the JSON Schema its arguments must
/// satisfy. The schema is supplied by the `ToolInvoker` (it knows the real tool
/// surface) and used by the loop to validate calls before dispatch (COR-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Tool name as the model addresses it.
    pub name: String,
    /// JSON Schema (draft-07-style object) for the tool's arguments.
    pub json_schema: serde_json::Value,
}

impl ToolSpec {
    /// Convenience constructor.
    pub fn new(name: impl Into<String>, json_schema: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            json_schema,
        }
    }
}

/// A single tool invocation the model emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Name of the tool to invoke. May not match any [`ToolSpec`] (unknown-tool
    /// is a recoverable malformed turn — see COR-3).
    pub name: String,
    /// Raw arguments the model produced, validated against the matching
    /// [`ToolSpec::json_schema`] before dispatch.
    pub args: serde_json::Value,
}

impl ToolCall {
    /// Convenience constructor.
    pub fn new(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            args,
        }
    }
}

/// Outcome of a single tool invocation, as the `ToolInvoker` reports it.
///
/// `Denied` is the relay of an authority/taint refusal from below the MCP
/// boundary (the runner does not decide it — the security layer does). The loop
/// treats a `Denied` as terminal: it surfaces a [`RunOutcome::ToolDenied`].
///
/// `Cancelled` signals that the invoker observed cooperative cancellation mid-call
/// (e.g. the CancellationToken fired before or during dispatch). The runner treats
/// it as terminal and returns [`RunOutcome::Cancelled`] without pushing the outcome
/// into the conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutcome {
    /// The tool ran and produced a (already-curated) JSON result.
    Ok(serde_json::Value),
    /// The tool was refused (not armed / expired / tainted / not authorized).
    /// Carries an operator-facing reason relayed from the security layer.
    Denied(String),
    /// The invoker itself rejected the arguments (a second line of defence
    /// behind the loop's own schema check). Carries the validation detail.
    InvalidArgs(String),
    /// The invocation was cooperatively cancelled before or during execution.
    /// Carries a short reason for logging. The runner does NOT push this into
    /// the conversation — it returns [`RunOutcome::Cancelled`] immediately.
    Cancelled(String),
}

/// A turn the model produced: either a final text answer, or one-or-more tool
/// calls to execute before the next turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTurn {
    /// A final assistant message; the loop returns [`RunOutcome::Completed`].
    Text(String),
    /// Tool calls to dispatch; the loop runs them, appends results, re-prompts.
    ToolCalls(Vec<ToolCall>),
}

/// Hard bounds on a single `run`. Defaults are conservative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of tool-executing turns before the loop stops and returns
    /// [`RunOutcome::NeedsOperator`] (COR-1). Default 10.
    pub max_tool_turns: u32,
    /// Per-turn wall-clock timeout for a single Provider call (COR-1). A turn
    /// that exceeds it is treated as exhaustion → [`RunOutcome::NeedsOperator`].
    pub per_turn_timeout: std::time::Duration,
    /// Maximum consecutive malformed-call retries within one turn before the
    /// loop gives up with [`RunOutcome::InvalidAction`] (COR-3). Default 2.
    pub max_malformed_retries: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tool_turns: 10,
            per_turn_timeout: std::time::Duration::from_secs(120),
            max_malformed_retries: 2,
        }
    }
}

/// A structured run-progress event emitted by [`crate::run_with_conversation`]
/// as the loop advances, so a caller can surface a conversational transcript
/// (assistant turns + tool-call chips) instead of only the terminal outcome.
///
/// This is a deliberately small, transport-agnostic type: the runner does NOT
/// depend on Elmer's `ElmerEvent` or Tauri. The Elmer session bridges these
/// into its own event channel (see `src/elmer/session.rs`).
///
/// The events are **fire-and-forget**: the callback that receives them must
/// not gate the loop, change any [`RunOutcome`], or affect cancellation /
/// timeout / COR invariants. A panicking callback would unwind through the
/// loop, so callers keep the callback trivial (a channel send / event emit).
///
/// `AssistantDelta` / `ReasoningDelta` are **incremental** streaming events a
/// streaming [`crate::Provider`] MAY emit *during* a turn (token-by-token), so a
/// caller can show inference progress live. They are additive to — not a
/// replacement for — `AssistantText`: a streaming turn emits zero-or-more
/// `ReasoningDelta` / `AssistantDelta` while the answer is produced, and the
/// loop still emits the single finalizing `AssistantText` before returning
/// [`RunOutcome::Completed`]. A consumer accumulates the deltas for live
/// rendering and treats `AssistantText` as the authoritative final answer.
/// Non-streaming providers emit no deltas, so callers that ignore them keep
/// working unchanged.
///
/// This enum is `#[non_exhaustive]`: future phases may add further streaming
/// variants, so external matches must carry a catch-all arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunEvent {
    /// The model produced its final assistant text (emitted just before the
    /// loop returns [`RunOutcome::Completed`]).
    AssistantText {
        /// The assistant's answer text.
        text: String,
    },
    /// The model invoked a tool. Emitted once per tool call, carrying the
    /// tool's name (the human-facing chip label).
    ToolCall {
        /// The tool name as the model addressed it.
        tool: String,
    },
    /// An incremental piece of the model's final answer, emitted as content
    /// streams in (before the finalizing [`RunEvent::AssistantText`]). Each
    /// chunk is a partial fragment, not a complete message — accumulate them.
    AssistantDelta {
        /// A partial fragment of the final answer text.
        chunk: String,
    },
    /// An incremental piece of the model's reasoning / "thinking" channel,
    /// emitted as it streams in. Models such as gpt-oss surface reasoning on a
    /// channel separate from the answer; this variant carries those fragments
    /// so a caller can render a live thinking trace distinct from the answer.
    ReasoningDelta {
        /// A partial fragment of the model's reasoning text.
        chunk: String,
    },
}

/// The terminal result of a `run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The model produced a final text answer.
    Completed(String),
    /// A bound was hit (turns or per-turn timeout) before finishing; the
    /// operator should decide whether to continue. Carries a reason (COR-1).
    NeedsOperator(String),
    /// The model could not produce a schema-valid tool call within the retry
    /// budget. Carries the last validation detail (COR-3).
    InvalidAction(String),
    /// The run was cancelled cooperatively (COR-2).
    Cancelled,
    /// A tool invocation was refused by the security layer (relayed). Carries
    /// the refusal reason.
    ToolDenied(String),
}
