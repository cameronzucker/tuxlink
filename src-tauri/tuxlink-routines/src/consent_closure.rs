//! One parameterized consent-closure walk + a canonical closure digest
//! (round-2 ranks C1, spec §4).
//!
//! Two duplicated call-graph traversals used to answer "does this routine's
//! closure contain a transmit step?" — one in the monolith's start gate
//! (`src/routines/consent.rs::closure_transmits`) and one in the leaf
//! validator (`validate/consent.rs::scan_routine_for_transmit`). They
//! disagreed on cycle bookkeeping: the monolith walk was path-scoped with a
//! [`MAX_CALL_DEPTH`](crate::compose::MAX_CALL_DEPTH) cap, the validator walk
//! used a global visited-set with **no** cap. This module unifies them into
//! ONE parameterized walk — `is_relevant` decides which action class the
//! closure is being computed for (transmit today; `writes_config` in C2) —
//! that a caller reduces to a boolean (`!steps.is_empty()`) or enumerates.
//!
//! **Traversal decree (the two old walks cannot both be matched):** the
//! shared walk uses a GLOBAL visited-set (each routine walked at most once,
//! for deterministic enumeration) plus the `MAX_CALL_DEPTH` cap. The monolith
//! gate keeps boolean-equivalent behavior; the leaf validator GAINS the depth
//! cap it lacked, aligning it with the runtime gate (an intended, small
//! behavior change).
//!
//! [`closure_digest`] hashes the enumerated closure so an operator's
//! acknowledgment can be bound to exactly what they signed: the tuple
//! `(routine, step, action, params)` per relevant step plus every call edge
//! `(routine, step, callee, args)` on a path that reaches a relevant step.
//! `track` is carried for validator findings but EXCLUDED from the hash.
//! Each `params`/`args` value is canonicalized (recursive key-sort + a
//! canonical re-serialization that never relies on serde_json's map ordering)
//! before hashing, so a re-ordered-but-equal JSON object yields the same
//! digest; tuples are sorted by `(routine, step)`; the output is sha256 hex.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::compose::MAX_CALL_DEPTH;
use crate::types::{Control, RoutineDef, Step, StepId};

/// A relevant step found somewhere in a routine's closure. `routine` names
/// whichever routine actually owns the step (may differ from the root if the
/// step lives behind one or more `Call` hops). `track` is carried for
/// validator findings (`.with_track`) and is EXCLUDED from [`closure_digest`].
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureStep {
    pub routine: String,
    pub track: String,
    pub step: StepId,
    pub action: String,
    pub params: Value,
}

/// A `Call` edge on a path that reaches a relevant step. `routine` is the
/// caller, `step` the call step's id, `callee` the invoked routine name,
/// `args` the call arguments. Included in the digest so re-pointing or
/// re-arging a call that leads to a relevant step invalidates a prior ack.
#[derive(Debug, Clone, PartialEq)]
pub struct CallEdge {
    pub routine: String,
    pub step: StepId,
    pub callee: String,
    pub args: Value,
}

/// The enumerated consent closure for one relevance class.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsentClosure {
    pub steps: Vec<ClosureStep>,
    pub call_edges: Vec<CallEdge>,
}

/// Walk `root`'s call-graph closure, collecting every step whose action name
/// satisfies `is_relevant` plus every `Call` edge on a path that reaches one.
///
/// `lookup` resolves a `Call`'s target routine (an unresolved callee
/// contributes nothing — conservative, matching both old walks). The walk is
/// global-visited (each routine at most once) and depth-capped at
/// [`MAX_CALL_DEPTH`]; see the module doc's traversal decree.
pub fn consent_closure(
    root: &RoutineDef,
    lookup: &dyn Fn(&str) -> Option<RoutineDef>,
    is_relevant: &dyn Fn(&str) -> bool,
) -> ConsentClosure {
    let mut acc = ConsentClosure::default();
    let mut visited = HashSet::new();
    let mut reaches = HashMap::new();
    walk(root, lookup, is_relevant, &mut visited, &mut reaches, 0, &mut acc);
    acc
}

/// Recurse `def`'s tracks. Returns whether this routine (or anything it calls)
/// reached a relevant step — the caller uses that to decide whether to record
/// the `Call` edge that led here.
///
/// **All-edges recording (C3 hardening, closes the C1 "only first edge" gap):**
/// the routine BODY is walked at most once (the `visited` guard is the
/// termination invariant — a callee's relevant steps are enumerated exactly
/// once, keeping deterministic enumeration). But every `Call` edge on a path
/// that reaches a relevant step is recorded, INCLUDING edges to an
/// already-walked callee. When a callee is `visited`-deduped, the walk does not
/// re-enumerate its steps; it consults the `reaches` cache (a routine's
/// finalized "does my closure reach a relevant step?" answer) and records the
/// edge iff that answer is `true`. So routine A calling callee B at two steps
/// with different args records BOTH edges — and each edge's `args` feed the
/// digest, so re-arging the second call invalidates a prior acknowledgment.
///
/// A cycle member still on the recursion stack has no finalized `reaches`
/// answer yet; the cache lookup misses and is treated as `false` (conservative,
/// preserving the old cycle behavior — an in-progress node contributes no edge
/// back to itself).
fn walk(
    def: &RoutineDef,
    lookup: &dyn Fn(&str) -> Option<RoutineDef>,
    is_relevant: &dyn Fn(&str) -> bool,
    visited: &mut HashSet<String>,
    reaches: &mut HashMap<String, bool>,
    depth: u32,
    acc: &mut ConsentClosure,
) -> bool {
    // Depth check BEFORE the visited insert: a routine skipped for depth on
    // one path can still be walked via a shallower path (mirrors the old
    // monolith walk's ordering).
    if depth > MAX_CALL_DEPTH {
        return false;
    }
    if !visited.insert(def.routine.clone()) {
        // Already walked (or in progress on a cycle): do NOT re-enumerate its
        // body, but return the cached "reaches a relevant step" answer so the
        // caller can still record THIS call edge. In-progress cycle members
        // are not yet cached -> conservative `false` (old cycle behavior).
        return reaches.get(&def.routine).copied().unwrap_or(false);
    }

    let mut reached = false;
    for track in &def.tracks {
        for step in &track.steps {
            match step {
                Step::Action(a) => {
                    if is_relevant(&a.action) {
                        acc.steps.push(ClosureStep {
                            routine: def.routine.clone(),
                            track: track.name.clone(),
                            step: a.id.clone(),
                            action: a.action.clone(),
                            params: a.params.clone(),
                        });
                        reached = true;
                    }
                }
                Step::Control(cs) => {
                    if let Control::Call { routine, args, .. } = &cs.control {
                        if let Some(child) = lookup(routine) {
                            if walk(
                                &child,
                                lookup,
                                is_relevant,
                                visited,
                                reaches,
                                depth + 1,
                                acc,
                            ) {
                                acc.call_edges.push(CallEdge {
                                    routine: def.routine.clone(),
                                    step: cs.id.clone(),
                                    callee: routine.clone(),
                                    args: args.clone(),
                                });
                                reached = true;
                            }
                        }
                    }
                }
            }
        }
    }
    // Finalize this routine's cached answer now that its body is fully walked.
    reaches.insert(def.routine.clone(), reached);
    reached
}

/// sha256 hex of the canonicalized closure. Hashes exactly
/// `(routine, step, action, params)` per step and `(routine, step, callee,
/// args)` per call edge; `track` is excluded. Tuples are sorted by
/// `(routine, step)`; each JSON value is canonicalized (recursive key-sort)
/// so map key order never affects the digest.
pub fn closure_digest(c: &ConsentClosure) -> String {
    let mut steps: Vec<&ClosureStep> = c.steps.iter().collect();
    steps.sort_by(|a, b| {
        a.routine
            .cmp(&b.routine)
            .then_with(|| a.step.0.cmp(&b.step.0))
    });
    let mut edges: Vec<&CallEdge> = c.call_edges.iter().collect();
    edges.sort_by(|a, b| {
        a.routine
            .cmp(&b.routine)
            .then_with(|| a.step.0.cmp(&b.step.0))
    });

    let mut hasher = Sha256::new();
    for s in steps {
        hasher.update(b"S\x00");
        feed(&mut hasher, s.routine.as_bytes());
        feed(&mut hasher, s.step.0.as_bytes());
        feed(&mut hasher, s.action.as_bytes());
        let mut buf = String::new();
        write_canonical(&s.params, &mut buf);
        feed(&mut hasher, buf.as_bytes());
    }
    for e in edges {
        hasher.update(b"E\x00");
        feed(&mut hasher, e.routine.as_bytes());
        feed(&mut hasher, e.step.0.as_bytes());
        feed(&mut hasher, e.callee.as_bytes());
        let mut buf = String::new();
        write_canonical(&e.args, &mut buf);
        feed(&mut hasher, buf.as_bytes());
    }

    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Feed one length-delimited field into the hasher (NUL terminator; the
/// canonical JSON never contains a raw NUL, and routine/step/action/callee
/// names are field-separated so no two field boundaries can alias).
fn feed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes);
    hasher.update(b"\x00");
}

/// Serialize `v` canonically: object keys sorted recursively, arrays left in
/// order (element order is significant), scalars via their JSON forms. Never
/// relies on serde_json's `Map` iteration order, so it is correct whether or
/// not the `preserve_order` feature is enabled anywhere in the build graph.
fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap_or_default()),
        Value::Array(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(e, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                write_canonical(&m[*k], out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn def(json_str: &str) -> RoutineDef {
        RoutineDef::parse(json_str).unwrap()
    }

    fn no_lookup(_: &str) -> Option<RoutineDef> {
        None
    }

    fn transmits_radio_connect(name: &str) -> bool {
        name == "radio.connect"
    }

    /// A one-track routine with a single `radio.connect` (relevant) step whose
    /// params are supplied verbatim from `params_json`.
    fn tx_routine(name: &str, params_json: &str) -> RoutineDef {
        def(&format!(
            r#"{{
              "routine": "{name}", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
              "tracks": [{{ "name": "t", "steps": [
                {{ "id": "s1", "action": "radio.connect", "params": {params_json} }}
              ]}}]
            }}"#
        ))
    }

    // ── enumeration ──────────────────────────────────────────────────────

    #[test]
    fn relevant_step_is_enumerated_with_its_params() {
        let c = consent_closure(
            &tx_routine("tx", r#"{"bands":["40m"],"listen":5}"#),
            &no_lookup,
            &transmits_radio_connect,
        );
        assert_eq!(c.steps.len(), 1);
        assert_eq!(c.steps[0].routine, "tx");
        assert_eq!(c.steps[0].track, "t");
        assert_eq!(c.steps[0].step, StepId("s1".into()));
        assert_eq!(c.steps[0].action, "radio.connect");
        assert_eq!(c.steps[0].params, json!({"bands":["40m"],"listen":5}));
        assert!(c.call_edges.is_empty());
    }

    #[test]
    fn irrelevant_only_routine_has_empty_closure() {
        let quiet = def(
            r#"{
              "routine": "quiet", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "s1", "action": "local.log", "params": {} }
              ]}]
            }"#,
        );
        let c = consent_closure(&quiet, &no_lookup, &transmits_radio_connect);
        assert!(c.steps.is_empty());
        assert!(c.call_edges.is_empty());
    }

    #[test]
    fn call_edge_recorded_only_on_a_path_reaching_a_relevant_step() {
        // parent -> tx (relevant) records the edge; parent -> quiet (no
        // relevant step) records NO edge.
        let parent = def(
            r#"{
              "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "cq", "control": "call", "routine": "quiet", "args": {"z":1}, "sync": true },
                { "id": "ctx", "control": "call", "routine": "tx", "args": {"y":2}, "sync": true }
              ]}]
            }"#,
        );
        let mut lib: HashMap<String, RoutineDef> = HashMap::new();
        lib.insert("tx".into(), tx_routine("tx", "{}"));
        lib.insert(
            "quiet".into(),
            def(
                r#"{
                  "routine": "quiet", "schema_version": 1, "transmit_mode": "attended",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
                  "tracks": [{ "name": "t", "steps": [
                    { "id": "s1", "action": "local.log", "params": {} }
                  ]}]
                }"#,
            ),
        );
        let lookup = |name: &str| lib.get(name).cloned();
        let c = consent_closure(&parent, &lookup, &transmits_radio_connect);
        assert_eq!(c.steps.len(), 1, "only tx's step is relevant");
        assert_eq!(c.call_edges.len(), 1, "only the tx call edge is recorded");
        assert_eq!(c.call_edges[0].routine, "parent");
        assert_eq!(c.call_edges[0].step, StepId("ctx".into()));
        assert_eq!(c.call_edges[0].callee, "tx");
        assert_eq!(c.call_edges[0].args, json!({"y":2}));
    }

    #[test]
    fn unresolved_call_contributes_nothing() {
        let parent = def(
            r#"{
              "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
              "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
              "tracks": [{ "name": "t", "steps": [
                { "id": "c1", "control": "call", "routine": "ghost", "args": {}, "sync": true }
              ]}]
            }"#,
        );
        let c = consent_closure(&parent, &no_lookup, &transmits_radio_connect);
        assert!(c.steps.is_empty());
        assert!(c.call_edges.is_empty());
    }

    // ── cycle + depth-cap parity (ported from the old walks) ──────────────

    #[test]
    fn recursive_cycle_terminates_and_finds_nothing() {
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
        let c = consent_closure(&a, &lookup, &transmits_radio_connect);
        assert!(c.steps.is_empty());
    }

    #[test]
    fn depth_cap_stops_a_chain_longer_than_max_call_depth() {
        // Build a linear chain r0 -> r1 -> ... -> rN where only the DEEPEST
        // routine, past the cap, transmits. With the cap in force the walk
        // never reaches it, so the closure stays empty (this is the behavior
        // the leaf validator GAINS over its old uncapped walk).
        let n = (MAX_CALL_DEPTH + 3) as usize;
        let mut lib: HashMap<String, RoutineDef> = HashMap::new();
        for i in 0..n {
            let name = format!("r{i}");
            let d = if i + 1 < n {
                let next = format!("r{}", i + 1);
                def(&format!(
                    r#"{{
                      "routine": "{name}", "schema_version": 1, "transmit_mode": "attended",
                      "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
                      "tracks": [{{ "name": "t", "steps": [
                        {{ "id": "c", "control": "call", "routine": "{next}", "args": {{}}, "sync": true }}
                      ]}}]
                    }}"#
                ))
            } else {
                tx_routine(&name, "{}")
            };
            lib.insert(name, d);
        }
        let root = lib.get("r0").cloned().unwrap();
        let lookup = |name: &str| lib.get(name).cloned();
        let c = consent_closure(&root, &lookup, &transmits_radio_connect);
        assert!(
            c.steps.is_empty(),
            "the transmit step is deeper than MAX_CALL_DEPTH and must be unreachable"
        );
    }

    #[test]
    fn a_relevant_step_within_the_cap_is_found() {
        // Same chain shape but the transmit routine sits AT the cap boundary,
        // so it IS reachable — guards against an off-by-one that hides real
        // transmit closures.
        let n = MAX_CALL_DEPTH as usize; // r0..r(cap-1) call chain, last transmits
        let mut lib: HashMap<String, RoutineDef> = HashMap::new();
        for i in 0..n {
            let name = format!("r{i}");
            let d = if i + 1 < n {
                let next = format!("r{}", i + 1);
                def(&format!(
                    r#"{{
                      "routine": "{name}", "schema_version": 1, "transmit_mode": "attended",
                      "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
                      "tracks": [{{ "name": "t", "steps": [
                        {{ "id": "c", "control": "call", "routine": "{next}", "args": {{}}, "sync": true }}
                      ]}}]
                    }}"#
                ))
            } else {
                tx_routine(&name, "{}")
            };
            lib.insert(name, d);
        }
        let root = lib.get("r0").cloned().unwrap();
        let lookup = |name: &str| lib.get(name).cloned();
        let c = consent_closure(&root, &lookup, &transmits_radio_connect);
        assert_eq!(c.steps.len(), 1, "the transmit step within the cap is found");
    }

    // ── digest: canonicalization + mutation sensitivity ───────────────────

    #[test]
    fn key_order_does_not_change_the_digest() {
        let a = consent_closure(
            &tx_routine("tx", r#"{"alpha":1,"beta":2,"nested":{"x":1,"y":2}}"#),
            &no_lookup,
            &transmits_radio_connect,
        );
        let b = consent_closure(
            &tx_routine("tx", r#"{"nested":{"y":2,"x":1},"beta":2,"alpha":1}"#),
            &no_lookup,
            &transmits_radio_connect,
        );
        assert_eq!(closure_digest(&a), closure_digest(&b));
    }

    #[test]
    fn a_param_mutation_flips_the_digest() {
        let a = consent_closure(
            &tx_routine("tx", r#"{"bands":["40m"]}"#),
            &no_lookup,
            &transmits_radio_connect,
        );
        let b = consent_closure(
            &tx_routine("tx", r#"{"bands":["80m"]}"#),
            &no_lookup,
            &transmits_radio_connect,
        );
        assert_ne!(closure_digest(&a), closure_digest(&b));
    }

    #[test]
    fn a_call_args_mutation_flips_the_digest() {
        let build = |args: &str| {
            let parent = def(&format!(
                r#"{{
                  "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
                  "tracks": [{{ "name": "t", "steps": [
                    {{ "id": "c1", "control": "call", "routine": "tx", "args": {args}, "sync": true }}
                  ]}}]
                }}"#
            ));
            let mut lib: HashMap<String, RoutineDef> = HashMap::new();
            lib.insert("tx".into(), tx_routine("tx", "{}"));
            let closure = {
                let lookup = |name: &str| lib.get(name).cloned();
                consent_closure(&parent, &lookup, &transmits_radio_connect)
            };
            closure_digest(&closure)
        };
        assert_ne!(build(r#"{"gain":1}"#), build(r#"{"gain":2}"#));
    }

    #[test]
    fn a_callee_mutation_flips_the_digest() {
        // Same call step id + args, but pointing at a different (still
        // transmitting) callee — the recorded edge's `callee` differs.
        let build = |callee: &str| {
            let parent = def(&format!(
                r#"{{
                  "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
                  "tracks": [{{ "name": "t", "steps": [
                    {{ "id": "c1", "control": "call", "routine": "{callee}", "args": {{}}, "sync": true }}
                  ]}}]
                }}"#
            ));
            let mut lib: HashMap<String, RoutineDef> = HashMap::new();
            lib.insert("tx1".into(), tx_routine("tx1", "{}"));
            lib.insert("tx2".into(), tx_routine("tx2", "{}"));
            let closure = {
                let lookup = |name: &str| lib.get(name).cloned();
                consent_closure(&parent, &lookup, &transmits_radio_connect)
            };
            closure_digest(&closure)
        };
        assert_ne!(build("tx1"), build("tx2"));
    }

    #[test]
    fn every_call_edge_to_a_relevant_callee_is_recorded_not_just_the_first() {
        // Routine A calls the SAME relevant callee B at two steps with
        // DIFFERENT args. Under the old first-edge-only walk (global-visited
        // returned false on the second visit), only the first edge was
        // recorded and re-arging the second call was invisible to the digest.
        // Both edges must now appear, and each edge's args must feed the hash.
        let build = |args1: &str, args2: &str| {
            let parent = def(&format!(
                r#"{{
                  "routine": "a", "schema_version": 1, "transmit_mode": "attended",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
                  "tracks": [{{ "name": "t", "steps": [
                    {{ "id": "c1", "control": "call", "routine": "b", "args": {args1}, "sync": true }},
                    {{ "id": "c2", "control": "call", "routine": "b", "args": {args2}, "sync": true }}
                  ]}}]
                }}"#
            ));
            let mut lib: HashMap<String, RoutineDef> = HashMap::new();
            lib.insert("b".into(), tx_routine("b", "{}"));
            let lookup = |name: &str| lib.get(name).cloned();
            consent_closure(&parent, &lookup, &transmits_radio_connect)
        };

        let base = build(r#"{"gain":1}"#, r#"{"gain":2}"#);
        // Both call edges to b are recorded, distinguished by their step ids.
        assert_eq!(base.call_edges.len(), 2, "both edges to b must be recorded");
        let steps: Vec<&str> = base.call_edges.iter().map(|e| e.step.0.as_str()).collect();
        assert!(steps.contains(&"c1") && steps.contains(&"c2"), "{steps:?}");
        // b's relevant step is enumerated exactly once (body walk is dedup'd).
        assert_eq!(base.steps.len(), 1, "b's step enumerated once");

        // Editing ONLY the second call's args flips the digest — proof the
        // second edge is in the hash, not silently dropped.
        let edited = build(r#"{"gain":1}"#, r#"{"gain":99}"#);
        assert_ne!(
            closure_digest(&base),
            closure_digest(&edited),
            "re-arging the second call edge must invalidate the digest"
        );
    }

    #[test]
    fn an_unrelated_edit_does_not_flip_the_digest() {
        // `track` is carried but excluded from the hash: renaming the track
        // (and touching an irrelevant sibling step) must not change the
        // digest.
        let base = consent_closure(
            &def(
                r#"{
                  "routine": "tx", "schema_version": 1, "transmit_mode": "attended",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
                  "tracks": [{ "name": "original", "steps": [
                    { "id": "s0", "action": "local.log", "params": {"note":"a"} },
                    { "id": "s1", "action": "radio.connect", "params": {"bands":["40m"]} }
                  ]}]
                }"#,
            ),
            &no_lookup,
            &transmits_radio_connect,
        );
        let edited = consent_closure(
            &def(
                r#"{
                  "routine": "tx", "schema_version": 1, "transmit_mode": "automatic",
                  "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
                  "tracks": [{ "name": "renamed-track", "steps": [
                    { "id": "s0", "action": "local.log", "params": {"note":"COMPLETELY DIFFERENT"} },
                    { "id": "s1", "action": "radio.connect", "params": {"bands":["40m"]} }
                  ]}]
                }"#,
            ),
            &no_lookup,
            &transmits_radio_connect,
        );
        assert_eq!(
            closure_digest(&base),
            closure_digest(&edited),
            "track name, transmit_mode, and irrelevant-step params are all outside the digest"
        );
    }
}
