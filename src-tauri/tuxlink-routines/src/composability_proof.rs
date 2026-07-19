//! Composability proof (tuxlink-iizmk item 12, operator challenge 2026-07-18):
//! executable evidence for which multi-step routines the engine can run TODAY
//! — real `Engine` + executor, `FakeAction` outcomes standing in for hardware
//! (the same seam production actions plug into), `$`-references and branch
//! decisions asserted from what each action actually RECEIVED.
//!
//! The R2 section documents the two links round 2 closed: nested output
//! paths (`$s1.indices.k_index`) and the branch comparison form
//! (`op`/`value`). Both were pinned here as NEGATIVE proofs when the file
//! landed; the tests now assert the positive behavior, and one deliberate
//! negative remains (a bare numeric branch without a comparison stays a
//! hard error; truthiness guessing is banned).

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::action::ActionRegistry;
    use crate::engine::{Engine, EngineConfig};
    use crate::fakes::{FakeAction, FakeResolver};
    use crate::journal::RunState;
    use crate::types::RoutineDef;

    fn fixed_now() -> i64 {
        1_760_000_000
    }

    fn engine_with(actions: Vec<Arc<FakeAction>>, dir: &std::path::Path) -> Arc<Engine> {
        let mut reg = ActionRegistry::default();
        for a in actions {
            reg.register(a);
        }
        Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new().entity(
                "preset",
                "40m-vara",
                json!({"frequencyHz": 7_103_500, "mode": "vara-hf"}),
            )),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: None,
            consent: None,
        }))
    }

    /// R1 — "Drift-checked clear-channel mail run". The attended morning
    /// ritual as one routine: validate the rig against the 40 m preset;
    /// branch — apply the preset only when drifted; listen before transmit;
    /// branch — bail politely when the channel is busy; connect; report the
    /// gateway actually reached through compose's vars (the sanctioned way
    /// outputs land in message text).
    ///
    /// Composability under proof: `s1.matches` (bool) gates apply;
    /// `s4.channel_busy` (bool) gates the dial; `$s6.gateway` flows into
    /// compose's `vars` object.
    const R1: &str = r#"{
      "routine": "morning-mail-run", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "rig.validate_preset",
          "params": {"preset": "@preset:40m-vara"} },
        { "id": "s2", "control": "branch", "on": "s1.matches",
          "then": ["s4"], "else": ["s3"] },
        { "id": "s3", "action": "rig.apply_preset",
          "params": {"preset": "@preset:40m-vara"} },
        { "id": "s4", "action": "radio.listen",
          "params": {"seconds": 5} },
        { "id": "s5", "control": "branch", "on": "s4.channel_busy",
          "then": ["s9"], "else": ["s6"] },
        { "id": "s6", "action": "radio.connect",
          "params": {"stations": ["N0DAJ"], "bands": ["40m"]} },
        { "id": "s7", "control": "branch", "on": "s6.connected",
          "then": ["s8"], "else": ["s9"] },
        { "id": "s8", "action": "local.compose",
          "params": {"to": ["W7AUX"], "subject": "morning check",
                     "template": "morning-report",
                     "vars": {"gateway": "$s6.gateway", "band": "$s6.band"}} },
        { "id": "end_ok", "control": "end", "failed": false },
        { "id": "s9", "action": "local.log",
          "params": {"message": "morning run aborted: busy channel or failed dial"} },
        { "id": "end_bail", "control": "end", "failed": false }
      ]}]
    }"#;

    #[tokio::test]
    async fn r1_drifted_rig_gets_preset_applied_and_gateway_reaches_compose_vars() {
        let dir = tempfile::tempdir().unwrap();
        let validate = Arc::new(
            FakeAction::new("rig.validate_preset")
                .ok(json!({"matches": false, "diff": {"freq_hz": 7_101_000}})),
        );
        let apply = Arc::new(FakeAction::new("rig.apply_preset").ok(json!({"applied": true})));
        let listen = Arc::new(
            FakeAction::new("radio.listen").ok(json!({"channel_busy": false, "rms": 0.02})),
        );
        let connect = Arc::new(FakeAction::new("radio.connect").ok(
            json!({"connected": true, "station": "N0DAJ", "band": "40m", "gateway": "N0DAJ-10"}),
        ));
        let compose = Arc::new(
            FakeAction::new("local.compose").ok(json!({"staged": true, "mid": "m-1"})),
        );
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(
            vec![
                validate.clone(),
                apply.clone(),
                listen.clone(),
                connect.clone(),
                compose.clone(),
                log.clone(),
            ],
            dir.path(),
        );
        let def = RoutineDef::parse(R1).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        // Drifted rig → the else-arm ran apply exactly once.
        assert_eq!(apply.calls().len(), 1, "drift branch must apply the preset");
        // Clear channel → the dial happened; busy-bail log never ran.
        assert_eq!(connect.calls().len(), 1);
        assert_eq!(log.calls().len(), 0, "no bail on a clear channel");
        // THE composability claim: compose received the RESOLVED gateway and
        // band from s6's output — not the literal "$s6.gateway" text.
        let sent = &compose.calls()[0];
        assert_eq!(sent["vars"]["gateway"], json!("N0DAJ-10"));
        assert_eq!(sent["vars"]["band"], json!("40m"));
    }

    #[tokio::test]
    async fn r1_busy_channel_bails_without_dialing() {
        let dir = tempfile::tempdir().unwrap();
        let validate =
            Arc::new(FakeAction::new("rig.validate_preset").ok(json!({"matches": true})));
        let apply = Arc::new(FakeAction::new("rig.apply_preset").ok(json!({"applied": true})));
        let listen = Arc::new(
            FakeAction::new("radio.listen").ok(json!({"channel_busy": true, "rms": 0.4})),
        );
        let connect = Arc::new(FakeAction::new("radio.connect").ok(json!({"connected": true})));
        let compose = Arc::new(FakeAction::new("local.compose").ok(json!({"staged": true})));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(
            vec![
                validate.clone(),
                apply.clone(),
                listen.clone(),
                connect.clone(),
                compose.clone(),
                log.clone(),
            ],
            dir.path(),
        );
        let def = RoutineDef::parse(R1).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        // Matching rig → apply skipped; busy channel → NO dial, the bail log ran.
        assert_eq!(apply.calls().len(), 0, "matching rig skips apply");
        assert_eq!(connect.calls().len(), 0, "busy channel must never dial");
        assert_eq!(log.calls().len(), 1);
    }

    /// R3 — "Gateway-continuity sweep": read the last connected gateway from
    /// app data and dial IT (an output feeding another action's INPUT array),
    /// then log the resolved target. Proves `$`-refs resolve inside arrays.
    const R3: &str = r#"{
      "routine": "gateway-continuity", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "data.read",
          "params": {"source": "last_connected_gateway"} },
        { "id": "s2", "action": "radio.connect",
          "params": {"stations": ["$s1.gateway"], "bands": ["40m"]} },
        { "id": "s3", "action": "local.log",
          "params": {"message": "$s1.gateway"} }
      ]}]
    }"#;

    #[tokio::test]
    async fn r3_last_gateway_output_feeds_the_next_dials_station_list() {
        let dir = tempfile::tempdir().unwrap();
        let read = Arc::new(FakeAction::new("data.read").ok(json!({"gateway": "W7DEF-10"})));
        let connect = Arc::new(FakeAction::new("radio.connect").ok(json!({"connected": true})));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![read.clone(), connect.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R3).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        // The dial received the READ gateway inside its stations array.
        assert_eq!(connect.calls()[0]["stations"], json!(["W7DEF-10"]));
        // A whole-string $-ref param resolves for log too.
        assert_eq!(log.calls()[0]["message"], json!("W7DEF-10"));
    }

    /// R4 — "Find-and-connect" (compat-tree rank 2, `data.find_stations`): a
    /// station query whose DEDUPED, distance-sorted callsign ARRAY feeds
    /// `radio.connect`'s `stations` list via a WHOLE-STRING `$s1.callsigns`
    /// ref. Proves the marquee find_stations → connect composition: a
    /// whole-string ref resolves to the underlying JSON ARRAY (not just a
    /// scalar), and the SAME array flows into a second consumer (`local.log`)
    /// unchanged.
    const R4_FIND_STATIONS: &str = r#"{
      "routine": "find-and-connect", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "data.find_stations",
          "params": {"modes": ["vara-hf"], "limit": 3} },
        { "id": "s2", "action": "radio.connect",
          "params": {"stations": "$s1.callsigns", "bands": ["40m"]} },
        { "id": "s3", "action": "local.log",
          "params": {"message": "$s1.callsigns"} }
      ]}]
    }"#;

    #[tokio::test]
    async fn r4_find_stations_callsigns_feed_connect_station_list() {
        let dir = tempfile::tempdir().unwrap();
        // The faked directory returns the exact `data.find_stations` output
        // shape: a distance-sorted, deduped callsign array plus the curated
        // gateway rows / provenance the real action emits.
        let find = Arc::new(FakeAction::new("data.find_stations").ok(json!({
            "gateways": [],
            "fetched_at_ms": null,
            "operator_grid": null,
            "callsigns": ["W7AAA", "K7BBB", "N7CCC"]
        })));
        let connect = Arc::new(FakeAction::new("radio.connect").ok(json!({"connected": true})));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![find.clone(), connect.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R4_FIND_STATIONS).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        // THE composability claim: the whole-string `$s1.callsigns` resolved to
        // the sorted callsign ARRAY inside connect's `stations` param — not the
        // literal token, not a scalar.
        assert_eq!(
            connect.calls()[0]["stations"],
            json!(["W7AAA", "K7BBB", "N7CCC"])
        );
        // The same array flows into a second consumer unchanged.
        assert_eq!(log.calls()[0]["message"], json!(["W7AAA", "K7BBB", "N7CCC"]));
    }

    /// R2 — "Propagation-gated band plan": fetch space weather, gate on the
    /// K-index, pick the band. Both halves were NEGATIVE proofs when this file
    /// landed (nested output paths unreachable, numeric branch a hard error);
    /// round 2 closed both links and these tests now assert the positive
    /// behavior, plus one retained negative: a bare numeric branch with NO
    /// comparison stays a hard error (guessing truthiness is banned).
    const R2_NESTED: &str = r#"{
      "routine": "propagation-gate-nested", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "data.spacewx_swpc", "params": {} },
        { "id": "s2", "action": "local.log",
          "params": {"message": "$s1.indices.k_index"} }
      ]}]
    }"#;

    #[tokio::test]
    async fn r2_nested_output_path_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let swpc = Arc::new(FakeAction::new("data.spacewx_swpc").ok(
            json!({"forecast_updated": true, "indices": {"sfi": 145.0, "k_index": 5.0}}),
        ));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![swpc.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R2_NESTED).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();

        // Round-2 link #1 closed: the deep path resolves and the log step
        // receives the RESOLVED number, not the token and not a failure.
        assert_eq!(swpc.calls().len(), 1);
        assert_eq!(log.calls()[0]["message"], json!(5.0));
        assert_eq!(outcome.state, RunState::Completed);
    }

    /// The comparison form: `k_index >= 4` picks the disturbed-band plan.
    /// Nested `on` path + `op`/`value` exercise BOTH closed links in one
    /// routine, and the journal is asserted for the decree events
    /// (`branch_taken`, `step_skipped`) so the decision and the not-taken
    /// remainder are durably explained.
    const R2_CMP_BRANCH: &str = r#"{
      "routine": "propagation-gate-cmp", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "data.spacewx_swpc", "params": {} },
        { "id": "s2", "control": "branch", "on": "s1.indices.k_index",
          "op": "gte", "value": 4, "then": ["s3"], "else": ["s4"] },
        { "id": "s3", "action": "local.log", "params": {"message": "disturbed"} },
        { "id": "end1", "control": "end", "failed": false },
        { "id": "s4", "action": "local.log", "params": {"message": "quiet"} }
      ]}]
    }"#;

    #[tokio::test]
    async fn r2_numeric_branch_comparison_gates_the_band_plan() {
        let dir = tempfile::tempdir().unwrap();
        let swpc = Arc::new(FakeAction::new("data.spacewx_swpc").ok(
            json!({"forecast_updated": true, "indices": {"sfi": 145.0, "k_index": 5.0}}),
        ));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![swpc.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R2_CMP_BRANCH).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let run_id = handle.run_id.clone();
        let outcome = handle.done.await.unwrap();

        // Round-2 link #2 closed: 5.0 >= 4 takes the then arm; end1 stops the
        // run before the quiet arm.
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(log.calls().len(), 1);
        assert_eq!(log.calls()[0]["message"], json!("disturbed"));

        // Decree events: the decision and the never-run remainder are durable.
        let entries =
            crate::journal::read_journal(&dir.path().join(format!("{run_id}.jsonl"))).unwrap();
        let branch = entries
            .iter()
            .find_map(|e| match &e.event {
                crate::journal::RunEvent::BranchTaken {
                    step,
                    value,
                    took_then,
                    target,
                    ..
                } if step.0 == "s2" => Some((value.clone(), *took_then, target.clone())),
                _ => None,
            })
            .expect("branch decision must be journaled");
        assert_eq!(branch.0, json!(5.0));
        assert!(branch.1, "5.0 >= 4 takes the then arm");
        assert_eq!(branch.2.unwrap().0, "s3");
        let skipped: Vec<&str> = entries
            .iter()
            .filter_map(|e| match &e.event {
                crate::journal::RunEvent::StepSkipped { step, .. } => Some(step.0.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(skipped, vec!["s4"], "the quiet arm is recorded as skipped");
    }

    /// Retained negative: a bare numeric branch with NO comparison is still a
    /// hard, verbatim error. Truthiness guessing stays banned; the fix for a
    /// number is the comparison form, not coercion.
    const R2_BARE_NUMERIC_BRANCH: &str = r#"{
      "routine": "propagation-gate-bare", "schema_version": 1,
      "transmit_mode": "attended", "on_interrupted": "stay",
      "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "data.spacewx_flat", "params": {} },
        { "id": "s2", "control": "branch", "on": "s1.k_index",
          "then": ["s3"], "else": ["s4"] },
        { "id": "s3", "action": "local.log", "params": {"message": "disturbed"} },
        { "id": "end1", "control": "end", "failed": false },
        { "id": "s4", "action": "local.log", "params": {"message": "quiet"} }
      ]}]
    }"#;

    #[tokio::test]
    async fn r2_bare_numeric_branch_remains_a_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let swpc = Arc::new(FakeAction::new("data.spacewx_flat").ok(json!({"k_index": 5.0})));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![swpc.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R2_BARE_NUMERIC_BRANCH).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();

        assert_eq!(swpc.calls().len(), 1);
        assert_eq!(log.calls().len(), 0, "neither branch arm can run");
        assert_eq!(outcome.state, RunState::Failed);
    }
}
