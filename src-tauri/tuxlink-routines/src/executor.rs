//! Track executor (spec §8): sequential steps, branch jumps, retry wrappers,
//! explicit End, per-step timeouts, cancellation, delays, and concurrent
//! tracks over shared vars. Composition lands in `compose.rs`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::action::ActionRegistry;
use crate::compose::RoutineInvoker;
use crate::error::StepError;
use crate::journal::{JournalWriter, RunEvent, RunState};
use crate::types::{ActionStep, Control, Step, StepId, Track};
use crate::vars::RunVars;

pub struct ExecCtx {
    pub registry: Arc<ActionRegistry>,
    pub journal: Arc<Mutex<JournalWriter>>,
    pub cancel: CancellationToken,
    pub default_timeout_s: u64,
    /// Clock used for `next:hour` / `next:day` delay alignment.
    pub now: fn() -> i64,
    /// Composition backstop for `Control::Call` (spec §7): the engine facade
    /// (Task 9) implements this against real child runs; tests use
    /// `fakes::FakeInvoker`.
    pub invoker: Arc<dyn RoutineInvoker>,
    /// This run's own id, threaded into `Provenance::parent_run_id` on every
    /// child call so journals alone reconstruct "run 47, step 3 -> child 48".
    pub run_id: String,
    /// Call-chain depth for the runtime backstop against runaway recursion
    /// (spec §7); root runs start at 0, each `Control::Call` a child engine
    /// spawns increments it by one.
    pub depth: u32,
}

/// `Align` and `duration_to_next_align` now live in `scheduler.rs` (Task 9;
/// shared with `Trigger::Schedule`'s `align` field). Re-exported here so
/// `Control::Delay`'s `next:hour` / `next:day` handling and Task 6's tests
/// compile unchanged.
pub use crate::scheduler::{duration_to_next_align, Align};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelaySpec {
    Relative(Duration),
    NextAlign(Align),
}

/// Parse "+5m" / "+90s" / "+2h" / "next:hour" / "next:day" (spec §6).
pub fn parse_delay(spec: &str) -> Result<DelaySpec, StepError> {
    let bad = || StepError::Action {
        action: "delay".into(),
        cause: format!("unparseable delay spec '{spec}' (want +Ns/+Nm/+Nh or next:hour/next:day)"),
    };
    if let Some(rest) = spec.strip_prefix("next:") {
        return match rest {
            "hour" => Ok(DelaySpec::NextAlign(Align::Hour)),
            "day" => Ok(DelaySpec::NextAlign(Align::Day)),
            _ => Err(bad()),
        };
    }
    let rest = spec.strip_prefix('+').ok_or_else(bad)?;
    let (num, unit) = rest.split_at(rest.len().saturating_sub(1));
    let n: u64 = num.parse().map_err(|_| bad())?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        _ => return Err(bad()),
    };
    Ok(DelaySpec::Relative(Duration::from_secs(secs)))
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackEnd {
    /// Ran off the end of the step list.
    Completed,
    /// Hit an explicit End step (terminates the RUN, spec §6).
    Ended { failed: bool, reason: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RunOutcome {
    pub state: RunState,
}

fn journal(ctx: &ExecCtx, event: RunEvent) {
    // Journal I/O failure is unrecoverable-by-design: a run we can't record
    // is a run we must not continue (no-silent-death). Panic in debug;
    // in release the lock/write error still aborts the run via unwrap.
    ctx.journal.lock().unwrap().append(event).expect("run journal must be writable");
}

/// Resolve `$var` string params through RunVars (spec §14 convention).
fn resolve_params(params: &serde_json::Value, vars: &RunVars) -> Result<serde_json::Value, StepError> {
    match params {
        serde_json::Value::String(s) if s.starts_with('$') => vars.resolve(&s[1..]),
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_params(v, vars)?);
            }
            Ok(serde_json::Value::Object(out))
        }
        serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
            items.iter().map(|v| resolve_params(v, vars)).collect::<Result<_, _>>()?,
        )),
        other => Ok(other.clone()),
    }
}

/// Run one action step against vars shared across concurrent tracks.
///
/// Lock-before-await discipline (same as `ModemSession`): `vars` is locked
/// only long enough to resolve `$var` params or to record the step's
/// output — the lock is dropped BEFORE the action's own `.await`, so a
/// slow/hanging action on one track never blocks a sibling track's var
/// access.
async fn run_action_step_shared(
    step: &ActionStep,
    vars: &tokio::sync::Mutex<RunVars>,
    ctx: &ExecCtx,
) -> Result<(), StepError> {
    let action = ctx.registry.get(&step.action).ok_or_else(|| StepError::Action {
        action: step.action.clone(),
        cause: format!("action '{}' is not in the registry", step.action),
    })?;
    let resolved = {
        let guard = vars.lock().await;
        resolve_params(&step.params, &guard)?
    }; // lock dropped here, before any action await
    journal(ctx, RunEvent::StepIntent {
        step: step.id.clone(),
        action: step.action.clone(),
        resolved_params: resolved.clone(),
    });
    let seconds = step.timeout_s.unwrap_or(ctx.default_timeout_s);
    let child_cancel = ctx.cancel.child_token();
    let result = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(seconds), action.execute(resolved, child_cancel.clone())) => {
            match r {
                Ok(inner) => inner,
                Err(_elapsed) => {
                    child_cancel.cancel();
                    Err(StepError::Timeout { seconds })
                }
            }
        }
        _ = ctx.cancel.cancelled() => Err(StepError::Cancelled),
    };
    match result {
        Ok(output) => {
            journal(ctx, RunEvent::StepOk { step: step.id.clone(), output: output.clone() });
            {
                let mut guard = vars.lock().await;
                guard.set_step_output(&step.id, output);
            } // lock dropped immediately after the write
            Ok(())
        }
        Err(err) => {
            journal(ctx, RunEvent::StepErr { step: step.id.clone(), error: err.clone() });
            Err(err)
        }
    }
}

fn index_of(track: &Track, id: &StepId) -> Option<usize> {
    track.steps.iter().position(|s| s.id() == id)
}

/// Run a single track to completion, with `vars` shared behind a mutex so
/// sibling tracks (spawned by `run_tracks`) can read/write the same store —
/// e.g. the "+5 min re-dial last heard gateway" scenario, where track B
/// reads `$track_a_step.gateway` after track A has set it.
pub async fn run_track_shared(
    track: &Track,
    vars: &tokio::sync::Mutex<RunVars>,
    ctx: &ExecCtx,
) -> Result<TrackEnd, StepError> {
    let mut idx = 0usize;
    // Steps that only exist as a Retry wrapper's target are skipped when
    // reached sequentially (the wrapper executed them).
    let mut consumed: std::collections::HashSet<StepId> = std::collections::HashSet::new();

    while idx < track.steps.len() {
        if ctx.cancel.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let step = &track.steps[idx];
        if consumed.contains(step.id()) {
            idx += 1;
            continue;
        }
        match step {
            Step::Action(a) => {
                run_action_step_shared(a, vars, ctx).await?;
                idx += 1;
            }
            Step::Control(c) => match &c.control {
                Control::Branch { on, then, r#else } => {
                    let v = {
                        let guard = vars.lock().await;
                        guard.resolve(on)?
                    };
                    let truthy = v == serde_json::Value::Bool(true);
                    let arm = if truthy { then } else { r#else };
                    match arm.first() {
                        Some(target) => {
                            idx = index_of(track, target).ok_or_else(|| StepError::Action {
                                action: "branch".into(),
                                cause: format!("branch target '{}' not found in track '{}'", target.0, track.name),
                            })?;
                        }
                        None => idx += 1, // empty arm: fall through
                    }
                }
                Control::Retry { step: target, attempts, backoff_s } => {
                    // Validate: attempts must be > 0 or the loop never executes.
                    if *attempts == 0 {
                        return Err(StepError::Action {
                            action: "retry".into(),
                            cause: format!("retry step '{}' has attempts: 0 — it can never execute its target", c.id.0),
                        });
                    }
                    let target_idx = index_of(track, target).ok_or_else(|| StepError::Action {
                        action: "retry".into(),
                        cause: format!("retry target '{}' not found in track '{}'", target.0, track.name),
                    })?;
                    let Step::Action(inner) = &track.steps[target_idx] else {
                        return Err(StepError::Action {
                            action: "retry".into(),
                            cause: format!("retry target '{}' is not an action step", target.0),
                        });
                    };
                    let mut last_err = None;
                    for attempt in 0..*attempts {
                        match run_action_step_shared(inner, vars, ctx).await {
                            Ok(()) => {
                                last_err = None;
                                break;
                            }
                            Err(e @ StepError::Cancelled) => return Err(e),
                            Err(e) => {
                                last_err = Some(e);
                                if attempt + 1 < *attempts && *backoff_s > 0 {
                                    tokio::select! {
                                        _ = tokio::time::sleep(Duration::from_secs(*backoff_s)) => {},
                                        _ = ctx.cancel.cancelled() => return Err(StepError::Cancelled),
                                    }
                                }
                            }
                        }
                    }
                    if let Some(e) = last_err {
                        return Err(e);
                    }
                    consumed.insert(target.clone());
                    idx += 1;
                }
                Control::End { failed, reason } => {
                    return Ok(TrackEnd::Ended { failed: *failed, reason: reason.clone() });
                }
                Control::Delay { delay } => {
                    let spec = parse_delay(delay)?;
                    let dur = match spec {
                        DelaySpec::Relative(d) => d,
                        DelaySpec::NextAlign(align) => duration_to_next_align((ctx.now)(), align),
                    };
                    journal(ctx, RunEvent::StateChanged { state: RunState::Waiting });
                    tokio::select! {
                        _ = tokio::time::sleep(dur) => {}
                        _ = ctx.cancel.cancelled() => return Err(StepError::Cancelled),
                    }
                    journal(ctx, RunEvent::StateChanged { state: RunState::Running });
                    idx += 1;
                }
                Control::Call { routine, args, sync } => {
                    // Resolve args first: a $var resolution failure is a
                    // step failure per the unset-variable rules (spec §10),
                    // same as an action step's params — no journal entry for
                    // a call that never had valid params to attempt.
                    let resolved_args = {
                        let guard = vars.lock().await;
                        resolve_params(args, &guard)?
                    }; // lock dropped here, before any invoke await
                    journal(ctx, RunEvent::StepIntent {
                        step: c.id.clone(),
                        action: format!("call:{routine}"),
                        resolved_params: resolved_args.clone(),
                    });
                    if ctx.depth >= crate::compose::MAX_CALL_DEPTH {
                        let err = StepError::Action {
                            action: format!("call:{routine}"),
                            cause: format!(
                                "call depth {} exceeds cap {} — recursive routine chain",
                                ctx.depth,
                                crate::compose::MAX_CALL_DEPTH
                            ),
                        };
                        journal(ctx, RunEvent::StepErr { step: c.id.clone(), error: err.clone() });
                        return Err(err);
                    }
                    let provenance = crate::compose::Provenance {
                        parent_run_id: ctx.run_id.clone(),
                        parent_step: c.id.clone(),
                    };
                    if *sync {
                        match ctx.invoker.invoke(routine, resolved_args, provenance).await {
                            Ok(result) => {
                                journal(ctx, RunEvent::StepOk { step: c.id.clone(), output: result.clone() });
                                let mut guard = vars.lock().await;
                                guard.set_step_output(&c.id, result);
                            }
                            Err(err) => {
                                journal(ctx, RunEvent::StepErr { step: c.id.clone(), error: err.clone() });
                                return Err(err);
                            }
                        }
                    } else {
                        let invoker = ctx.invoker.clone();
                        let routine = routine.clone();
                        tokio::spawn(async move {
                            // Child journals its own outcome; the parent does
                            // not await it (fire-and-forget, spec §7).
                            let _ = invoker.invoke(&routine, resolved_args, provenance).await;
                        });
                        let marker = serde_json::json!({"dispatched": true});
                        journal(ctx, RunEvent::StepOk { step: c.id.clone(), output: marker.clone() });
                        let mut guard = vars.lock().await;
                        guard.set_step_output(&c.id, marker);
                    }
                    idx += 1;
                }
            },
        }
    }
    Ok(TrackEnd::Completed)
}

/// Single-track entry point (Task 5's original signature). A thin wrapper
/// over `run_track_shared`: wraps `vars` in a task-local mutex so every
/// pre-existing test keeps working against an owned `&mut RunVars` unchanged.
pub async fn run_track(
    track: &Track,
    vars: &mut RunVars,
    ctx: &ExecCtx,
) -> Result<TrackEnd, StepError> {
    let shared = tokio::sync::Mutex::new(std::mem::take(vars));
    let result = run_track_shared(track, &shared, ctx).await;
    *vars = shared.into_inner();
    result
}

/// Run all tracks concurrently over shared vars; map to a run outcome per
/// the locked rules (spec §8 / task header):
///   - any track's `Ended{failed:true}` -> run `Failed` (first reason wins),
///     remaining tracks cancelled.
///   - any track's unhandled `StepErr` -> run `Failed` verbatim (propagated
///     as `Err`), remaining tracks cancelled.
///   - any track's `Ended{failed:false}` -> run `Completed` immediately
///     (an explicit successful End means the routine has done its job), remaining
///     tracks cancelled.
///   - all tracks `Completed` (ran off the end) -> run `Completed`.
///
/// Cancelling siblings after one track concludes produces collateral
/// `Err(StepError::Cancelled)` results from the rest. Those must never be
/// mistaken for the run's own failure, no matter what order `join_next`
/// hands results back in — so once THIS function has triggered a cancel for
/// any reason, every subsequent `Cancelled` result is treated as collateral,
/// not re-examined against the conclusive outcome already recorded.
pub async fn run_tracks(
    tracks: &[Track],
    vars: Arc<tokio::sync::Mutex<RunVars>>,
    ctx: &ExecCtx,
) -> Result<RunOutcome, StepError> {
    let mut set = tokio::task::JoinSet::new();
    // clippy suggests dropping `.cloned()` and spawning against a borrowed
    // `&Track`, but `tracks: &[Track]` is not `'static` — `JoinSet::spawn`
    // requires an owned, `'static` future, so the clone is load-bearing
    // (confirmed: the borrowed form fails with E0521 "borrowed data escapes
    // outside of function").
    #[allow(clippy::unnecessary_to_owned)]
    for track in tracks.iter().cloned() {
        let vars = vars.clone();
        let task_ctx = ExecCtx {
            registry: ctx.registry.clone(),
            journal: ctx.journal.clone(),
            cancel: ctx.cancel.child_token(),
            default_timeout_s: ctx.default_timeout_s,
            now: ctx.now,
            invoker: ctx.invoker.clone(),
            run_id: ctx.run_id.clone(),
            depth: ctx.depth,
        };
        set.spawn(async move { run_track_shared(&track, &vars, &task_ctx).await });
    }

    let mut outcome = RunOutcome { state: RunState::Completed };
    let mut first_err: Option<StepError> = None;
    let mut intentional_cancel = false;

    while let Some(joined) = set.join_next().await {
        match joined.expect("track task must not panic") {
            Ok(TrackEnd::Completed) => {}
            Ok(TrackEnd::Ended { failed: false, .. }) => {
                ctx.cancel.cancel();
                intentional_cancel = true;
                if outcome.state != RunState::Failed {
                    outcome.state = RunState::Completed;
                }
            }
            Ok(TrackEnd::Ended { failed: true, .. }) => {
                ctx.cancel.cancel();
                intentional_cancel = true;
                outcome.state = RunState::Failed;
            }
            // Collateral of a cancel THIS function already triggered — never
            // conclusive, regardless of arrival order relative to the
            // triggering result.
            Err(StepError::Cancelled) if intentional_cancel => {}
            Err(e) => {
                ctx.cancel.cancel();
                intentional_cancel = true;
                first_err.get_or_insert(e);
            }
        }
    }

    if let Some(e) = first_err {
        return Err(e);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionRegistry;
    use crate::fakes::{FakeAction, FakeInvoker};
    use crate::journal::{read_journal, JournalWriter, RunEvent};
    use crate::types::{ActionStep, BusyPolicy, Control, ControlStep, Step, StepId, Track};
    use crate::vars::RunVars;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    fn fixed_now() -> i64 { 1_752_400_000 }

    fn action(id: &str, name: &'static str, params: serde_json::Value) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: name.into(),
            params,
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn ctx(reg: ActionRegistry, dir: &std::path::Path) -> (ExecCtx, std::path::PathBuf) {
        let journal = JournalWriter::create(dir, "run-t", fixed_now).unwrap();
        let path = journal.path();
        let ctx = ExecCtx {
            registry: Arc::new(reg),
            journal: Arc::new(Mutex::new(journal)),
            cancel: CancellationToken::new(),
            default_timeout_s: 30,
            now: fixed_now,
            invoker: Arc::new(FakeInvoker::new()),
            run_id: "run-t".into(),
            depth: 0,
        };
        (ctx, path)
    }

    #[tokio::test]
    async fn happy_path_runs_steps_in_order_and_journals_intent_before_ok() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.compose").ok(json!({"staged": 1}))));
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, jpath) = ctx(reg, dir.path());

        let track = Track {
            name: "t1".into(),
            steps: vec![
                action("s1", "local.compose", json!({})),
                action("s2", "local.log", json!({})),
            ],
        };
        let mut vars = RunVars::default();
        let end = run_track(&track, &mut vars, &ctx).await.unwrap();
        assert!(matches!(end, TrackEnd::Completed));

        let entries = read_journal(&jpath).unwrap();
        let kinds: Vec<&str> = entries.iter().map(|e| match &e.event {
            RunEvent::StepIntent { .. } => "intent",
            RunEvent::StepOk { .. } => "ok",
            other => panic!("unexpected {other:?}"),
        }).collect();
        assert_eq!(kinds, vec!["intent", "ok", "intent", "ok"]);
    }

    #[tokio::test]
    async fn branch_follows_the_variable_and_jumps() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("radio.connect").ok(json!({"connected": false}))));
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        reg.register(Arc::new(FakeAction::new("local.notify").ok(json!({}))));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, jpath) = ctx(reg, dir.path());

        // s1 connect → branch on s1.connected: then [s3 log] else [s4 notify]
        let track = Track {
            name: "t1".into(),
            steps: vec![
                action("s1", "radio.connect", json!({})),
                Step::Control(ControlStep {
                    id: StepId("s2".into()),
                    control: Control::Branch {
                        on: "s1.connected".into(),
                        then: vec![StepId("s3".into())],
                        r#else: vec![StepId("s4".into())],
                    },
                }),
                action("s3", "local.log", json!({})),
                Step::Control(ControlStep {
                    id: StepId("end1".into()),
                    control: Control::End { failed: false, reason: None },
                }),
                action("s4", "local.notify", json!({})),
            ],
        };
        let mut vars = RunVars::default();
        let end = run_track(&track, &mut vars, &ctx).await.unwrap();
        // connected=false → else-arm → s4 executes, s3 does not
        assert!(matches!(end, TrackEnd::Completed));
        let entries = read_journal(&jpath).unwrap();
        let executed: Vec<String> = entries.iter().filter_map(|e| match &e.event {
            RunEvent::StepIntent { step, .. } => Some(step.0.clone()),
            _ => None,
        }).collect();
        assert_eq!(executed, vec!["s1", "s4"]);
    }

    #[tokio::test]
    async fn end_step_terminates_with_failed_and_reason() {
        let reg = ActionRegistry::default();
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![Step::Control(ControlStep {
                id: StepId("e".into()),
                control: Control::End { failed: true, reason: Some("no contact after all bands".into()) },
            })],
        };
        let mut vars = RunVars::default();
        match run_track(&track, &mut vars, &ctx).await.unwrap() {
            TrackEnd::Ended { failed, reason } => {
                assert!(failed);
                assert_eq!(reason.as_deref(), Some("no contact after all bands"));
            }
            other => panic!("expected Ended, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unhandled_step_error_fails_the_run_verbatim() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("radio.connect").err("CAT: rig did not respond on /dev/ttyUSB0")));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, jpath) = ctx(reg, dir.path());
        let track = Track { name: "t1".into(), steps: vec![action("s1", "radio.connect", json!({}))] };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        match &err {
            StepError::Action { cause, .. } => assert_eq!(cause, "CAT: rig did not respond on /dev/ttyUSB0"),
            other => panic!("expected verbatim Action error, got {other:?}"),
        }
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(entries.last().unwrap().event, RunEvent::StepErr { .. }));
    }

    #[tokio::test]
    async fn retry_reruns_the_wrapped_step_until_success() {
        // Retry WRAPS: the executor sees the Retry control step INSTEAD of a
        // bare action in the sequential flow. The wrapped action ("s1") sits
        // in the track only as the wrapper's jump target — run_track's
        // executed-set logic marks it consumed once the wrapper runs it, so
        // sequential flow skips over it afterward.
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(
            FakeAction::new("radio.connect")
                .err("VARA: BUSY")
                .err("VARA: BUSY")
                .ok(json!({"connected": true})),
        ));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![
                Step::Control(ControlStep {
                    id: StepId("r1".into()),
                    control: Control::Retry { step: StepId("s1".into()), attempts: 3, backoff_s: 0 },
                }),
                action("s1", "radio.connect", json!({})),
            ],
        };
        let mut vars = RunVars::default();
        let end = run_track(&track, &mut vars, &ctx).await.unwrap();
        assert!(matches!(end, TrackEnd::Completed));
        assert_eq!(vars.resolve("s1.connected").unwrap(), json!(true));
    }

    #[tokio::test(start_paused = true)]
    async fn step_timeout_fails_with_timeout_error() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("radio.connect").hang()));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let step = ActionStep {
            id: StepId("s1".into()),
            action: "radio.connect".into(),
            params: json!({}),
            timeout_s: Some(1),
            on_radio_busy: BusyPolicy::Wait,
        };
        let track = Track { name: "t1".into(), steps: vec![Step::Action(step)] };
        let mut vars = RunVars::default();
        // With paused virtual time, tokio::time::timeout's deadline elapses
        // as soon as the executor awaits it (no real-world sleep needed).
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(matches!(err, StepError::Timeout { seconds: 1 }));
    }

    #[tokio::test]
    async fn dollar_params_resolve_from_vars_before_execute() {
        let connect = Arc::new(FakeAction::new("radio.connect").ok(json!({"gateway": "W7DEF-10"})));
        let redial = Arc::new(FakeAction::new("radio.redial").ok(json!({})));
        let mut reg = ActionRegistry::default();
        reg.register(connect.clone());
        reg.register(redial.clone());
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![
                action("s1", "radio.connect", json!({})),
                action("s2", "radio.redial", json!({"station": "$s1.gateway"})),
            ],
        };
        let mut vars = RunVars::default();
        run_track(&track, &mut vars, &ctx).await.unwrap();
        assert_eq!(redial.calls()[0]["station"], "W7DEF-10");
    }

    #[tokio::test]
    async fn dollar_param_on_unset_variable_fails_the_step() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("radio.redial").ok(json!({}))));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![action("s2", "radio.redial", json!({"station": "$s1.gateway"}))],
        };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(matches!(err, StepError::UnsetVariable(p) if p == "s1.gateway"));
    }

    #[tokio::test]
    async fn retry_with_zero_attempts_fails_loudly() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("radio.connect").ok(json!({"connected": true}))));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![
                Step::Control(ControlStep {
                    id: StepId("r1".into()),
                    control: Control::Retry { step: StepId("s1".into()), attempts: 0, backoff_s: 0 },
                }),
                action("s1", "radio.connect", json!({})),
            ],
        };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        match err {
            StepError::Action { cause, .. } => {
                assert!(cause.contains("attempts: 0"));
            }
            other => panic!("expected StepError::Action with 'attempts: 0', got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retry_backoff_is_cancellable() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(
            FakeAction::new("radio.connect")
                .err("error1")
                .err("error2")
                .err("error3"),
        ));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![
                Step::Control(ControlStep {
                    id: StepId("r1".into()),
                    control: Control::Retry { step: StepId("s1".into()), attempts: 3, backoff_s: 3600 },
                }),
                action("s1", "radio.connect", json!({})),
            ],
        };

        let mut vars = RunVars::default();

        // Set up a cancellation that fires after minimal delay.
        // We use a channel-based approach: a background task will cancel after a short delay.
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let cancel_token = ctx.cancel.clone();

        // Spawn a lightweight task that will cancel after a delay.
        let cancel_handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_token.cancel();
            let _ = tx.send(()).await;
        });

        // Run the track; it should be cancelled during the backoff.
        let result = run_track(&track, &mut vars, &ctx).await;
        let _ = rx.recv().await; // Wait for cancellation signal to ensure timing.
        let _ = cancel_handle.await;

        match result {
            Err(StepError::Cancelled) => {
                // Success: the backoff was cancelled.
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn delay_step_sleeps_virtual_time_and_journals_waiting() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, jpath) = ctx(reg, dir.path());
        let track = Track {
            name: "t1".into(),
            steps: vec![
                Step::Control(ControlStep {
                    id: StepId("d1".into()),
                    control: Control::Delay { delay: "+5m".into() },
                }),
                action("s1", "local.log", json!({})),
            ],
        };
        let mut vars = RunVars::default();
        let start = tokio::time::Instant::now();
        run_track(&track, &mut vars, &ctx).await.unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_secs(300));
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(entries[0].event, RunEvent::StateChanged { state: RunState::Waiting }));
        assert!(matches!(entries[1].event, RunEvent::StateChanged { state: RunState::Running }));
    }

    #[test]
    fn parse_delay_shapes() {
        assert_eq!(parse_delay("+5m").unwrap(), DelaySpec::Relative(std::time::Duration::from_secs(300)));
        assert_eq!(parse_delay("+90s").unwrap(), DelaySpec::Relative(std::time::Duration::from_secs(90)));
        assert_eq!(parse_delay("+2h").unwrap(), DelaySpec::Relative(std::time::Duration::from_secs(7200)));
        assert_eq!(parse_delay("next:hour").unwrap(), DelaySpec::NextAlign(Align::Hour));
        assert!(parse_delay("5 minutes").is_err());
        assert!(parse_delay("+5x").is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn parallel_tracks_share_vars_and_join() {
        // Track A sets a gateway; track B delays then reads it — the
        // "+5 min re-dial last heard gateway" scenario shape (spec §1).
        let connect = Arc::new(FakeAction::new("radio.connect").ok(json!({"gateway": "W7DEF-10"})));
        let redial = Arc::new(FakeAction::new("radio.redial").ok(json!({})));
        let mut reg = ActionRegistry::default();
        reg.register(connect);
        reg.register(redial.clone());
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let tracks = vec![
            Track { name: "a".into(), steps: vec![action("s1", "radio.connect", json!({}))] },
            Track {
                name: "b".into(),
                steps: vec![
                    Step::Control(ControlStep {
                        id: StepId("d1".into()),
                        control: Control::Delay { delay: "+5m".into() },
                    }),
                    action("s2", "radio.redial", json!({"station": "$s1.gateway"})),
                ],
            },
        ];
        let vars = Arc::new(tokio::sync::Mutex::new(RunVars::default()));
        let outcome = run_tracks(&tracks, vars, &ctx).await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(redial.calls()[0]["station"], "W7DEF-10");
    }

    #[tokio::test(start_paused = true)]
    async fn one_track_failing_cancels_the_others() {
        let hang = Arc::new(FakeAction::new("local.wait").hang());
        let boom = Arc::new(FakeAction::new("radio.connect").err("ARDOP: session wedge, ARQ timeout 120s"));
        let mut reg = ActionRegistry::default();
        reg.register(hang);
        reg.register(boom);
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx(reg, dir.path());
        let tracks = vec![
            Track { name: "a".into(), steps: vec![action("s1", "local.wait", json!({}))] },
            Track { name: "b".into(), steps: vec![action("s2", "radio.connect", json!({}))] },
        ];
        let vars = Arc::new(tokio::sync::Mutex::new(RunVars::default()));
        let err = run_tracks(&tracks, vars, &ctx).await.unwrap_err();
        assert!(matches!(err, StepError::Action { cause, .. } if cause.contains("ARQ timeout 120s")));
    }
}
