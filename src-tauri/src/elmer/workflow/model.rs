//! The model port for Elmer's multi-phase "build me a routine" workflow
//! (Routine CI slice 1a, Task 3).
//!
//! `PhaseModel` is the workflow engine's (Task 6) seam onto a model: one call
//! = one phase's worth of model reasoning, given a rendered prompt and the
//! tool schemas available to that phase. It is deliberately a NARROWER port
//! than `tuxlink_agent_runner::Provider` — the workflow engine does not run
//! the bounded agent loop's tool-call dispatch itself; each phase issues (at
//! most) one model call and gets back a [`PhaseTurn`] carrying the raw
//! `final_text` for the phase to parse against its own artifact schema.
//!
//! **Dyn-compatibility.** The engine holds this as `&dyn PhaseModel`, and a
//! native `async fn` in a trait is not dyn-compatible on this crate's MSRV
//! (1.75). We use `#[async_trait::async_trait]` to match how
//! `tuxlink_agent_runner::traits::ToolInvoker` (also held as
//! `Box<dyn ToolInvoker>` / `&dyn ToolInvoker`) declares its own async
//! methods — same crate (`async-trait`) already in `src-tauri/Cargo.toml`,
//! no new dependency.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use tuxlink_agent_runner::{RunOutcome, ToolCall, ToolSpec};

/// One model turn within a workflow phase.
///
/// Distinct from `tuxlink_agent_runner::types::ModelTurn` (a `Provider`'s
/// per-turn output inside the bounded agent loop): `PhaseTurn` is the
/// workflow engine's own unit of model output, carrying the loop-style
/// [`RunOutcome`] the phase decides against plus the raw text the phase
/// parses, so a phase can inspect both without the engine reimplementing the
/// agent loop's outcome vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseTurn {
    /// How the phase's model call resolved.
    pub outcome: RunOutcome,
    /// Tool calls the model requested during this turn, if any.
    pub tool_calls: Vec<ToolCall>,
    /// The model's raw final text (e.g. the JSON a phase parses into its
    /// artifact). Populated from `outcome`'s `Completed` payload for the
    /// common case; kept as a separate field so a phase can read it without
    /// re-matching on `outcome`.
    pub final_text: String,
    /// Prompt tokens the model call consumed, as reported by the provider.
    pub prompt_tokens: u64,
}

impl PhaseTurn {
    /// Convenience constructor for the common case: a plain completed text
    /// turn with no tool calls. `outcome` is `RunOutcome::Completed(s)`.
    pub fn text(s: &str, toks: u64) -> Self {
        Self {
            outcome: RunOutcome::Completed(s.to_string()),
            tool_calls: Vec::new(),
            final_text: s.to_string(),
            prompt_tokens: toks,
        }
    }
}

/// The model port a workflow phase runs its reasoning through.
///
/// Object-safe (`#[async_trait]`) and `Send + Sync` so the engine (Task 6)
/// can hold it as `&dyn PhaseModel`, mirroring `ToolInvoker` /
/// `Provider` in `tuxlink_agent_runner::traits`.
#[async_trait]
pub trait PhaseModel: Send + Sync {
    /// Run one phase's model call: `prompt` is the fully-rendered phase
    /// prompt, `tools` are the tool schemas available to this phase (may be
    /// empty for phases that never call tools).
    async fn run_phase(&self, prompt: String, tools: &[ToolSpec]) -> PhaseTurn;
}

/// A scripted [`PhaseModel`] for tests: returns pre-loaded [`PhaseTurn`]s in
/// FIFO order and records every prompt it was given, so a test (including the
/// workflow-engine invariant tests in Task 6) can assert what the engine
/// actually sent.
pub struct StubModel {
    turns: Mutex<VecDeque<PhaseTurn>>,
    prompts_seen: Mutex<Vec<String>>,
}

impl StubModel {
    /// Build a stub scripted to return `turns` in order, one per
    /// `run_phase` call.
    pub fn new(turns: Vec<PhaseTurn>) -> Self {
        Self {
            turns: Mutex::new(turns.into_iter().collect()),
            prompts_seen: Mutex::new(Vec::new()),
        }
    }

    /// The prompts passed to `run_phase`, in call order.
    pub fn prompts_seen(&self) -> Vec<String> {
        self.prompts_seen.lock().expect("prompts_seen mutex poisoned").clone()
    }
}

#[async_trait]
impl PhaseModel for StubModel {
    async fn run_phase(&self, prompt: String, _tools: &[ToolSpec]) -> PhaseTurn {
        self.prompts_seen
            .lock()
            .expect("prompts_seen mutex poisoned")
            .push(prompt);
        self.turns
            .lock()
            .expect("turns mutex poisoned")
            .pop_front()
            .expect("StubModel called more times than it was scripted for")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_model_returns_scripted_turn_and_records_prompt() {
        let stub = StubModel::new(vec![PhaseTurn::text("{\"outcome\":\"x\"}", 12)]);
        let turn = stub.run_phase("PROMPT-A".into(), &[]).await;
        assert_eq!(turn.final_text, "{\"outcome\":\"x\"}");
        assert_eq!(turn.prompt_tokens, 12);
        assert_eq!(stub.prompts_seen(), vec!["PROMPT-A".to_string()]);
    }
}
