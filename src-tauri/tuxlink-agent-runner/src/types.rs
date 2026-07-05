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
    /// Opaque, provider-supplied metadata that MUST be echoed back verbatim on
    /// the next request for a multi-turn tool loop to succeed. The runner never
    /// interprets it — it only carries it through the conversation so the
    /// provider adapter can round-trip it.
    ///
    /// This exists for Gemini 3.x "thinking" models, whose OpenAI-compat layer
    /// returns a per-tool-call `extra_content.google.thought_signature` and
    /// rejects any follow-up turn whose assistant `tool_calls[]` omit it (HTTP
    /// 400 INVALID_ARGUMENT). The adapter stores the whole `extra_content` object
    /// here on the way in and re-emits it on the way out; other providers leave
    /// it `None`. Kept provider-neutral (a raw `Value`) so the transport-agnostic
    /// runner never grows a Gemini-specific concept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_meta: Option<serde_json::Value>,
}

impl ToolCall {
    /// Convenience constructor. `provider_meta` defaults to `None`; use
    /// [`ToolCall::with_provider_meta`] to attach round-trip metadata.
    pub fn new(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            args,
            provider_meta: None,
        }
    }

    /// Attach opaque provider round-trip metadata (see [`ToolCall::provider_meta`]).
    #[must_use]
    pub fn with_provider_meta(mut self, provider_meta: Option<serde_json::Value>) -> Self {
        self.provider_meta = provider_meta;
        self
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
    /// The context-window usage reported by a native local provider after a
    /// turn. Emitted (fire-and-forget) by a `Provider` — currently the native
    /// Ollama `/api/chat` adapter — once per turn when the model reports token
    /// counts, so a caller can render a context-fullness meter against the
    /// `num_ctx` the provider requested.
    ///
    /// Like the other variants this is purely informational: the runner relays
    /// it unchanged and never interprets the counts. It is only meaningful when
    /// `num_ctx` is known (i.e. the native path where the app sets the window);
    /// providers that leave the window at the server default do not emit it. A
    /// model that omits token counts simply causes no emission, so a caller that
    /// hides its meter until the first event degrades gracefully.
    ContextUsage {
        /// Tokens the full prompt occupied this turn (Ollama `prompt_eval_count`).
        prompt_tokens: u32,
        /// Tokens the model generated this turn (Ollama `eval_count`).
        eval_tokens: u32,
        /// The context window the provider requested (`options.num_ctx`), the
        /// denominator for the fullness meter.
        num_ctx: u32,
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
    /// The provider returned HTTP 429 — the endpoint is temporarily throttling
    /// requests.  Carries an already-redacted detail string.  The frontend maps
    /// this to the `"rateLimited"` outcome-kind event so the React pane can show
    /// the rate-limit callout.  No automatic retry is performed.
    RateLimited(String),
    /// The provider call FAILED (transport error, non-2xx HTTP other than 429, or
    /// an unparseable response) — distinct from [`RunOutcome::NeedsOperator`], which
    /// is a soft "you hit a bound, continue?" gate. Carries an already-redacted
    /// detail string. The frontend maps this to the persisted `"error"` outcome-kind
    /// so a model error (a Gemini tool-call 400, a 404/500, a dead endpoint) lands in
    /// the transcript verbatim instead of a single-slot callout the next run
    /// overwrites (tuxlink-a1xwx).
    ProviderError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `provider_meta` (Gemini thought_signature carrier, tuxlink-0tuc3) is skipped
    /// on serialization when `None` — so existing persisted conversations and
    /// non-Gemini turns round-trip byte-identically — and round-trips when present.
    #[test]
    fn tool_call_provider_meta_serde_round_trips() {
        // Absent: the field must NOT appear in the serialized form.
        let plain = ToolCall::new("find_stations", serde_json::json!({ "grid": "DM79" }));
        let plain_json = serde_json::to_value(&plain).unwrap();
        assert!(
            plain_json.get("provider_meta").is_none(),
            "provider_meta=None must skip serialization; got {plain_json}"
        );
        // A serialized form WITHOUT the field still deserializes (serde default).
        let decoded: ToolCall = serde_json::from_value(
            serde_json::json!({ "name": "x", "args": {} }),
        )
        .unwrap();
        assert_eq!(decoded.provider_meta, None);

        // Present: round-trips verbatim.
        let meta = serde_json::json!({ "google": { "thought_signature": "SIG" } });
        let withmeta = ToolCall::new("x", serde_json::json!({}))
            .with_provider_meta(Some(meta.clone()));
        let round: ToolCall =
            serde_json::from_value(serde_json::to_value(&withmeta).unwrap()).unwrap();
        assert_eq!(round.provider_meta, Some(meta));
    }

    /// The `ContextUsage` variant constructs with the documented fields and
    /// relays its counts unchanged through a fire-and-forget sink (the same
    /// contract the runner uses for every other `RunEvent`). Guards T2: adding
    /// the variant must not change how a caller reads the counts back out.
    #[test]
    fn context_usage_variant_constructs_and_relays_counts() {
        let event = RunEvent::ContextUsage {
            prompt_tokens: 1234,
            eval_tokens: 56,
            num_ctx: 32_768,
        };

        // A fire-and-forget sink records the event verbatim.
        let mut seen: Vec<RunEvent> = Vec::new();
        let mut sink = |e: RunEvent| seen.push(e);
        sink(event.clone());

        assert_eq!(seen.len(), 1);
        match &seen[0] {
            RunEvent::ContextUsage {
                prompt_tokens,
                eval_tokens,
                num_ctx,
            } => {
                assert_eq!(*prompt_tokens, 1234);
                assert_eq!(*eval_tokens, 56);
                assert_eq!(*num_ctx, 32_768);
            }
            other => panic!("expected ContextUsage, got {other:?}"),
        }

        // Equality is field-wise (derived `PartialEq`), so a re-constructed
        // value with the same fields compares equal — the relay is lossless.
        assert_eq!(seen[0], event);
    }
}
