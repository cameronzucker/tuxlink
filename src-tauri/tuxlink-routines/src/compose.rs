//! Routine composition (spec §7): call is the primitive; fire-and-forget is
//! call-without-await. Provenance rides every invocation.
//!
//! The invoker is TWO-PHASE (O3): `start` returns as soon as the child run id
//! is known (so the parent can journal the `call_child` edge and, for
//! fire-and-forget, move on without ever awaiting), and `await_outcome`
//! consumes the returned [`ChildHandle`] to block on the child's terminal
//! outcome. Splitting the phases is what lets the executor journal a child run
//! id in EVERY call path (sync AND fire-and-forget) and lets a registry-backed
//! impl register the child for cancellation before returning — a wedged
//! `Running` registry entry would otherwise block the scheduler forever.

use async_trait::async_trait;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::error::StepError;
use crate::executor::RunOutcome;
use crate::types::StepId;

pub const MAX_CALL_DEPTH: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub parent_run_id: String,
    pub parent_step: StepId,
}

/// The started child run, handed back by [`RoutineInvoker::start`]. The
/// `run_id` is known immediately (journaled as the `call_child` edge); the
/// `cancel` token is the CHILD's own cancellation handle (a clone the executor
/// keeps before awaiting, so a mid-await parent cancel can cancel the child
/// even though `await_outcome` consumes the handle). The terminal outcome
/// rides a private oneshot the caller reaches ONLY through
/// [`RoutineInvoker::await_outcome`].
pub struct ChildHandle {
    pub run_id: String,
    pub cancel: CancellationToken,
    /// Private: cross-crate impls (the monolith's `SessionChildInvoker`)
    /// construct via [`ChildHandle::from_parts`] and extract the receiver in
    /// their `await_outcome` via [`ChildHandle::into_parts`]. CALLERS (the
    /// executor) await only through the trait fn.
    done: oneshot::Receiver<RunOutcome>,
}

impl ChildHandle {
    /// Public constructor for cross-crate impls.
    pub fn from_parts(
        run_id: String,
        cancel: CancellationToken,
        done: oneshot::Receiver<RunOutcome>,
    ) -> Self {
        ChildHandle {
            run_id,
            cancel,
            done,
        }
    }

    /// Consuming extractor for cross-crate impls' `await_outcome`.
    pub fn into_parts(self) -> (String, CancellationToken, oneshot::Receiver<RunOutcome>) {
        (self.run_id, self.cancel, self.done)
    }
}

/// Call-site context the executor already holds (`ExecCtx`), carried so the
/// impl can gate + register without global state.
pub struct CallCtx {
    pub provenance: Provenance,
    pub child_depth: u32,
    pub parent_attended: bool,
    /// The root run's consent context (C3): carries the root routine's name so
    /// a child-start can recompute the root's digests against the live store.
    /// `None` for attended roots and everywhere in B2 (C3 fills it).
    pub root: Option<RootConsent>,
}

/// The root run's acknowledgment context (C3): the root routine name plus the
/// digests recorded when it was acknowledged, re-verified at each child start.
#[derive(Debug, Clone, PartialEq)]
pub struct RootConsent {
    pub routine: String,
    pub transmit_digest: Option<String>,
    pub write_digest: Option<String>,
}

#[async_trait]
pub trait RoutineInvoker: Send + Sync {
    /// Start a child run; the run id is known on return (journaled as the
    /// `call_child` edge). Impls derive the child token from
    /// `parent_cancel.child_token()`. Registry-registering impls register the
    /// child (id + the CHILD's own token — cancellability must not depend on
    /// the [`ChildHandle`], which fire-and-forget drops) BEFORE returning.
    async fn start(
        &self,
        routine: &str,
        args: serde_json::Value,
        call: CallCtx,
        parent_cancel: &CancellationToken,
    ) -> Result<ChildHandle, StepError>;

    /// Await the child's terminal outcome; consumes the handle.
    async fn await_outcome(&self, handle: ChildHandle)
        -> Result<serde_json::Value, StepError>;
}

#[cfg(test)]
mod tests {
    use crate::action::ActionRegistry;
    use crate::executor::{run_track, ExecCtx};
    use crate::fakes::FakeInvoker;
    use crate::journal::{read_journal, JournalWriter, RunEvent};
    use crate::types::{Control, ControlStep, Step, StepId, Track};
    use crate::vars::RunVars;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    fn fixed_now() -> i64 {
        1_752_400_000
    }

    fn call_step(id: &str, routine: &str, args: serde_json::Value, sync: bool) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Call {
                routine: routine.into(),
                args,
                sync,
            },
        })
    }

    fn ctx_with(invoker: Arc<FakeInvoker>, dir: &std::path::Path) -> (ExecCtx, std::path::PathBuf) {
        let journal = JournalWriter::create(dir, "run-parent", fixed_now).unwrap();
        let path = journal.path();
        (
            ExecCtx {
                registry: Arc::new(ActionRegistry::default()),
                journal: Arc::new(Mutex::new(journal)),
                cancel: CancellationToken::new(),
                default_timeout_s: 30,
                now: fixed_now,
                invoker,
                run_id: "run-parent".into(),
                depth: 0,
                attended: false,
                consent: None,
            },
            path,
        )
    }

    #[tokio::test]
    async fn sync_call_awaits_child_and_stores_result_as_step_output() {
        let invoker = Arc::new(FakeInvoker::new().result(
            "clear-channel-connect",
            json!({"connected": true, "band": "40m"}),
        ));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx_with(invoker.clone(), dir.path());
        let track = Track {
            name: "t".into(),
            steps: vec![call_step(
                "s5",
                "clear-channel-connect",
                json!({"bands": ["40m"]}),
                true,
            )],
        };
        let mut vars = RunVars::default();
        run_track(&track, &mut vars, &ctx).await.unwrap();
        assert_eq!(vars.resolve("s5.band").unwrap(), json!("40m"));
        // Provenance was passed through `start`:
        let invs = invoker.invocations();
        assert_eq!(invs[0].routine, "clear-channel-connect");
        assert_eq!(invs[0].provenance.parent_run_id, "run-parent");
        assert_eq!(invs[0].provenance.parent_step.0, "s5");
    }

    #[tokio::test]
    async fn fire_and_forget_returns_immediately_with_dispatched_marker() {
        // Inline-start contract (O3): even fire-and-forget journals the
        // `call_child` edge (the child run id is known from `start`), then the
        // `dispatched: true` marker — and never awaits the child (`hang` here
        // would block a sync call, but F&F drops the handle unawaited).
        let invoker = Arc::new(FakeInvoker::new().hang("slow-routine"));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, jpath) = ctx_with(invoker, dir.path());
        let track = Track {
            name: "t".into(),
            steps: vec![call_step("s1", "slow-routine", json!({}), false)],
        };
        let mut vars = RunVars::default();
        run_track(&track, &mut vars, &ctx).await.unwrap(); // must NOT block
        assert_eq!(vars.resolve("s1.dispatched").unwrap(), json!(true));
        let entries = read_journal(&jpath).unwrap();
        // The intent is still journaled...
        assert!(entries.iter().any(|e| matches!(&e.event,
            RunEvent::StepIntent { action, .. } if action == "call:slow-routine")));
        // ...and so is the call_child edge, with the started child run id.
        assert!(entries.iter().any(|e| matches!(&e.event,
            RunEvent::CallChild { step, child_run_id }
                if step.0 == "s1" && !child_run_id.is_empty())));
        // ...and the dispatched marker as the step's ok output.
        assert!(entries.iter().any(|e| matches!(&e.event,
            RunEvent::StepOk { step, output }
                if step.0 == "s1" && output.get("dispatched") == Some(&json!(true)))));
    }

    #[tokio::test]
    async fn sync_call_failure_propagates_verbatim() {
        let invoker = Arc::new(
            FakeInvoker::new().error("broken", "child run failed at step s2: CAT: PTT stuck"),
        );
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx_with(invoker, dir.path());
        let track = Track {
            name: "t".into(),
            steps: vec![call_step("s1", "broken", json!({}), true)],
        };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(
            matches!(err, crate::error::StepError::Action { cause, .. } if cause.contains("PTT stuck"))
        );
    }

    #[tokio::test]
    async fn depth_cap_stops_runaway_recursion() {
        let invoker = Arc::new(FakeInvoker::new().result("x", json!({})));
        let dir = tempfile::tempdir().unwrap();
        let (mut ctx, _) = ctx_with(invoker, dir.path());
        ctx.depth = 8; // already at the cap
        let track = Track {
            name: "t".into(),
            steps: vec![call_step("s1", "x", json!({}), true)],
        };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(
            matches!(err, crate::error::StepError::Action { cause, .. } if cause.contains("depth"))
        );
    }
}
