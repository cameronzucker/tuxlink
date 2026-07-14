//! Routine composition (spec §7): call is the primitive; fire-and-forget is
//! call-without-await. Provenance rides every invocation.

use async_trait::async_trait;

use crate::error::StepError;
use crate::types::StepId;

pub const MAX_CALL_DEPTH: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub parent_run_id: String,
    pub parent_step: StepId,
}

#[async_trait]
pub trait RoutineInvoker: Send + Sync {
    /// Invoke a routine (or composite library step) by name; resolves when
    /// the child run reaches a terminal state. Failure cause is the child's
    /// verbatim failure.
    async fn invoke(
        &self,
        routine: &str,
        args: serde_json::Value,
        provenance: Provenance,
    ) -> Result<serde_json::Value, StepError>;
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

    fn fixed_now() -> i64 { 1_752_400_000 }

    fn call_step(id: &str, routine: &str, args: serde_json::Value, sync: bool) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Call { routine: routine.into(), args, sync },
        })
    }

    fn ctx_with(invoker: Arc<FakeInvoker>, dir: &std::path::Path) -> (ExecCtx, std::path::PathBuf) {
        let journal = JournalWriter::create(dir, "run-parent", fixed_now).unwrap();
        let path = journal.path();
        (ExecCtx {
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
        }, path)
    }

    #[tokio::test]
    async fn sync_call_awaits_child_and_stores_result_as_step_output() {
        let invoker = Arc::new(FakeInvoker::new().result("clear-channel-connect", json!({"connected": true, "band": "40m"})));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx_with(invoker.clone(), dir.path());
        let track = Track {
            name: "t".into(),
            steps: vec![call_step("s5", "clear-channel-connect", json!({"bands": ["40m"]}), true)],
        };
        let mut vars = RunVars::default();
        run_track(&track, &mut vars, &ctx).await.unwrap();
        assert_eq!(vars.resolve("s5.band").unwrap(), json!("40m"));
        // Provenance was passed:
        let invs = invoker.invocations();
        assert_eq!(invs[0].routine, "clear-channel-connect");
        assert_eq!(invs[0].provenance.parent_run_id, "run-parent");
        assert_eq!(invs[0].provenance.parent_step.0, "s5");
    }

    #[tokio::test]
    async fn fire_and_forget_returns_immediately_with_dispatched_marker() {
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
        assert!(entries.iter().any(|e| matches!(&e.event,
            RunEvent::StepIntent { action, .. } if action == "call:slow-routine")));
    }

    #[tokio::test]
    async fn sync_call_failure_propagates_verbatim() {
        let invoker = Arc::new(FakeInvoker::new().error("broken", "child run failed at step s2: CAT: PTT stuck"));
        let dir = tempfile::tempdir().unwrap();
        let (ctx, _) = ctx_with(invoker, dir.path());
        let track = Track { name: "t".into(), steps: vec![call_step("s1", "broken", json!({}), true)] };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(matches!(err, crate::error::StepError::Action { cause, .. } if cause.contains("PTT stuck")));
    }

    #[tokio::test]
    async fn depth_cap_stops_runaway_recursion() {
        let invoker = Arc::new(FakeInvoker::new().result("x", json!({})));
        let dir = tempfile::tempdir().unwrap();
        let (mut ctx, _) = ctx_with(invoker, dir.path());
        ctx.depth = 8; // already at the cap
        let track = Track { name: "t".into(), steps: vec![call_step("s1", "x", json!({}), true)] };
        let mut vars = RunVars::default();
        let err = run_track(&track, &mut vars, &ctx).await.unwrap_err();
        assert!(matches!(err, crate::error::StepError::Action { cause, .. } if cause.contains("depth")));
    }
}
