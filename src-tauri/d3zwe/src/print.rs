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
}
