//! The bounded agent loop (T3 COR-1, T4 COR-2, T5 COR-3).
//!
//! [`run`] drives a [`Provider`] and a [`ToolInvoker`] to a terminal
//! [`RunOutcome`]. It holds ONLY a `&dyn Provider`, a `&dyn ToolInvoker`, and a
//! read-only [`EgressStatus`] — nothing that can arm send authority or clear
//! taint (SEC-4). Every tool call carries [`CallAuthority::Agent`] (SEC-3).
//!
//! Termination is guaranteed: every iteration either returns, advances the
//! bounded malformed-retry counter (COR-3), or moves wall-clock time toward the
//! whole-run budget (COR-1, `max_response_duration`, checked at the loop top).
//! The malformed-retry cap and the run budget are both hard bounds, so the loop
//! cannot spin forever even against an adversarial Provider.

use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::traits::{EgressStatus, Provider, ProviderError, ToolInvoker};
use crate::types::{
    CallAuthority, Limits, ModelTurn, RunEvent, RunOutcome, ToolCall, ToolOutcome, ToolSpec,
};
use crate::validate;

/// The operator-facing terminal message when a TAINT denial's one narration turn
/// is spent on more tool calls instead of an answer (pf6re). Truthful: a tainted
/// session only unlocks via a quarantine re-arm, which discards the conversation.
const TAINT_REARM_MSG: &str =
    "session tainted — the operator must re-arm to start a fresh authorized session \
     (this discards the conversation); nothing was sent";

/// Which kind of egress denial occurred, derived from the relayed reason string
/// (the security layer's `EgressDenied` Display; `"tainted"` marks the taint case).
/// Derived from the string rather than reshaping [`ToolOutcome::Denied`] — the
/// injection-test suite pattern-matches that variant and must stay untouched; the
/// coupling mirrors `classify_call_error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DenialKind {
    /// Not armed / expired — an operator ARM unlocks and PRESERVES the conversation
    /// (the agent can resume where it left off).
    Authority,
    /// Session tainted — only a quarantine re-arm unlocks it, and that DISCARDS the
    /// conversation (no resume). The stricter path.
    Taint,
}

fn denial_kind(reason: &str) -> DenialKind {
    if reason.to_ascii_lowercase().contains("tainted") {
        DenialKind::Taint
    } else {
        DenialKind::Authority
    }
}

/// Run the bounded agent loop to completion, using a pre-built conversation.
///
/// **Contract:** the caller MUST append the initiating `Message::User` via
/// [`Conversation::push_user`] BEFORE calling this function. This function does
/// NOT call `Conversation::new` — it drives the loop from whatever state the
/// conversation is already in, enabling multi-turn / resumed sessions.
///
/// * `conversation` is the running transcript (mutated in place).
/// * `provider` produces model turns; `invoker` executes tool calls.
/// * `status` is an observed (read-only) egress snapshot — purely informational.
/// * `limits` bound turns / per-turn time / malformed retries.
/// * `cancel` cooperatively aborts the loop (COR-2).
/// * `on_event` is a **fire-and-forget** progress callback (assistant text +
///   tool calls) so a caller can render a conversational transcript. It does
///   NOT gate the loop, change the [`RunOutcome`], or affect cancellation /
///   timeout / COR invariants — it is called for its side effect and its
///   return value is ignored. Pass `&|_| {}` to opt out (this is what [`run`]
///   does). Keep the callback trivial: it is called inline on the loop's task,
///   so a panic in it would unwind through the run.
pub async fn run_with_conversation(
    conversation: &mut Conversation,
    provider: &dyn Provider,
    invoker: &dyn ToolInvoker,
    status: EgressStatus,
    limits: Limits,
    cancel: CancellationToken,
    // `+ Sync` so a `&on_event` held across the loop's `.await`s is `Send` — the
    // Elmer session drives this loop inside a `tokio::spawn`ed (Send) task, and
    // `&F: Send` requires `F: Sync`. The no-op `&|_| {}` and the real sink
    // (which captures only an `Arc<dyn Fn + Send + Sync>`) both satisfy it.
    on_event: &(dyn Fn(RunEvent) + Sync),
) -> RunOutcome {
    // `status` is observed but never used to gate (gating lives below the MCP
    // boundary). Bind it so the read-only contract is explicit and the param is
    // not dead.
    let _ = status;

    let tools: Vec<ToolSpec> = invoker.tools().to_vec();

    let mut malformed_retries: u32 = 0;
    // pf6re: one-shot post-denial finalization. Set when an egress call is denied;
    // the model is then granted exactly ONE more turn to narrate the denial. If it
    // answers (Text) the run completes with that narration; if it calls tools again
    // it gets NO working window — the run terminates with denial context. This
    // preserves the "egress never retried" invariant while ending the turn with the
    // agent's own message instead of a raw error that clobbers output.
    let mut denial_final: Option<(String, DenialKind)> = None;
    // COR-1: bound the WHOLE run by wall-clock, not by an arbitrary tool-call
    // count. `tokio::time::Instant` shares the runtime clock with the per-turn
    // `timeout` below, so both are controllable together in time-paused tests.
    let start = tokio::time::Instant::now();

    loop {
        // COR-2: cooperative yield so a Stop/cancel task is polled between fast
        // tool turns even on a current-thread runtime. Without the former count
        // cap, a Provider + ToolInvoker that both return immediately could keep
        // this loop continuously ready until the whole-run budget elapsed and
        // starve the reactor — masking an external cancellation. yield_now costs
        // one reschedule per iteration and does not advance the clock.
        tokio::task::yield_now().await;

        // COR-2: never start a Provider call once cancellation is requested.
        if cancel.is_cancelled() {
            return RunOutcome::Cancelled;
        }

        // COR-1: the whole-run wall-clock budget. Checked before starting each
        // turn; a turn already in flight is bounded by the per-turn timeout, so
        // the total run is bounded by max_response_duration + one per-turn
        // timeout. Replaces the former fixed tool-turn count cap.
        if start.elapsed() >= limits.max_response_duration {
            return RunOutcome::NeedsOperator(format!(
                "Elmer's response exceeded the {}s budget",
                limits.max_response_duration.as_secs()
            ));
        }

        // COR-1: a per-turn wall-clock timeout. Exhaustion → NeedsOperator.
        // COR-2: race the per-turn call against the cancellation token so a Stop
        // that arrives WHILE a Provider turn is in flight interrupts it
        // immediately. Without this race the loop would block on `provider.turn()`
        // (a long model call — minutes for a large local model) until it finished
        // or the per-turn timeout elapsed, and the cancel would only be observed
        // at the next loop top — making Stop appear unresponsive. `select!` drops
        // the in-flight turn future on cancel (cancelling the model HTTP request);
        // `biased` prefers the cancel branch.
        let turn = tokio::select! {
            biased;
            () = cancel.cancelled() => return RunOutcome::Cancelled,
            timed = tokio::time::timeout(
                limits.per_turn_timeout,
                // Pass the loop's own fire-and-forget sink so a streaming
                // provider can emit AssistantDelta / ReasoningDelta as content
                // arrives. The provider's deltas plus the loop's own finalizing
                // AssistantText / ToolCall emits all flow through the same
                // callback; deltas are side-effect-only and never change the
                // returned ModelTurn or any COR invariant.
                provider.turn(conversation, &tools, on_event),
            ) => match timed {
                Err(_elapsed) => {
                    return RunOutcome::NeedsOperator(format!(
                        "model turn exceeded the {}s per-turn timeout",
                        limits.per_turn_timeout.as_secs()
                    ));
                }
                Ok(Err(err)) => return provider_error_outcome(err),
                Ok(Ok(turn)) => turn,
            },
        };

        match turn {
            ModelTurn::Text(text) => {
                conversation.push_assistant(&text);
                // Fire-and-forget: surface the assistant answer as a chat turn
                // BEFORE returning. Does not affect the outcome.
                on_event(RunEvent::AssistantText { text: text.clone() });
                return RunOutcome::Completed(text);
            }
            ModelTurn::ToolCalls(calls) => {
                // pf6re: one-shot finalization. If a prior tool call was denied we
                // granted exactly ONE narration turn. The model was supposed to
                // ANSWER (Text). It emitted tool calls instead — do NOT dispatch
                // them (a tainted/injected model gets no working window after a
                // denial). Terminate with denial context: taint routes to a
                // re-arm-quarantine NeedsOperator; authority relays the denial.
                if let Some((reason, kind)) = denial_final.take() {
                    return match kind {
                        DenialKind::Taint => RunOutcome::NeedsOperator(TAINT_REARM_MSG.to_string()),
                        DenialKind::Authority => RunOutcome::ToolDenied(reason),
                    };
                }

                // COR-3: validate the calls. A malformed batch is fed back and
                // re-prompted, bounded by `max_malformed_retries`.
                if let Some(detail) = first_validation_error(&tools, &calls) {
                    if malformed_retries >= limits.max_malformed_retries {
                        return RunOutcome::InvalidAction(detail);
                    }
                    malformed_retries += 1;
                    // Record the offending calls + the validation error so the
                    // model can correct itself on the re-prompt.
                    for call in &calls {
                        conversation.push_tool_call(call.clone());
                    }
                    conversation.push_tool_error("validation", detail);
                    continue;
                }

                // Valid batch. Reset the malformed counter — the model
                // recovered. The whole-run wall-clock budget (checked at the
                // loop top) bounds how long the tool loop may run; there is no
                // fixed cap on the number of tool turns.
                malformed_retries = 0;

                // Dispatch each call. Cancellation is checked before each and
                // propagated into the in-flight tool future (COR-2).
                for call in &calls {
                    if cancel.is_cancelled() {
                        return RunOutcome::Cancelled;
                    }
                    conversation.push_tool_call(call.clone());
                    // Fire-and-forget: surface the tool call as a chat chip.
                    // Does not affect the outcome or cancellation.
                    on_event(RunEvent::ToolCall { tool: call.name.clone() });

                    // SEC-3: the authority is ALWAYS Agent. There is no code
                    // path here that can construct any other CallAuthority.
                    let outcome = invoker
                        .invoke(call, CallAuthority::Agent, &cancel)
                        .await;

                    // A Cancelled outcome is terminal: surface immediately without
                    // pushing into the conversation (the session is being torn down).
                    if let ToolOutcome::Cancelled(_) = &outcome {
                        return RunOutcome::Cancelled;
                    }

                    // pf6re: a denial no longer KILLS the turn. Feed it back as a
                    // tool result (the model sees the cause-accurate reason and can
                    // narrate it), emit a DURABLE denial event so the security
                    // signal survives even if the run ends `Completed`, then BREAK
                    // the batch (do NOT execute the remaining calls — they were
                    // predicated on this one succeeding) and re-prompt for the ONE
                    // narration turn. Egress stays absolutely locked: the gate
                    // already refused, and any retry in the narration turn is not
                    // dispatched (the `denial_final` check above).
                    if let ToolOutcome::Denied(reason) = &outcome {
                        conversation.push_outcome(&call.name, &outcome);
                        on_event(RunEvent::ToolDenied {
                            tool: call.name.clone(),
                            reason: reason.clone(),
                        });
                        denial_final = Some((reason.clone(), denial_kind(reason)));
                        break;
                    }

                    conversation.push_outcome(&call.name, &outcome);
                }
            }
        }
    }
}

/// Run the bounded agent loop to completion.
///
/// * `user_msg` seeds the conversation.
/// * `provider` produces model turns; `invoker` executes tool calls.
/// * `status` is an observed (read-only) egress snapshot — purely informational.
/// * `limits` bound turns / per-turn time / malformed retries.
/// * `cancel` cooperatively aborts the loop (COR-2).
pub async fn run(
    user_msg: impl Into<String>,
    provider: &dyn Provider,
    invoker: &dyn ToolInvoker,
    status: EgressStatus,
    limits: Limits,
    cancel: CancellationToken,
) -> RunOutcome {
    let mut conversation = Conversation::new(user_msg);
    // `run` keeps its historical signature by opting out of progress events with
    // a no-op callback; only `run_with_conversation` exposes the `on_event` hook.
    run_with_conversation(
        &mut conversation,
        provider,
        invoker,
        status,
        limits,
        cancel,
        &|_| {},
    )
    .await
}

/// Map a [`ProviderError`] onto a terminal outcome. The loop cannot make
/// progress without the model, so any provider failure surfaces to the operator.
fn provider_error_outcome(err: ProviderError) -> RunOutcome {
    match err {
        ProviderError::RateLimited(msg) => RunOutcome::RateLimited(msg),
        // Transport / non-2xx / unparseable are genuine FAILURES the operator must be
        // able to capture verbatim — surface them as ProviderError (persisted as the
        // "error" outcome), NOT the soft NeedsOperator gate (tuxlink-a1xwx).
        other => RunOutcome::ProviderError(format!("provider error: {other}")),
    }
}

/// Return the first validation error across a batch of tool calls, or `None`
/// if every call is well-formed. An unknown tool name is itself a malformed
/// call (the model addressed a tool that does not exist).
fn first_validation_error(tools: &[ToolSpec], calls: &[ToolCall]) -> Option<String> {
    if calls.is_empty() {
        return Some("model emitted an empty tool-call batch".to_string());
    }
    for call in calls {
        match tools.iter().find(|t| t.name == call.name) {
            None => {
                let known: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
                return Some(format!(
                    "unknown tool `{}`; available tools: {}",
                    call.name,
                    known.join(", ")
                ));
            }
            Some(spec) => {
                if let Err(detail) = validate::validate(&spec.json_schema, &call.args) {
                    return Some(format!("arguments for `{}` are invalid: {detail}", call.name));
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// provider_error_outcome unit tests
// (kept at end-of-file: clippy::items_after_test_module denies production items
// after a #[cfg(test)] mod, so test modules live below all production items.)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod provider_error_outcome_tests {
    use super::*;

    /// `ProviderError::RateLimited` maps to `RunOutcome::RateLimited`, NOT
    /// `NeedsOperator`.  This ensures a 429 from the model surfaces the
    /// rate-limit callout rather than the generic operator-nudge.
    #[test]
    fn rate_limited_error_maps_to_rate_limited_outcome() {
        let err = ProviderError::RateLimited("HTTP 429: quota exceeded".to_string());
        let outcome = provider_error_outcome(err);
        match &outcome {
            RunOutcome::RateLimited(msg) => {
                assert!(
                    msg.contains("429"),
                    "detail must carry the 429 snippet; got: {msg:?}"
                );
            }
            other => panic!("expected RunOutcome::RateLimited, got {other:?}"),
        }
    }

    /// `ProviderError::Transport` maps to `RunOutcome::ProviderError` — a genuine
    /// failure the operator can capture (persisted), NOT the soft `NeedsOperator`
    /// gate (tuxlink-a1xwx). This is the regression that hid the Gemini tool-call
    /// 400 in a single-slot callout the next run overwrote.
    #[test]
    fn transport_error_maps_to_provider_error() {
        let err = ProviderError::Transport("connection refused".to_string());
        let outcome = provider_error_outcome(err);
        assert!(
            matches!(outcome, RunOutcome::ProviderError(_)),
            "Transport must map to ProviderError, got: {outcome:?}"
        );
    }

    /// `ProviderError::Unparseable` maps to `RunOutcome::ProviderError`.
    #[test]
    fn unparseable_error_maps_to_provider_error() {
        let err = ProviderError::Unparseable("bad JSON".to_string());
        let outcome = provider_error_outcome(err);
        assert!(
            matches!(outcome, RunOutcome::ProviderError(_)),
            "Unparseable must map to ProviderError, got: {outcome:?}"
        );
    }
}
