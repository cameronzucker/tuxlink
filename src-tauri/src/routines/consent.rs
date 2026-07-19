//! Transmit-consent enforcement — the Part 97 layer (plan 2 Task 5b, spec §4).
//!
//! Consent is a per-routine, design-time property mirroring Part 97's own
//! attended/automatic vocabulary (§97.109, §97.221). This module holds the two
//! runtime halves of that model:
//!
//! 1. [`closure_transmits`] — the transmit-closure predicate the start gate
//!    ([`super::session::RoutinesState::start_routine`]) evaluates: does a
//!    routine's call-graph closure contain a transmit step? A routine whose
//!    closure transmits is a *transmitting routine* and must declare a mode;
//!    an unacknowledged automatic one is not a startable state (spec §4).
//!    This is the canonical definition of "transmitting routine" for the whole
//!    monolith; plan 3's validator (`validate/consent.rs`, not yet built) MUST
//!    mirror THIS walk so enforcement and validation never disagree about which
//!    routines transmit.
//!
//! 2. [`ConsentRegistry`] — the attended-mode pause, implemented as the
//!    engine's [`ConsentPort`]. When an attended run reaches a `transmits: true`
//!    step, the executor ([`tuxlink_routines::executor::run_action_step_shared`])
//!    parks on this registry — a WAITING state (`RunState::AwaitingConsent`)
//!    entered **before** the per-step timeout, so parked-awaiting-consent time
//!    is never charged against the transmit step's timeout. The park resolves
//!    when the operator grants ([`ConsentRegistry::grant`], reached from the UI
//!    command); a run cancelled while parked drops the park future, whose RAII
//!    guard releases the parked slot so no stale grant sender leaks.
//!
//! ## Why parking lives in the executor, not an action wrapper
//!
//! An earlier design wrapped each transmit action in a `ConsentGated` `Action`
//! that parked *inside* `execute`. But the executor wraps `execute` in
//! `tokio::time::timeout`, so the parked wait was charged against the step
//! timeout — an attended transmit step parked at 03:00 failed after the timeout
//! elapsed instead of waiting (spec §8 defines `AwaitingConsent` as a WAITING
//! state and the timeout as a wedged-transport backstop, not a consent clock).
//! Moving the park into the executor, before the timeout, makes the wait a true
//! waiting state and lets the journal record `AwaitingConsent` → `Running`
//! honestly. This registry is now just the [`ConsentPort`] the executor calls.
//!
//! ## MCP (spec §13)
//!
//! [`ConsentRegistry::grant`] is reachable only from the operator UI command
//! (`routines_consent_grant`, wired in Task 6). The MCP surface has NO
//! parameter that can supply consent — the design-time acknowledgment is the
//! only consent envelope MCP touches, and it is recorded by a UI act. This
//! module never exposes a grant path an agent could reach.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::oneshot;

use tuxlink_routines::action::ActionRegistry;
use tuxlink_routines::consent::ConsentPort;
use tuxlink_routines::consent_closure::{closure_digest, consent_closure};
use tuxlink_routines::error::StepError;
use tuxlink_routines::journal::ParkKind;
use tuxlink_routines::types::{RoutineDef, TransmitAck};

use super::events::{RoutinesEvent, RoutinesEventSink};

/// A routine-name → definition lookup (the [`super::store::DefinitionStore`]'s
/// `get`), used by [`closure_transmits`] to walk `Call` steps into their
/// callees. Kept as a bare trait object so the closure walk is pure and
/// unit-testable against a `HashMap`-backed fake.
pub type DefLookup<'a> = dyn Fn(&str) -> Option<RoutineDef> + 'a;

/// A catalog-action-name → `transmits` predicate (the engine registry's
/// descriptors). Unknown names resolve `false` (conservative: a routine
/// referencing an action absent from the catalog will fail at execute anyway;
/// the closure never over-claims transmission from an unresolvable action).
pub type TransmitsPredicate<'a> = dyn Fn(&str) -> bool + 'a;

/// Does `def`'s call-graph closure contain a transmit step (spec §4, §10)?
///
/// A thin boolean over the shared
/// [`consent_closure`](tuxlink_routines::consent_closure::consent_closure)
/// walk: the closure transmits iff enumerating it with the `transmits`
/// relevance predicate yields at least one step. `lookup` resolves `Call`
/// targets; an unresolved callee contributes nothing (conservative — the
/// validator flags the unresolved reference). The shared walk is
/// global-visited and depth-capped at
/// [`MAX_CALL_DEPTH`](tuxlink_routines::compose::MAX_CALL_DEPTH), matching the
/// runtime backstop the executor applies to `Control::Call`.
pub fn closure_transmits(
    def: &RoutineDef,
    lookup: &DefLookup<'_>,
    transmits: &TransmitsPredicate<'_>,
) -> bool {
    !consent_closure(def, lookup, transmits).steps.is_empty()
}

/// Does `def`'s call-graph closure contain a config-write step (C3, spec §4)?
/// The `writes_config` sibling of [`closure_transmits`], over the same shared
/// [`consent_closure`](tuxlink_routines::consent_closure::consent_closure) walk
/// with the `writes` relevance predicate.
pub fn closure_writes(
    def: &RoutineDef,
    lookup: &DefLookup<'_>,
    writes: &TransmitsPredicate<'_>,
) -> bool {
    !consent_closure(def, lookup, writes).steps.is_empty()
}

/// The sha256 hex digest of `def`'s consent closure for the given relevance
/// predicate (C3), computed over the SAME shared walk + canonicalizing digest
/// the validator uses — so a digest the monolith records at ack time and a
/// digest the leaf validator recomputes at validation time are byte-identical
/// (enforcement and validation can never disagree about "is this ack current").
/// `is_relevant` selects the class: transmit (a `transmits` predicate) or
/// config-write (a `writes_config` predicate).
pub fn closure_digest_for(
    def: &RoutineDef,
    lookup: &DefLookup<'_>,
    is_relevant: &TransmitsPredicate<'_>,
) -> String {
    closure_digest(&consent_closure(def, lookup, is_relevant))
}

/// Does `ack` bind the exact `live_digest` the closure currently hashes to
/// (C3)? The monolith mirror of the leaf validator's `ack_binds_closure`,
/// covering all three stale clauses at once: **missing** (`None`), **empty**
/// (blank `by`/`at`), and **digest-mismatched** (a re-edited closure, OR a
/// digest-less legacy ack whose `closure_digest` is `None` and thus never
/// equals a live digest). The start gate uses this so enforcement refuses on
/// the same grounds the validator flags.
pub fn ack_binds(ack: &Option<TransmitAck>, live_digest: &str) -> bool {
    match ack {
        Some(a) => {
            !a.by.trim().is_empty()
                && !a.at.trim().is_empty()
                && a.closure_digest.as_deref() == Some(live_digest)
        }
        None => false,
    }
}

/// The map of parked transmit steps, keyed by `(run_id, step_id)` → the
/// one-shot sender that grants its consent.
type ParkMap = HashMap<(String, String), oneshot::Sender<()>>;

/// The parking desk for attended-mode transmit steps, and the engine's
/// [`ConsentPort`]. A parked step registers a one-shot sender keyed by
/// `(run_id, step_id)` and emits [`RoutinesEvent::AwaitingConsent`];
/// [`grant`](Self::grant) fires the sender. The map's critical sections are a
/// single op each, no `await` held across the lock.
pub struct ConsentRegistry {
    parked: Mutex<ParkMap>,
    sink: Arc<dyn RoutinesEventSink>,
}

impl ConsentRegistry {
    /// Build a registry that emits its `AwaitingConsent` events into `sink`
    /// (the same run-lifecycle sink the session bridge uses).
    pub fn new(sink: Arc<dyn RoutinesEventSink>) -> Self {
        Self {
            parked: Mutex::new(HashMap::new()),
            sink,
        }
    }

    /// Grant consent for a parked transmit step. Returns `true` iff a step was
    /// actually waiting (so the command layer can report "nothing to grant").
    pub fn grant(&self, run_id: &str, step_id: &str) -> bool {
        let sender = self
            .lock()
            .remove(&(run_id.to_string(), step_id.to_string()));
        match sender {
            Some(tx) => {
                // `send` errs only if the receiver was already dropped (the run
                // was cancelled between grant lookup and here) — harmless.
                let _ = tx.send(());
                true
            }
            None => false,
        }
    }

    /// How many steps are currently parked (test/introspection helper).
    pub fn parked_count(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ParkMap> {
        self.parked.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl ConsentPort for ConsentRegistry {
    async fn park(&self, run_id: &str, step_id: &str, kind: ParkKind) -> Result<(), StepError> {
        let key = (run_id.to_string(), step_id.to_string());
        // Register the grant channel BEFORE emitting the event, so a grant
        // racing in right after the UI receives `AwaitingConsent` finds the
        // parked sender.
        let rx = {
            let (tx, rx) = oneshot::channel();
            self.lock().insert(key.clone(), tx);
            rx
        };
        self.sink.emit(&RoutinesEvent::AwaitingConsent {
            run_id: run_id.to_string(),
            step_id: step_id.to_string(),
            park_kind: kind,
        });
        // RAII: if this future is dropped before a grant (the executor takes
        // its cancel branch while parked), release the parked slot so no stale
        // sender leaks. On a normal grant the entry is already gone (removed by
        // `grant`), so the guard's remove is an idempotent no-op.
        let _guard = ParkGuard {
            parked: &self.parked,
            key,
        };
        match rx.await {
            // Operator confirmed: the executor proceeds into the timed execute.
            Ok(()) => Ok(()),
            // Sender dropped without a grant (grant channel torn down): treat as
            // a cancel — the transmit action never executes.
            Err(_) => Err(StepError::Cancelled),
        }
    }
}

/// RAII cleanup for a parked slot: removes the `(run_id, step_id)` entry when
/// the park future is dropped, so a cancelled park leaves no orphaned grant
/// sender behind (the leak the executor-side restructure closes).
struct ParkGuard<'a> {
    parked: &'a Mutex<ParkMap>,
    key: (String, String),
}

impl Drop for ParkGuard<'_> {
    fn drop(&mut self) {
        self.parked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}

/// The set of catalog action names that transmit, captured from a registry's
/// descriptors. The start gate's [`closure_transmits`] predicate reads this
/// (`|name| set.contains(name)`) instead of holding the registry, so a routine
/// definition never needs a live `Arc<dyn Action>` to be consent-checked.
pub fn transmit_action_names(registry: &ActionRegistry) -> HashSet<String> {
    registry
        .descriptors()
        .into_iter()
        .filter(|d| d.transmits)
        .map(|d| d.name.to_string())
        .collect()
}

/// The set of catalog action names that write station config (`writes_config`,
/// C3), captured from a registry's descriptors — the `writes_config` sibling of
/// [`transmit_action_names`]. The write-consent start gate's `writes` predicate
/// reads this the same way the transmit gate reads the transmit set.
pub fn write_action_names(registry: &ActionRegistry) -> HashSet<String> {
    registry
        .descriptors()
        .into_iter()
        .filter(|d| d.writes_config)
        .map(|d| d.name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use tuxlink_routines::action::ActionRegistry;
    use tuxlink_routines::fakes::FakeAction;

    fn def(json_str: &str) -> RoutineDef {
        RoutineDef::parse(json_str).unwrap()
    }

    fn no_lookup(_: &str) -> Option<RoutineDef> {
        None
    }

    /// A recording sink for registry tests.
    #[derive(Default)]
    struct RecSink {
        events: Mutex<Vec<RoutinesEvent>>,
    }
    impl RoutinesEventSink for RecSink {
        fn emit(&self, e: &RoutinesEvent) {
            self.events.lock().unwrap().push(e.clone());
        }
    }

    fn registry_with(sink: Arc<RecSink>) -> Arc<ConsentRegistry> {
        let dyn_sink: Arc<dyn RoutinesEventSink> = sink;
        Arc::new(ConsentRegistry::new(dyn_sink))
    }

    async fn wait_parked(reg: &ConsentRegistry) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while reg.parked_count() == 0 {
            assert!(std::time::Instant::now() < deadline, "never parked");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    // ── closure_transmits ────────────────────────────────────────────────

    const TX_ROUTINE: &str = r#"{
      "routine": "tx", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "radio.connect", "params": {} }
      ]}]
    }"#;

    const NO_TX_ROUTINE: &str = r#"{
      "routine": "quiet", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "local.log", "params": {} }
      ]}]
    }"#;

    fn transmits_radio_connect(name: &str) -> bool {
        name == "radio.connect"
    }

    #[test]
    fn direct_transmit_step_makes_the_closure_transmit() {
        assert!(closure_transmits(
            &def(TX_ROUTINE),
            &no_lookup,
            &transmits_radio_connect
        ));
    }

    #[test]
    fn a_routine_with_no_transmit_step_does_not_transmit() {
        assert!(!closure_transmits(
            &def(NO_TX_ROUTINE),
            &no_lookup,
            &transmits_radio_connect
        ));
    }

    #[test]
    fn transmission_propagates_through_a_call() {
        // parent calls "tx" (which transmits); parent itself has no TX step.
        let parent = def(
            r#"{
              "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "c1", "control": "call", "routine": "tx", "args": {}, "sync": true }
              ]}]
            }"#,
        );
        let mut lib: HashMap<String, RoutineDef> = HashMap::new();
        lib.insert("tx".into(), def(TX_ROUTINE));
        let lookup = |name: &str| lib.get(name).cloned();
        assert!(closure_transmits(&parent, &lookup, &transmits_radio_connect));
    }

    #[test]
    fn unresolved_call_contributes_no_transmission() {
        let parent = def(
            r#"{
              "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "c1", "control": "call", "routine": "ghost", "args": {}, "sync": true }
              ]}]
            }"#,
        );
        assert!(!closure_transmits(&parent, &no_lookup, &transmits_radio_connect));
    }

    #[test]
    fn recursive_call_cycle_terminates() {
        // a → a: cycle guard prevents infinite recursion; no TX step present.
        let a = def(
            r#"{
              "routine": "a", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "c1", "control": "call", "routine": "a", "args": {}, "sync": true }
              ]}]
            }"#,
        );
        let mut lib: HashMap<String, RoutineDef> = HashMap::new();
        lib.insert("a".into(), a.clone());
        let lookup = |name: &str| lib.get(name).cloned();
        assert!(!closure_transmits(&a, &lookup, &transmits_radio_connect));
    }

    // ── ConsentRegistry (ConsentPort) ────────────────────────────────────

    #[tokio::test]
    async fn park_emits_awaiting_consent_and_grant_wakes_it() {
        let rec = Arc::new(RecSink::default());
        let reg = registry_with(rec.clone());

        let r2 = reg.clone();
        let task = tokio::spawn(async move { r2.park("run-1", "s1", ParkKind::Transmit).await });
        wait_parked(&reg).await;

        // The park emitted AwaitingConsent for the right (run, step, kind).
        assert!(rec.events.lock().unwrap().iter().any(|e| matches!(e,
            RoutinesEvent::AwaitingConsent { run_id, step_id, park_kind }
                if run_id == "run-1" && step_id == "s1" && *park_kind == ParkKind::Transmit)));

        assert!(reg.grant("run-1", "s1"));
        let res = task.await.unwrap();
        assert!(res.is_ok(), "park resolves Ok on grant");
        assert_eq!(reg.parked_count(), 0, "grant removes the parked entry");
    }

    /// The park kind threads through the registry into the emitted event: a
    /// `ParkKind::Write` park surfaces `AwaitingConsent { park_kind: Write }`
    /// so the ConsentGate renders config-write copy, not transmit copy (C2).
    #[tokio::test]
    async fn write_park_emits_awaiting_consent_with_write_kind() {
        let rec = Arc::new(RecSink::default());
        let reg = registry_with(rec.clone());

        let r2 = reg.clone();
        let task = tokio::spawn(async move { r2.park("run-w", "s1", ParkKind::Write).await });
        wait_parked(&reg).await;

        assert!(rec.events.lock().unwrap().iter().any(|e| matches!(e,
            RoutinesEvent::AwaitingConsent { run_id, park_kind, .. }
                if run_id == "run-w" && *park_kind == ParkKind::Write)));

        assert!(reg.grant("run-w", "s1"));
        assert!(task.await.unwrap().is_ok());
    }

    #[test]
    fn grant_of_an_unparked_step_is_false() {
        let reg = registry_with(Arc::new(RecSink::default()));
        assert!(!reg.grant("run-1", "nope"));
    }

    /// The registry leak guard (restructure #3): dropping a parked future
    /// without a grant releases the slot — no stale sender is left behind.
    #[tokio::test]
    async fn dropping_a_parked_future_releases_the_slot() {
        let rec = Arc::new(RecSink::default());
        let reg = registry_with(rec.clone());

        let r2 = reg.clone();
        // Never granted: parks and stays pending until the task is aborted.
        let task = tokio::spawn(async move { r2.park("run-1", "s1", ParkKind::Transmit).await });
        wait_parked(&reg).await;
        assert_eq!(reg.parked_count(), 1);

        // Drop the parked future (as the executor's cancel branch does): the
        // RAII guard must release the slot.
        task.abort();
        let _ = task.await;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while reg.parked_count() != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "parked slot not released on drop"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(reg.parked_count(), 0, "no stale sender leaked");
    }

    // ── transmit_action_names ────────────────────────────────────────────

    #[test]
    fn transmit_names_are_the_transmitting_descriptors() {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(
            FakeAction::new("radio.tx").with_capabilities(true, true, false).ok(json!({})),
        ));
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        let names = transmit_action_names(&reg);
        assert!(names.contains("radio.tx"));
        assert!(!names.contains("local.log"));
    }
}
