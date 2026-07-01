//! Elmer ↔ TypeScript event contract (Task 8b, tuxlink-13v2l).
//!
//! This file is the **single source of truth** for the four Elmer event
//! channels. The TypeScript pane listens on these string constants via
//! `tauriListen(EV_TURN, ...)` etc.
//!
//! ## Serde shape
//!
//! All variants use `#[serde(tag = "kind", rename_all = "camelCase")]`
//! so the TS listener receives `{ kind: "turn", role: "...", text: "..." }` etc.
//! Fields that would collide with the `"kind"` discriminant key are suffixed
//! `_kind` in Rust (e.g. `outcome_kind` → `"outcomeKind"`, `delta_kind` →
//! `"deltaKind"`) — see individual variant docs for the exact wire names.

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

/// An incremental streaming delta from the model (reasoning or answer content).
///
/// Emitted before the finalizing [`EV_TURN`] event that carries the complete text.
/// The frontend accumulates these chunks to render a live typing effect.
pub const EV_DELTA: &str = "elmer-delta";

/// Context-window usage from a native local provider, for the fullness meter
/// above the composer.  Bridged from `RunEvent::ContextUsage` in `session.rs`.
/// Only emitted on the native Ollama path where `num_ctx` is known.
pub const EV_CONTEXT: &str = "elmer-context";

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
        ///
        /// The explicit `rename` is REQUIRED: `rename_all = "camelCase"` on an
        /// enum renames the variant tags, NOT the fields inside struct variants
        /// (that needs `rename_all_fields`). Without this, the field shipped as
        /// snake_case `outcome_kind`, the frontend's `payload.outcomeKind` was
        /// `undefined`, and every run fell through to the `'error'` phase —
        /// rendering the answer text in an error callout (the "raw box").
        #[serde(rename = "outcomeKind")]
        outcome_kind: String,
        /// Detail string (operator-facing, already redacted at the session layer).
        detail: String,
    },
    /// An incremental streaming delta from the model.
    ///
    /// Emitted on [`EV_DELTA`] during a streamed provider turn — before the
    /// finalizing [`ElmerEvent::Turn`] that carries the complete assembled text.
    ///
    /// On the wire (`elmer-delta` channel) the payload is:
    /// `{ "kind": "delta", "deltaKind": "assistant" | "reasoning", "chunk": "..." }`
    ///
    /// The `delta_kind` field is named with the `_kind` suffix (matching the
    /// `outcome_kind` precedent) to avoid collision with the internally-tagged
    /// enum's `"kind"` discriminant key.  TypeScript receives it as `"deltaKind"`.
    Delta {
        /// Sub-type of the delta: `"assistant"` for answer text, `"reasoning"` for
        /// extended-thinking content.
        ///
        /// Named `delta_kind` (not `kind`) to avoid collision with the serde
        /// `tag = "kind"` discriminant.  TypeScript receives this as `"deltaKind"`.
        ///
        /// The explicit `rename` is REQUIRED — see the `outcome_kind` note above:
        /// enum-level `rename_all` does not reach struct-variant fields, so
        /// without this the field would ship as snake_case `delta_kind` and the
        /// frontend's `payload.deltaKind` would be `undefined`.
        #[serde(rename = "deltaKind")]
        delta_kind: String,
        /// The incremental text chunk for this delta event.
        chunk: String,
    },
    /// Context-window usage from a native local provider, for the fullness
    /// meter above the composer.  Bridged from `RunEvent::ContextUsage`.
    /// Only emitted on the native Ollama path (known `num_ctx` denominator).
    ///
    /// On the wire (`elmer-context` channel) the payload is:
    /// `{ "kind": "context", "promptTokens": N, "evalTokens": N, "numCtx": N }`
    ///
    /// ## Per-field `rename` — REQUIRED (mirrors `outcome_kind` / `delta_kind` precedent)
    ///
    /// `#[serde(rename_all = "camelCase")]` on the `ElmerEvent` enum renames
    /// **variant tags only**, not the fields inside struct variants (that would
    /// need `rename_all_fields`).  Without explicit per-field renames, multi-word
    /// fields ship as snake_case (`prompt_tokens`, etc.) and the frontend's
    /// `payload.promptTokens` is `undefined`.  See the `outcome_kind` → `outcomeKind`
    /// and `delta_kind` → `deltaKind` precedents for prior instances of this fix.
    Context {
        /// Total prompt tokens for this turn (from Ollama `prompt_eval_count`).
        ///
        /// Explicit `rename` required — see variant doc.
        #[serde(rename = "promptTokens")]
        prompt_tokens: u32,
        /// Generated tokens for this turn (from Ollama `eval_count`).
        ///
        /// Explicit `rename` required — see variant doc.
        #[serde(rename = "evalTokens")]
        eval_tokens: u32,
        /// The context-window size that was set for this request (`options.num_ctx`).
        /// Used as the denominator for the fullness meter.
        ///
        /// Explicit `rename` required — see variant doc.
        #[serde(rename = "numCtx")]
        num_ctx: u32,
    },
}

// ---------------------------------------------------------------------------
// Serde shape tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `ElmerEvent::Turn` serializes with the `"kind"` discriminant tag and
    /// camelCase fields — this is the established contract the frontend already
    /// depends on; the Delta tests below must not break this shape.
    #[test]
    fn turn_serializes_with_kind_discriminant() {
        let event = ElmerEvent::Turn {
            role: "assistant".to_string(),
            text: "hello".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "turn");
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["text"], "hello");
    }

    /// `ElmerEvent::Delta` serializes to the flat payload the EV_DELTA frontend
    /// listener will receive:
    ///   `{ "kind": "delta", "deltaKind": "assistant" | "reasoning", "chunk": "..." }`
    ///
    /// Note: `delta_kind` → `"deltaKind"` (camelCase rename) to avoid collision
    /// with the internally-tagged enum's `"kind"` discriminant.
    #[test]
    fn delta_assistant_serializes_flat() {
        let event = ElmerEvent::Delta {
            delta_kind: "assistant".to_string(),
            chunk: "Hello ".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "delta", "discriminant must be 'delta'");
        assert_eq!(json["deltaKind"], "assistant", "sub-type must be 'assistant'");
        assert_eq!(json["chunk"], "Hello ");
        // The payload must be a flat object — no nested wrapper.
        assert!(json.is_object(), "payload must be a flat object");
        assert_eq!(
            json.as_object().unwrap().len(),
            3,
            "expected exactly 3 keys: kind, deltaKind, chunk"
        );
    }

    #[test]
    fn delta_reasoning_serializes_flat() {
        let event = ElmerEvent::Delta {
            delta_kind: "reasoning".to_string(),
            chunk: "thinking…".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "delta");
        assert_eq!(json["deltaKind"], "reasoning");
        assert_eq!(json["chunk"], "thinking…");
    }

    /// `ElmerEvent::Outcome` must ship its `_kind` field as the camelCase
    /// `outcomeKind` the frontend reads. Regression guard: when the rename was
    /// missing the field shipped snake_case, `payload.outcomeKind` was undefined,
    /// and every run fell through to the `'error'` phase (rendering the answer in
    /// an error callout). This test would have caught that.
    #[test]
    fn outcome_serializes_with_camelcase_outcome_kind() {
        let event = ElmerEvent::Outcome {
            outcome_kind: "done".to_string(),
            detail: "the answer".to_string(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "outcome");
        assert_eq!(json["outcomeKind"], "done", "frontend reads payload.outcomeKind");
        assert!(
            json.get("outcome_kind").is_none(),
            "must NOT ship snake_case outcome_kind"
        );
        assert_eq!(json["detail"], "the answer");
    }

    /// `ElmerEvent::Context` must serialize to the flat payload the EV_CONTEXT
    /// frontend listener will receive:
    ///   `{ "kind": "context", "promptTokens": N, "evalTokens": N, "numCtx": N }`
    ///
    /// Regression guard for the per-field rename discipline: without explicit
    /// `#[serde(rename = "...")]` on each multi-word field, `rename_all =
    /// "camelCase"` on the enum only renames the variant tag, and the fields
    /// ship as snake_case (`prompt_tokens`, etc.).  The frontend's
    /// `payload.promptTokens` would then be `undefined`.  This test asserts both
    /// that the camelCase keys ARE present and that the snake_case keys are ABSENT.
    #[test]
    fn context_serializes_with_camelcase_fields() {
        let event = ElmerEvent::Context {
            prompt_tokens: 1024,
            eval_tokens: 256,
            num_ctx: 32768,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();

        // Discriminant tag
        assert_eq!(json["kind"], "context", "discriminant must be 'context'");

        // camelCase field names MUST be present (what the frontend reads)
        assert_eq!(json["promptTokens"], 1024, "frontend reads payload.promptTokens");
        assert_eq!(json["evalTokens"], 256, "frontend reads payload.evalTokens");
        assert_eq!(json["numCtx"], 32768, "frontend reads payload.numCtx");

        // snake_case field names MUST be absent (regression guard for missing rename)
        assert!(
            json.get("prompt_tokens").is_none(),
            "must NOT ship snake_case prompt_tokens"
        );
        assert!(
            json.get("eval_tokens").is_none(),
            "must NOT ship snake_case eval_tokens"
        );
        assert!(
            json.get("num_ctx").is_none(),
            "must NOT ship snake_case num_ctx"
        );

        // Payload must be a flat object with exactly 4 keys: kind + 3 fields
        assert!(json.is_object(), "payload must be a flat object");
        assert_eq!(
            json.as_object().unwrap().len(),
            4,
            "expected exactly 4 keys: kind, promptTokens, evalTokens, numCtx"
        );
    }
}
