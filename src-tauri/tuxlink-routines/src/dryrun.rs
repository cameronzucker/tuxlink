//! Dry-run registry swap (spec §10 layer 3, plan-3 task 5): a dry run is
//! `Engine::start_run` (`engine.rs`) with a registry where every REAL
//! action has been replaced 1:1 by a `FakeAction` (`fakes.rs`) mirroring
//! its declared capability flags — "one mechanism" (spec §10, §15): the
//! executor never knows or cares whether the `Arc<dyn Action>` it resolved
//! from the registry is real or scripted.
//!
//! v1 rule (spec: "a dry-run touches NOTHING real"): every descriptor in
//! the real registry gets a fake, full stop — there is no partial dry-run
//! that lets some actions through to real infrastructure.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::action::{ActionDescriptor, ActionRegistry};
use crate::fakes::FakeAction;

/// One scripted outcome for a `FakeAction` swapped in for a real action.
/// Mirrors `fakes.rs`'s `Outcome` shape but is public (the script is
/// authored by callers — engine adapters, MCP, UI dry-run panels — outside
/// this crate's test-only fakes).
#[derive(Debug, Clone, PartialEq)]
pub enum DryRunOutcome {
    Ok(Value),
    Err(String),
}

/// What an unscripted action (no entry in `DryRunScript.outcomes`, or an
/// entry whose list is empty) returns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DryRunDefault {
    /// Every unscripted action succeeds: `{"dry_run": true}`, plus
    /// `"connected": true` for any descriptor with `needs_radio: true` (so
    /// a routine's `Branch` on `<step>.connected` takes its "it worked"
    /// arm by default without the caller having to script every radio
    /// step by hand).
    #[default]
    Optimistic,
    /// Every unscripted action fails with a fixed, recognizable cause —
    /// useful for "does this routine's error handling actually work"
    /// dry-runs.
    Pessimistic,
}

/// Per-action scripted outcome queues (keyed by `ActionDescriptor.name`)
/// plus the default policy for any action with no queue (or an exhausted
/// one — `FakeAction` itself handles "replay in order, then repeat the
/// last queued outcome", so an action with a NON-empty queue never falls
/// through to `default` even after the queue is nominally exhausted).
#[derive(Debug, Clone, Default)]
pub struct DryRunScript {
    pub outcomes: HashMap<String, Vec<DryRunOutcome>>,
    pub default: DryRunDefault,
}

impl DryRunScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue outcomes for one action name, replacing any previously queued
    /// outcomes for that name.
    pub fn with_outcomes(mut self, action: &str, outcomes: Vec<DryRunOutcome>) -> Self {
        self.outcomes.insert(action.to_string(), outcomes);
        self
    }

    pub fn with_default(mut self, default: DryRunDefault) -> Self {
        self.default = default;
        self
    }
}

/// Build a fresh `ActionRegistry` where every entry in `real_descriptors`
/// is backed by a scripted `FakeAction` with MIRRORED capability flags
/// (`needs_radio`/`transmits`/`needs_internet` copied verbatim from the
/// real descriptor) — the validator's capability checks and the executor's
/// radio-lease bookkeeping see exactly the same declared shape a dry run
/// would see for real, only the `execute()` body is swapped.
///
/// `FakeAction::new` takes `&'static str`; `ActionDescriptor.name` already
/// IS `&'static str` (the real registries only ever register `'static`
/// descriptors — `action.rs`), so `descriptor.name` threads through
/// directly with no leak/alloc trick required.
pub fn build_dryrun_registry(
    real_descriptors: &[ActionDescriptor],
    script: DryRunScript,
) -> ActionRegistry {
    let mut registry = ActionRegistry::default();
    for descriptor in real_descriptors {
        let mut fake = FakeAction::new(descriptor.name).with_capabilities(
            descriptor.needs_radio,
            descriptor.transmits,
            descriptor.needs_internet,
        );
        match script.outcomes.get(descriptor.name) {
            Some(queued) if !queued.is_empty() => {
                for outcome in queued {
                    fake = match outcome {
                        DryRunOutcome::Ok(v) => fake.ok(v.clone()),
                        DryRunOutcome::Err(cause) => fake.err(cause),
                    };
                }
            }
            _ => {
                fake = apply_default(fake, descriptor, script.default);
            }
        }
        registry.register(Arc::new(fake));
    }
    registry
}

fn apply_default(
    fake: FakeAction,
    descriptor: &ActionDescriptor,
    default: DryRunDefault,
) -> FakeAction {
    match default {
        DryRunDefault::Optimistic => {
            let mut payload = json!({"dry_run": true});
            if descriptor.needs_radio {
                payload["connected"] = json!(true);
            }
            fake.ok(payload)
        }
        DryRunDefault::Pessimistic => fake.err("dry-run scripted failure"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    const RADIO_CONNECT: ActionDescriptor = ActionDescriptor {
        name: "radio.connect",
        needs_radio: true,
        transmits: true,
        needs_internet: false,
    };
    const LOCAL_LOG: ActionDescriptor = ActionDescriptor {
        name: "local.log",
        needs_radio: false,
        transmits: false,
        needs_internet: false,
    };
    const DATA_LOOKUP: ActionDescriptor = ActionDescriptor {
        name: "data.web_lookup",
        needs_radio: false,
        transmits: false,
        needs_internet: true,
    };

    #[test]
    fn every_real_descriptor_gets_a_mirrored_fake() {
        let registry = build_dryrun_registry(
            &[RADIO_CONNECT, LOCAL_LOG, DATA_LOOKUP],
            DryRunScript::new(),
        );
        assert!(registry.get("radio.connect").is_some());
        assert!(registry.get("local.log").is_some());
        assert!(registry.get("data.web_lookup").is_some());
        assert!(registry.get("nonexistent").is_none());

        let descriptors = registry.descriptors();
        assert_eq!(descriptors.len(), 3);
        for d in &descriptors {
            let original = [RADIO_CONNECT, LOCAL_LOG, DATA_LOOKUP]
                .into_iter()
                .find(|o| o.name == d.name)
                .unwrap();
            assert_eq!(d.needs_radio, original.needs_radio, "{}", d.name);
            assert_eq!(d.transmits, original.transmits, "{}", d.name);
            assert_eq!(d.needs_internet, original.needs_internet, "{}", d.name);
        }
    }

    #[tokio::test]
    async fn unscripted_non_radio_action_optimistic_default_has_no_connected_key() {
        let registry = build_dryrun_registry(&[LOCAL_LOG], DryRunScript::new());
        let fake = registry.get("local.log").unwrap();
        let out = fake
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out, json!({"dry_run": true}));
        assert!(out.get("connected").is_none());
    }

    #[tokio::test]
    async fn unscripted_radio_action_optimistic_default_includes_connected_true() {
        let registry = build_dryrun_registry(&[RADIO_CONNECT], DryRunScript::new());
        let fake = registry.get("radio.connect").unwrap();
        let out = fake
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(out, json!({"dry_run": true, "connected": true}));
    }

    #[tokio::test]
    async fn unscripted_action_pessimistic_default_errors() {
        let script = DryRunScript::new().with_default(DryRunDefault::Pessimistic);
        let registry = build_dryrun_registry(&[RADIO_CONNECT], script);
        let fake = registry.get("radio.connect").unwrap();
        let err = fake
            .execute(json!({}), CancellationToken::new())
            .await
            .unwrap_err();
        match err {
            crate::error::StepError::Action { cause, .. } => {
                assert_eq!(cause, "dry-run scripted failure");
            }
            other => panic!("expected scripted action error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn scripted_outcomes_replay_in_order_then_repeat_the_last() {
        let script = DryRunScript::new().with_outcomes(
            "radio.connect",
            vec![
                DryRunOutcome::Err("VARA: BUSY channel occupied".into()),
                DryRunOutcome::Ok(json!({"connected": true, "gateway": "W7DEF-10"})),
            ],
        );
        let registry = build_dryrun_registry(&[RADIO_CONNECT], script);
        let fake = registry.get("radio.connect").unwrap();
        let cancel = CancellationToken::new();

        let first = fake.execute(json!({}), cancel.clone()).await;
        match first {
            Err(crate::error::StepError::Action { cause, .. }) => {
                assert_eq!(cause, "VARA: BUSY channel occupied");
            }
            other => panic!("expected scripted error first, got {other:?}"),
        }

        let second = fake.execute(json!({}), cancel.clone()).await.unwrap();
        assert_eq!(second["connected"], json!(true));

        // Queue exhausted: repeats the last queued outcome (fakes.rs's own
        // replay contract), not a fall-through to the default policy.
        let third = fake.execute(json!({}), cancel).await.unwrap();
        assert_eq!(third["gateway"], json!("W7DEF-10"));
    }

    #[test]
    fn a_scripted_but_empty_outcome_list_falls_through_to_the_default() {
        let script = DryRunScript::new().with_outcomes("radio.connect", vec![]);
        let registry = build_dryrun_registry(&[RADIO_CONNECT], script);
        let descriptors = registry.descriptors();
        assert_eq!(descriptors.len(), 1);
        // Behavior verified via unscripted_radio_action_* above; this test
        // just proves the empty-vec branch didn't panic/skip registration.
        assert!(registry.get("radio.connect").is_some());
    }
}
