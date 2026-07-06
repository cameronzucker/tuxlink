//! Pure rendering of the run outcome for the terminal (T8).
//!
//! Kept pure (returns Strings) so the formatting is unit-testable without
//! capturing stdout; `main` just prints what these return.
//!
//! NOTE on "the transcript": the runner's [`tuxlink_agent_runner::run`] owns the
//! [`tuxlink_agent_runner::Conversation`] internally and returns only the
//! terminal [`RunOutcome`], so the frontend cannot render the full transcript
//! after the fact. Live per-tool progress is streamed to stderr by the
//! [`crate::uds::UdsToolInvoker`] as each tool runs (a tracing-style echo); the
//! final model answer is carried by [`RunOutcome::Completed`] and rendered here.

use tuxlink_agent_runner::RunOutcome;

/// Render the terminal [`RunOutcome`] as an operator-facing summary line.
pub fn render_outcome(outcome: &RunOutcome) -> String {
    match outcome {
        RunOutcome::Completed(text) => format!("✓ completed: {text}"),
        RunOutcome::NeedsOperator(reason) => format!("⏸ needs operator: {reason}"),
        RunOutcome::InvalidAction(detail) => format!("✗ invalid action: {detail}"),
        RunOutcome::Cancelled => "■ cancelled".to_string(),
        RunOutcome::ToolDenied(reason) => format!("⛔ tool denied: {reason}"),
        RunOutcome::RateLimited(reason) => format!("⏳ rate limited: {reason}"),
        RunOutcome::ProviderError(detail) => format!("✗ provider error: {detail}"),
    }
}

/// Render the terminal [`RunOutcome`] as a single-line machine-readable JSON
/// object `{"kind","text"}` (tuxlink-cnz5o, Task 6). The Python grounding judge
/// grades the `text` of a `"completed"` outcome as the agent's final answer; the
/// other kinds carry their reason/detail as `text` so a non-completed run is
/// still classifiable. `kind` is a stable lowercase tag, one per `RunOutcome`
/// variant.
pub fn render_outcome_json(outcome: &RunOutcome) -> String {
    let (kind, text): (&str, &str) = match outcome {
        RunOutcome::Completed(text) => ("completed", text),
        RunOutcome::NeedsOperator(reason) => ("needs_operator", reason),
        RunOutcome::InvalidAction(detail) => ("invalid_action", detail),
        RunOutcome::Cancelled => ("cancelled", ""),
        RunOutcome::ToolDenied(reason) => ("denied", reason),
        RunOutcome::RateLimited(reason) => ("rate_limited", reason),
        RunOutcome::ProviderError(detail) => ("provider_error", detail),
    };
    // serde_json handles the escaping; a compact single-line object is emitted so
    // one run = one greppable line.
    serde_json::json!({ "kind": kind, "text": text }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_outcome_variant() {
        assert!(render_outcome(&RunOutcome::Completed("done".into())).contains("completed"));
        assert!(render_outcome(&RunOutcome::NeedsOperator("turns".into())).contains("needs operator"));
        assert!(render_outcome(&RunOutcome::InvalidAction("bad".into())).contains("invalid action"));
        assert!(render_outcome(&RunOutcome::Cancelled).contains("cancelled"));
        assert!(render_outcome(&RunOutcome::ToolDenied("tainted".into())).contains("denied"));
        assert!(render_outcome(&RunOutcome::ProviderError("HTTP 400".into())).contains("provider error"));
    }

    #[test]
    fn json_outcome_is_parseable_with_kind_and_text() {
        let line = render_outcome_json(&RunOutcome::Completed("W1AW is on 7104".into()));
        // Single line — no embedded newline.
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(v["kind"], "completed");
        assert_eq!(v["text"], "W1AW is on 7104");
    }

    #[test]
    fn json_outcome_tags_denied() {
        let line = render_outcome_json(&RunOutcome::ToolDenied("not armed".into()));
        let v: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(v["kind"], "denied");
        assert_eq!(v["text"], "not armed");
    }

    #[test]
    fn json_outcome_covers_every_variant() {
        for outcome in [
            RunOutcome::Completed("a".into()),
            RunOutcome::NeedsOperator("b".into()),
            RunOutcome::InvalidAction("c".into()),
            RunOutcome::Cancelled,
            RunOutcome::ToolDenied("d".into()),
            RunOutcome::RateLimited("e".into()),
            RunOutcome::ProviderError("f".into()),
        ] {
            let line = render_outcome_json(&outcome);
            let v: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
            assert!(v.get("kind").and_then(|k| k.as_str()).is_some());
            assert!(v.get("text").is_some());
        }
    }
}
