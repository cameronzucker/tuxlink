//! # tuxlink-agent-runner
//!
//! A transport-agnostic, bounded agent loop for Tuxlink's "Elmer" assistant
//! spine. It runs a model ([`Provider`]) against a tool surface
//! ([`ToolInvoker`]) to a terminal [`RunOutcome`], with cooperative
//! cancellation and malformed-tool-call recovery. It is fully unit-testable in
//! CI with the shipped fakes ([`ScriptedProvider`] + [`RecordingInvoker`]) — no
//! live model, no live MCP, no Tauri.
//!
//! ## Security invariants (enforced by the trait surface + tests)
//!
//! * **SEC-3 — Agent authority only.** Every tool call the loop makes carries
//!   [`CallAuthority::Agent`]. That enum has a single variant, so the runner has
//!   no code path that can construct an `Operator`-authority call (an `Operator`
//!   call bypasses the arm/taint gate in `tuxlink-security`). A fake invoker
//!   additionally asserts the received authority is always `Agent`.
//!
//! * **SEC-4 — no arm / no clear-taint capability.** The loop holds only a
//!   `&dyn `[`Provider`], a `&dyn `[`ToolInvoker`], and a read-only
//!   [`EgressStatus`] snapshot. None of those can arm send authority or clear
//!   taint — the real `EgressGuard` (with `arm()` / `clear_taint()`) lives in
//!   `tuxlink-security` and **this crate does not depend on it at all** (verified
//!   by the `sec4_no_security_dependency` test below). The loop is therefore
//!   incapable of arming or clearing taint *by construction*, not merely by
//!   convention.
//!
//! * **ARCH-1 / SEC-2 — single canonical tool path.** The [`ToolInvoker`] is the
//!   only way the loop reaches tools; it never reaches below the MCP tool
//!   boundary, so taint / redaction / schema enforcement there are never
//!   bypassed.
//!
//! ## Correctness invariants
//!
//! * **COR-1** — a hard [`Limits::max_tool_turns`] cap and per-turn timeout;
//!   exhaustion returns [`RunOutcome::NeedsOperator`].
//! * **COR-2** — a [`tokio_util::sync::CancellationToken`] is checked before
//!   every Provider call and propagated into each tool invocation.
//! * **COR-3** — each tool call's args are validated against the tool's JSON
//!   schema; on failure the error is fed back and re-prompted, bounded by
//!   [`Limits::max_malformed_retries`], then [`RunOutcome::InvalidAction`].

mod conversation;
mod fakes;
mod runner;
mod traits;
mod types;
mod validate;

pub use conversation::{Conversation, Message};
pub use fakes::{RecordedCall, RecordingInvoker, ScriptedProvider, ScriptedTurn};
pub use runner::{run, run_with_conversation};
pub use traits::{EgressStatus, Provider, ProviderError, ToolInvoker};
pub use types::{
    CallAuthority, Limits, ModelTurn, RunEvent, RunOutcome, ToolCall, ToolOutcome, ToolSpec,
};

// The minimal validator is an implementation detail of COR-3, but exposing it
// lets frontends pre-validate without duplicating the rules.
pub use validate::validate as validate_args;

#[cfg(test)]
mod acceptance_tests {
    //! Cross-cutting acceptance tests for the published security + correctness
    //! invariants (SEC-3, SEC-4, COR-1, COR-2, COR-3). Each test maps to an
    //! acceptance-criterion ID from the plan.

    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    fn echo_tool() -> ToolSpec {
        ToolSpec::new(
            "echo",
            json!({
                "type": "object",
                "required": ["msg"],
                "properties": { "msg": { "type": "string" } }
            }),
        )
    }

    fn fast_limits() -> Limits {
        Limits {
            max_tool_turns: 10,
            per_turn_timeout: Duration::from_secs(5),
            max_malformed_retries: 2,
        }
    }

    // --- Happy path -------------------------------------------------------

    #[tokio::test]
    async fn completes_on_text_turn() {
        let provider = ScriptedProvider::new(vec![ModelTurn::Text("all done".into())]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "hello",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("all done".into()));
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn runs_a_tool_then_completes() {
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "hi"}))]),
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::new(
            vec![echo_tool()],
            vec![ToolOutcome::Ok(json!({"echoed": "hi"}))],
        );
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        assert_eq!(invoker.call_count(), 1);
        // SEC-3: the single call carried Agent authority.
        assert!(invoker.all_authorities_agent());
    }

    #[tokio::test]
    async fn dispatches_multiple_calls_in_one_turn() {
        // A single turn may emit several tool calls; all run in order, then the
        // loop re-prompts.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![
                ToolCall::new("echo", json!({"msg": "a"})),
                ToolCall::new("echo", json!({"msg": "b"})),
            ]),
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        assert_eq!(invoker.call_count(), 2);
        let recorded = invoker.recorded();
        assert_eq!(recorded[0].call.args, json!({"msg": "a"}));
        assert_eq!(recorded[1].call.args, json!({"msg": "b"}));
        assert!(invoker.all_authorities_agent());
    }

    // --- SEC-3 ------------------------------------------------------------

    #[tokio::test]
    async fn sec3_every_call_is_agent_authority() {
        // Several tool turns, then completion; assert ALL recorded authorities
        // are Agent (the RecordingInvoker also panics internally on non-Agent).
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "a"}))]),
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "b"}))]),
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "c"}))]),
            ModelTurn::Text("fin".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("fin".into()));
        assert_eq!(invoker.call_count(), 3);
        assert!(invoker.all_authorities_agent());
        for rec in invoker.recorded() {
            assert_eq!(rec.authority, CallAuthority::Agent);
        }
    }

    #[test]
    fn sec3_call_authority_has_only_agent_variant() {
        // Compile-time-ish guard: the only constructible value is Agent. If a
        // future edit adds an Operator variant, the runner could mint it — this
        // test documents the single-variant invariant the type relies on.
        let a = CallAuthority::Agent;
        let b = CallAuthority::default();
        assert_eq!(a, b);
        assert_eq!(a, CallAuthority::Agent);
    }

    // --- SEC-4 ------------------------------------------------------------

    #[test]
    fn sec4_no_security_dependency() {
        // The runner must not depend on tuxlink-security at all (and so cannot
        // reach arm() / clear_taint()). This is the SEC-4 "does not depend on
        // the mutating API" check.
        //
        // We scan only NON-COMMENT lines of the manifest so that a comment that
        // merely *mentions* the crate (e.g. "deliberately does not depend on
        // tuxlink-security") does not trip the test — only a real dependency
        // declaration must fail it.
        let manifest = include_str!("../Cargo.toml");
        let offending: Vec<&str> = manifest
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.starts_with('#'))
            .filter(|l| {
                // Strip an inline trailing comment before matching.
                let code = match l.split_once('#') {
                    Some((before, _)) => before,
                    None => l,
                };
                code.contains("tuxlink-security") || code.contains("tuxlink_security")
            })
            .collect();
        assert!(
            offending.is_empty(),
            "tuxlink-agent-runner must NOT depend on tuxlink-security (SEC-4): \
             offending manifest lines: {offending:?}"
        );
    }

    #[test]
    fn sec4_egress_status_is_read_only_value() {
        // EgressStatus is a plain Copy value: observing it can never mutate the
        // guard. (If it ever held an Arc<EgressGuard>, this would not be Copy.)
        fn assert_copy<T: Copy>() {}
        assert_copy::<EgressStatus>();
        let s = EgressStatus {
            armed: true,
            tainted: false,
        };
        let _clone = s; // Copy, not move — no shared mutable handle.
        assert!(s.armed);
    }

    // --- COR-1 ------------------------------------------------------------

    #[tokio::test]
    async fn cor1_terminates_at_max_tool_turns() {
        // A provider that emits a (valid) tool call EVERY turn must terminate at
        // the bound, not loop forever.
        let mut script = Vec::new();
        for _ in 0..100 {
            script.push(ModelTurn::ToolCalls(vec![ToolCall::new(
                "echo",
                json!({"msg": "again"}),
            )]));
        }
        let provider = ScriptedProvider::new(script);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let limits = Limits {
            max_tool_turns: 3,
            ..fast_limits()
        };
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            limits,
            CancellationToken::new(),
        )
        .await;
        match outcome {
            RunOutcome::NeedsOperator(reason) => {
                assert!(reason.contains('3'), "reason was: {reason}");
            }
            other => panic!("expected NeedsOperator, got {other:?}"),
        }
        // 3 turns executed before the 4th tripped the bound.
        assert_eq!(invoker.call_count(), 3);
    }

    #[tokio::test]
    async fn cor1_per_turn_timeout_yields_needs_operator() {
        // A provider whose turn never resolves should trip the per-turn timeout.
        struct HangingProvider;
        #[async_trait::async_trait]
        impl Provider for HangingProvider {
            async fn turn(
                &self,
                _c: &Conversation,
                _t: &[ToolSpec],
                _on_event: &(dyn Fn(RunEvent) + Sync),
            ) -> Result<ModelTurn, ProviderError> {
                // Never completes within the timeout window.
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let limits = Limits {
            per_turn_timeout: Duration::from_millis(20),
            ..fast_limits()
        };
        let outcome = run(
            "go",
            &HangingProvider,
            &invoker,
            EgressStatus::default(),
            limits,
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(outcome, RunOutcome::NeedsOperator(_)));
    }

    // --- COR-2 ------------------------------------------------------------

    #[tokio::test]
    async fn cor2_pre_cancelled_makes_no_provider_call() {
        let provider = ScriptedProvider::new(vec![ModelTurn::Text("never".into())]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            cancel,
        )
        .await;
        assert_eq!(outcome, RunOutcome::Cancelled);
        // COR-2: not a single Provider call was started.
        assert_eq!(provider.call_count(), 0);
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn cor2_cancel_mid_script_stops_before_next_provider_call() {
        // The provider cancels the token from inside its FIRST turn (simulating
        // an operator abort arriving while the model is responding). The loop
        // dispatches that turn's tool, then must NOT call the Provider again.
        let cancel = CancellationToken::new();
        let cancel_for_hook = cancel.clone();
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "x"}))]),
            ModelTurn::Text("should-not-reach".into()),
        ])
        .with_on_turn(move || cancel_for_hook.cancel());
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);

        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            cancel,
        )
        .await;
        assert_eq!(outcome, RunOutcome::Cancelled);
        // Exactly ONE provider call (the first); the cancel prevented a second.
        assert_eq!(provider.call_count(), 1);
        // The tool from the first turn never ran: cancel was observed before
        // dispatch (the loop checks cancel before invoking each call).
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn cor2_cancel_during_in_flight_turn_returns_cancelled_promptly() {
        // A Stop arriving WHILE a long Provider turn is in flight must interrupt
        // the in-flight turn and return Cancelled — NOT block until the per-turn
        // timeout. Regression for the unresponsive-Stop bug: before the select!
        // race, a cancel during a hanging turn was only observed at the next loop
        // top, so with a long timeout Stop appeared to do nothing for minutes.
        struct HangingProvider;
        #[async_trait::async_trait]
        impl Provider for HangingProvider {
            async fn turn(
                &self,
                _c: &Conversation,
                _t: &[ToolSpec],
                _on_event: &(dyn Fn(RunEvent) + Sync),
            ) -> Result<ModelTurn, ProviderError> {
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let cancel = CancellationToken::new();
        let cancel_for_stop = cancel.clone();
        // Simulate the operator clicking Stop ~20ms after the run starts — the
        // turn is hanging by then.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel_for_stop.cancel();
        });
        // A LONG per-turn timeout: the test must terminate via the cancel race,
        // NOT via the timeout (without the fix this test would hang ~1h).
        let limits = Limits {
            per_turn_timeout: Duration::from_secs(3600),
            ..fast_limits()
        };
        let outcome = run(
            "go",
            &HangingProvider,
            &invoker,
            EgressStatus::default(),
            limits,
            cancel,
        )
        .await;
        assert_eq!(outcome, RunOutcome::Cancelled);
    }

    #[tokio::test]
    async fn cor2_cancel_token_reaches_invoke() {
        // The token is propagated into invoke(): an invoker hook observes it.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let saw_token = Arc::new(AtomicBool::new(false));
        let saw_clone = saw_token.clone();
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "x"}))]),
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]).with_on_invoke(
            move |tok: &CancellationToken| {
                // Token is live and not yet cancelled — confirms propagation.
                if !tok.is_cancelled() {
                    saw_clone.store(true, Ordering::SeqCst);
                }
            },
        );
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        assert!(saw_token.load(Ordering::SeqCst), "invoke did not receive the token");
    }

    // --- COR-3 ------------------------------------------------------------

    #[tokio::test]
    async fn cor3_malformed_then_valid_completes() {
        // First turn: a schema-invalid call (missing required `msg`). The loop
        // feeds the error back and re-prompts; the model then emits a valid call
        // and finishes.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]), // invalid
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "ok"}))]), // valid
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        // The malformed call was NOT invoked; only the valid one was.
        assert_eq!(invoker.call_count(), 1);
    }

    #[tokio::test]
    async fn cor3_malformed_three_times_returns_invalid_action() {
        // max_malformed_retries = 2 → 3 consecutive malformed turns exhausts.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]),
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]),
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]),
            // A 4th valid turn that must never be reached.
            ModelTurn::Text("should-not-reach".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(outcome, RunOutcome::InvalidAction(_)),
            "expected InvalidAction, got {outcome:?}"
        );
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn cor3_empty_batch_is_malformed_then_recovers() {
        // An empty tool-call batch is a malformed turn (the model said "I'll use
        // tools" but named none); it is recoverable like any other.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![]),
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn cor3_retry_counter_resets_after_recovery() {
        // malformed → valid (resets counter) → malformed → malformed → valid.
        // With budget 2, the counter must reset after the first recovery so the
        // later malformed pair does NOT trip InvalidAction.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]), // malformed
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "1"}))]), // valid
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]), // malformed
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({}))]), // malformed
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "2"}))]), // valid
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        // Two valid tool calls dispatched.
        assert_eq!(invoker.call_count(), 2);
    }

    #[tokio::test]
    async fn cor3_unknown_tool_is_malformed() {
        // Addressing a tool that does not exist is a recoverable malformed turn.
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("nonexistent", json!({}))]),
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "ok"}))]),
            ModelTurn::Text("done".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
        assert_eq!(invoker.call_count(), 1);
    }

    // --- Tool denial ------------------------------------------------------

    #[tokio::test]
    async fn tool_denied_is_terminal() {
        let provider = ScriptedProvider::new(vec![ModelTurn::ToolCalls(vec![
            ToolCall::new("echo", json!({"msg": "x"})),
        ])]);
        let invoker = RecordingInvoker::new(
            vec![echo_tool()],
            vec![ToolOutcome::Denied("session is tainted".into())],
        );
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        match outcome {
            RunOutcome::ToolDenied(reason) => assert!(reason.contains("tainted")),
            other => panic!("expected ToolDenied, got {other:?}"),
        }
        assert_eq!(invoker.call_count(), 1);
    }

    #[tokio::test]
    async fn provider_error_yields_provider_error_outcome() {
        // A failed model call surfaces as ProviderError (a capturable failure), not
        // the soft NeedsOperator gate (tuxlink-a1xwx).
        let provider =
            ScriptedProvider::from_scripted(vec![ScriptedTurn::Error("upstream 503".into())]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(outcome, RunOutcome::ProviderError(_)));
    }

    // --- run_with_conversation / ToolOutcome::Cancelled -------------------

    #[tokio::test]
    async fn run_with_conversation_appends_to_existing_context() {
        let mut convo = Conversation::from_messages(vec![Message::User("first".into()), Message::Assistant("ok".into())]);
        convo.push_user("second");
        let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Turn(ModelTurn::Text("done".into()))]);
        let invoker = RecordingInvoker::new(vec![], vec![]);
        let out = run_with_conversation(&mut convo, &provider, &invoker, EgressStatus::default(), Limits::default(), CancellationToken::new(), &|_| {}).await;
        assert!(matches!(out, RunOutcome::Completed(t) if t == "done"));
        assert!(convo.messages().len() >= 3);
    }

    // --- RunEvent progress callback ---------------------------------------

    #[tokio::test]
    async fn on_event_emits_toolcall_then_assistant_text_in_order() {
        // Drive [ToolCalls(echo), Text("final")] through a recording callback.
        // The callback must observe a ToolCall{tool:"echo"} BEFORE the final
        // AssistantText{text:"final"}, and must NOT change the RunOutcome.
        use std::sync::Mutex;
        let provider = ScriptedProvider::new(vec![
            ModelTurn::ToolCalls(vec![ToolCall::new("echo", json!({"msg": "hi"}))]),
            ModelTurn::Text("final".into()),
        ]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);

        let events: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
        let mut convo = Conversation::new("go");
        let outcome = run_with_conversation(
            &mut convo,
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
            &|ev| events.lock().unwrap().push(ev),
        )
        .await;

        // Callback did not alter the outcome.
        assert_eq!(outcome, RunOutcome::Completed("final".into()));

        let recorded = events.into_inner().unwrap();
        assert_eq!(
            recorded,
            vec![
                RunEvent::ToolCall { tool: "echo".into() },
                RunEvent::AssistantText { text: "final".into() },
            ],
            "callback must see ToolCall(echo) then AssistantText(final) in order"
        );
    }

    #[tokio::test]
    async fn streaming_provider_emits_deltas_before_finalize_without_changing_outcome() {
        // A ScriptedProvider scripted with a `Streamed` turn (reasoning + answer
        // deltas + a final Text turn) is driven through `run_with_conversation`
        // with a recording callback. The recorded sequence must be:
        //   ReasoningDelta(s) → AssistantDelta(s) → AssistantText(final)
        // and the RunOutcome must be unchanged (Completed(final)). This proves
        // deltas stream BEFORE the loop's finalize emit without altering the
        // outcome — the fire-and-forget contract.
        use std::sync::Mutex;
        let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Streamed {
            reasoning: vec!["weighing options".into(), "deciding".into()],
            deltas: vec!["The ".into(), "answer".into()],
            turn: ModelTurn::Text("The answer".into()),
        }]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);

        let events: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
        let mut convo = Conversation::new("go");
        let outcome = run_with_conversation(
            &mut convo,
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
            &|ev| events.lock().unwrap().push(ev),
        )
        .await;

        // The deltas did not alter the terminal outcome.
        assert_eq!(outcome, RunOutcome::Completed("The answer".into()));

        let recorded = events.into_inner().unwrap();
        assert_eq!(
            recorded,
            vec![
                RunEvent::ReasoningDelta { chunk: "weighing options".into() },
                RunEvent::ReasoningDelta { chunk: "deciding".into() },
                RunEvent::AssistantDelta { chunk: "The ".into() },
                RunEvent::AssistantDelta { chunk: "answer".into() },
                RunEvent::AssistantText { text: "The answer".into() },
            ],
            "deltas must stream (reasoning then answer) before the finalize AssistantText"
        );
        // No tools were called: a pure text streaming turn.
        assert_eq!(invoker.call_count(), 0);
    }

    #[tokio::test]
    async fn non_streaming_turn_emits_no_deltas_and_outcome_unchanged() {
        // Guard against regressing the non-streaming path: a plain
        // ScriptedTurn::Turn(Text) emits NO deltas — only the loop's finalizing
        // AssistantText — and the outcome is unchanged.
        use std::sync::Mutex;
        let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Turn(
            ModelTurn::Text("plain".into()),
        )]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);

        let events: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
        let mut convo = Conversation::new("go");
        let outcome = run_with_conversation(
            &mut convo,
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
            &|ev| events.lock().unwrap().push(ev),
        )
        .await;

        assert_eq!(outcome, RunOutcome::Completed("plain".into()));
        let recorded = events.into_inner().unwrap();
        assert_eq!(
            recorded,
            vec![RunEvent::AssistantText { text: "plain".into() }],
            "a non-streaming turn emits only the finalizing AssistantText"
        );
    }

    #[tokio::test]
    async fn run_uses_noop_callback_and_completes() {
        // The thin `run` wrapper passes a no-op callback internally; existing
        // `run` callers/tests are unaffected. This documents that contract.
        let provider = ScriptedProvider::new(vec![ModelTurn::Text("done".into())]);
        let invoker = RecordingInvoker::always_ok(vec![echo_tool()]);
        let outcome = run(
            "go",
            &provider,
            &invoker,
            EgressStatus::default(),
            fast_limits(),
            CancellationToken::new(),
        )
        .await;
        assert_eq!(outcome, RunOutcome::Completed("done".into()));
    }

    #[tokio::test]
    async fn cancelled_tool_outcome_terminates_run_as_cancelled() {
        let provider = ScriptedProvider::from_scripted(vec![ScriptedTurn::Turn(ModelTurn::ToolCalls(vec![ToolCall::new("x", serde_json::json!({}))]))]);
        let invoker = RecordingInvoker::new(
            vec![ToolSpec{name:"x".into(),json_schema:serde_json::json!({"type":"object"})}],
            vec![ToolOutcome::Cancelled("aborted".into())]);
        let out = run("go", &provider, &invoker, EgressStatus::default(), Limits::default(), CancellationToken::new()).await;
        assert!(matches!(out, RunOutcome::Cancelled));
    }
}
