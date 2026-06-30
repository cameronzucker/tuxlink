//! The bounded agent loop (T3 COR-1, T4 COR-2, T5 COR-3).
//!
//! [`run`] drives a [`Provider`] and a [`ToolInvoker`] to a terminal
//! [`RunOutcome`]. It holds ONLY a `&dyn Provider`, a `&dyn ToolInvoker`, and a
//! read-only [`EgressStatus`] — nothing that can arm send authority or clear
//! taint (SEC-4). Every tool call carries [`CallAuthority::Agent`] (SEC-3).
//!
//! Termination is guaranteed: every iteration either returns, advances the
//! bounded tool-turn counter (COR-1), or advances the bounded malformed-retry
//! counter (COR-3). Both counters are hard caps, so the loop cannot spin
//! forever even against an adversarial Provider.

use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::traits::{EgressStatus, Provider, ProviderError, ToolInvoker};
use crate::types::{
    CallAuthority, Limits, ModelTurn, RunEvent, RunOutcome, ToolCall, ToolOutcome, ToolSpec,
};
use crate::validate;

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
    on_event: &dyn Fn(RunEvent),
) -> RunOutcome {
    // `status` is observed but never used to gate (gating lives below the MCP
    // boundary). Bind it so the read-only contract is explicit and the param is
    // not dead.
    let _ = status;

    let tools: Vec<ToolSpec> = invoker.tools().to_vec();

    let mut tool_turns: u32 = 0;
    let mut malformed_retries: u32 = 0;

    loop {
        // COR-2: never start a Provider call once cancellation is requested.
        if cancel.is_cancelled() {
            return RunOutcome::Cancelled;
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
                provider.turn(conversation, &tools),
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

                // Valid batch. COR-1: this counts as one tool-executing turn.
                // Reset the malformed counter — the model recovered.
                malformed_retries = 0;
                tool_turns += 1;
                if tool_turns > limits.max_tool_turns {
                    return RunOutcome::NeedsOperator(format!(
                        "taken {} tool turns without finishing — continue?",
                        limits.max_tool_turns
                    ));
                }

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

                    // A denial is terminal: the security layer refused egress.
                    if let ToolOutcome::Denied(reason) = &outcome {
                        conversation.push_outcome(&call.name, &outcome);
                        return RunOutcome::ToolDenied(reason.clone());
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
    RunOutcome::NeedsOperator(format!("provider error: {err}"))
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
