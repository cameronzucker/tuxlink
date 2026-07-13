//! Test doubles, public so later plans' tests and the dry-run layer reuse them.

use std::sync::Mutex;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::action::{Action, ActionDescriptor};
use crate::error::StepError;

enum Outcome {
    Ok(serde_json::Value),
    Err(String),
    Hang,
}

/// Scriptable action: outcomes replay in the order queued; when the script
/// is exhausted the last outcome repeats.
pub struct FakeAction {
    name: &'static str,
    descriptor: ActionDescriptor,
    outcomes: Mutex<Vec<Outcome>>,
    calls: Mutex<Vec<serde_json::Value>>,
}

impl FakeAction {
    pub fn new(name: &'static str) -> Self {
        FakeAction {
            name,
            descriptor: ActionDescriptor { name, needs_radio: false, transmits: false, needs_internet: false },
            outcomes: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Override capability flags (for arbiter/validator tests in later plans).
    pub fn with_capabilities(mut self, needs_radio: bool, transmits: bool, needs_internet: bool) -> Self {
        self.descriptor = ActionDescriptor { name: self.name, needs_radio, transmits, needs_internet };
        self
    }

    pub fn ok(self, output: serde_json::Value) -> Self {
        self.outcomes.lock().unwrap().push(Outcome::Ok(output));
        self
    }

    pub fn err(self, verbatim_cause: &str) -> Self {
        self.outcomes.lock().unwrap().push(Outcome::Err(verbatim_cause.to_string()));
        self
    }

    pub fn hang(self) -> Self {
        self.outcomes.lock().unwrap().push(Outcome::Hang);
        self
    }

    pub fn calls(&self) -> Vec<serde_json::Value> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Action for FakeAction {
    fn descriptor(&self) -> ActionDescriptor {
        self.descriptor
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, StepError> {
        self.calls.lock().unwrap().push(params);
        let outcome = {
            let mut outcomes = self.outcomes.lock().unwrap();
            if outcomes.len() > 1 {
                outcomes.remove(0)
            } else {
                match outcomes.first() {
                    Some(Outcome::Ok(v)) => Outcome::Ok(v.clone()),
                    Some(Outcome::Err(s)) => Outcome::Err(s.clone()),
                    Some(Outcome::Hang) => Outcome::Hang,
                    None => Outcome::Ok(serde_json::json!({})),
                }
            }
        };
        match outcome {
            Outcome::Ok(v) => Ok(v),
            Outcome::Err(cause) => Err(StepError::Action { action: self.name.to_string(), cause }),
            Outcome::Hang => {
                cancel.cancelled().await;
                Err(StepError::Cancelled)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, ActionRegistry};
    use crate::error::StepError;
    use serde_json::json;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn fake_action_replays_scripted_outcomes_in_order() {
        let fake = FakeAction::new("radio.connect")
            .err("VARA: BUSY channel occupied")
            .ok(json!({"connected": true, "gateway": "W7DEF-10"}));
        let cancel = CancellationToken::new();

        let first = fake.execute(json!({"try": 1}), cancel.clone()).await;
        match first {
            Err(StepError::Action { cause, .. }) => assert_eq!(cause, "VARA: BUSY channel occupied"),
            other => panic!("expected scripted error, got {other:?}"),
        }
        let second = fake.execute(json!({"try": 2}), cancel).await.unwrap();
        assert_eq!(second["gateway"], "W7DEF-10");
        assert_eq!(fake.calls().len(), 2);
        assert_eq!(fake.calls()[0]["try"], 1);
    }

    #[tokio::test]
    async fn hang_outcome_blocks_until_cancelled() {
        let fake = FakeAction::new("radio.connect").hang();
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        let task = tokio::spawn(async move { fake.execute(json!({}), c2).await });
        cancel.cancel();
        let res = task.await.unwrap();
        assert!(matches!(res, Err(StepError::Cancelled)));
    }

    #[tokio::test]
    async fn registry_resolves_by_name() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        assert!(reg.get("local.log").is_some());
        assert!(reg.get("nope").is_none());
        assert_eq!(reg.descriptors().len(), 1);
    }
}
