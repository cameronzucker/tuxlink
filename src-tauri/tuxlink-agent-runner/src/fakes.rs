//! Test fakes that exercise the whole loop in CI with no network and no MCP
//! (TEST-1). Mirrors `tuxlink-security`'s `EgressGuard::with_clock`
//! fake-injection pattern: deterministic, in-process, no I/O.
//!
//! These are compiled into the library (not gated behind `#[cfg(test)]`) so the
//! `d3zwe` frontend's own integration tests and future Elmer tests can reuse
//! them. They contain no production logic.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::traits::{Provider, ProviderError, ToolInvoker};
use crate::types::{CallAuthority, ModelTurn, ToolCall, ToolOutcome, ToolSpec};

/// A side-effect hook run at the start of every scripted `turn`.
type OnTurnHook = Box<dyn Fn() + Send + Sync>;

/// A side-effect hook run at the start of every recorded `invoke`, given the
/// in-flight cancellation token.
type OnInvokeHook = Box<dyn Fn(&CancellationToken) + Send + Sync>;

/// A [`Provider`] that replays a pre-set sequence of turns. Each call to
/// [`Provider::turn`] returns the next scripted item; running past the end
/// yields a [`ProviderError`] (a script that under-supplies turns is a test
/// bug, surfaced loudly rather than hanging).
pub struct ScriptedProvider {
    turns: Mutex<std::vec::IntoIter<ScriptedTurn>>,
    /// Number of times `turn` has been called (COR-2 assertions).
    calls: AtomicUsize,
    /// Optional async hook to run *inside* `turn`, before producing the turn.
    /// Used to simulate a slow or cancel-racing model.
    on_turn: Option<OnTurnHook>,
}

/// One scripted model turn, or an injected provider error.
#[derive(Debug, Clone)]
pub enum ScriptedTurn {
    /// Yield a model turn.
    Turn(ModelTurn),
    /// Yield a transport error.
    Error(String),
}

impl ScriptedProvider {
    /// Build from a sequence of model turns.
    pub fn new(turns: Vec<ModelTurn>) -> Self {
        let scripted = turns.into_iter().map(ScriptedTurn::Turn).collect::<Vec<_>>();
        Self::from_scripted(scripted)
    }

    /// Build from a sequence that may include injected errors.
    pub fn from_scripted(turns: Vec<ScriptedTurn>) -> Self {
        Self {
            turns: Mutex::new(turns.into_iter()),
            calls: AtomicUsize::new(0),
            on_turn: None,
        }
    }

    /// Attach a side-effect run at the start of every `turn` (e.g. cancel a
    /// token to simulate a mid-flight cancellation).
    pub fn with_on_turn(mut self, hook: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_turn = Some(Box::new(hook));
        self
    }

    /// How many times `turn` has been invoked.
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    async fn turn(
        &self,
        _conversation: &Conversation,
        _tools: &[ToolSpec],
    ) -> Result<ModelTurn, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = &self.on_turn {
            hook();
        }
        let next = self.turns.lock().unwrap().next();
        match next {
            Some(ScriptedTurn::Turn(t)) => Ok(t),
            Some(ScriptedTurn::Error(e)) => Err(ProviderError::Transport(e)),
            None => Err(ProviderError::Transport(
                "ScriptedProvider exhausted: the test script supplied too few turns".to_string(),
            )),
        }
    }
}

/// A recorded tool invocation (for assertions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCall {
    pub call: ToolCall,
    pub authority: CallAuthority,
}

/// A [`ToolInvoker`] that records every call and returns scripted outcomes.
///
/// **SEC-3 enforcement.** `invoke` asserts (panics) if it ever receives an
/// authority other than [`CallAuthority::Agent`]. Because the runner has no way
/// to construct any other variant, this assertion can never fire in production
/// — it is a belt-and-suspenders guard that turns any future regression into a
/// loud test failure.
pub struct RecordingInvoker {
    tools: Vec<ToolSpec>,
    /// Outcomes keyed by call order; runs out → default `Ok({})`.
    outcomes: Mutex<std::vec::IntoIter<ToolOutcome>>,
    calls: Mutex<Vec<RecordedCall>>,
    /// Optional hook to observe/await/cancel inside `invoke` (COR-2 tests).
    on_invoke: Option<OnInvokeHook>,
}

impl RecordingInvoker {
    /// Build with the given tool specs and a queue of outcomes (consumed in
    /// call order). When the queue empties, further calls return `Ok({})`.
    pub fn new(tools: Vec<ToolSpec>, outcomes: Vec<ToolOutcome>) -> Self {
        Self {
            tools,
            outcomes: Mutex::new(outcomes.into_iter()),
            calls: Mutex::new(Vec::new()),
            on_invoke: None,
        }
    }

    /// Convenience: an invoker exposing `tools` that always returns `Ok({})`.
    pub fn always_ok(tools: Vec<ToolSpec>) -> Self {
        Self::new(tools, Vec::new())
    }

    /// Attach a side-effect run at the start of every `invoke`.
    pub fn with_on_invoke(
        mut self,
        hook: impl Fn(&CancellationToken) + Send + Sync + 'static,
    ) -> Self {
        self.on_invoke = Some(Box::new(hook));
        self
    }

    /// Number of times `invoke` was called.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    /// Snapshot of all recorded calls.
    pub fn recorded(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Whether every recorded call carried `Agent` authority (SEC-3).
    pub fn all_authorities_agent(&self) -> bool {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.authority == CallAuthority::Agent)
    }
}

#[async_trait]
impl ToolInvoker for RecordingInvoker {
    fn tools(&self) -> &[ToolSpec] {
        &self.tools
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        // SEC-3 belt-and-suspenders: the runner must only ever send Agent.
        assert_eq!(
            authority,
            CallAuthority::Agent,
            "ToolInvoker received non-Agent authority — SEC-3 violation"
        );
        self.calls.lock().unwrap().push(RecordedCall {
            call: call.clone(),
            authority,
        });
        if let Some(hook) = &self.on_invoke {
            hook(cancel);
        }
        let next = self.outcomes.lock().unwrap().next();
        next.unwrap_or(ToolOutcome::Ok(serde_json::json!({})))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn echo_tool() -> ToolSpec {
        ToolSpec::new(
            "echo",
            json!({ "type": "object", "properties": { "msg": { "type": "string" } } }),
        )
    }

    #[tokio::test]
    async fn scripted_provider_replays_in_order() {
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "hi"}))]),
            ModelTurn::Text("done".into()),
        ]);
        let convo = Conversation::new("go");
        let tools = vec![echo_tool()];

        let t1 = provider.turn(&convo, &tools).await.unwrap();
        assert!(matches!(t1, ModelTurn::ToolCalls(_)));
        let t2 = provider.turn(&convo, &tools).await.unwrap();
        assert_eq!(t2, ModelTurn::Text("done".into()));
        assert_eq!(provider.call_count(), 2);
    }

    #[tokio::test]
    async fn scripted_provider_errors_when_exhausted() {
        let provider = ScriptedProvider::new(vec![]);
        let convo = Conversation::new("go");
        let err = provider.turn(&convo, &[]).await.unwrap_err();
        assert!(matches!(err, ProviderError::Transport(_)));
    }

    #[tokio::test]
    async fn scripted_provider_injects_errors() {
        let provider =
            ScriptedProvider::from_scripted(vec![ScriptedTurn::Error("boom".into())]);
        let convo = Conversation::new("go");
        let err = provider.turn(&convo, &[]).await.unwrap_err();
        assert!(matches!(err, ProviderError::Transport(_)));
    }

    #[tokio::test]
    async fn recording_invoker_records_agent_authority() {
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let cancel = CancellationToken::new();
        let call = ToolCall::new("echo", json!({"msg": "hi"}));
        let outcome = invoker
            .invoke(&call, CallAuthority::Agent, &cancel)
            .await;
        assert_eq!(outcome, ToolOutcome::Ok(json!({})));
        assert_eq!(invoker.call_count(), 1);
        assert!(invoker.all_authorities_agent());
        assert_eq!(invoker.recorded()[0].call, call);
    }

    #[tokio::test]
    async fn recording_invoker_returns_scripted_outcomes_in_order() {
        let invoker = RecordingInvoker::new(
            vec![echo_tool()],
            vec![
                ToolOutcome::Ok(json!({"n": 1})),
                ToolOutcome::Denied("nope".into()),
            ],
        );
        let cancel = CancellationToken::new();
        let call = ToolCall::new("echo", json!({}));
        let first = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
        let second = invoker.invoke(&call, CallAuthority::Agent, &cancel).await;
        assert_eq!(first, ToolOutcome::Ok(json!({"n": 1})));
        assert_eq!(second, ToolOutcome::Denied("nope".into()));
    }
}
