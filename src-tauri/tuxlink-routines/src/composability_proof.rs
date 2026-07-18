//! Composability proof (tuxlink-iizmk item 12, operator challenge 2026-07-18):
//! executable evidence for which multi-step routines the engine can run TODAY
//! — real `Engine` + executor, `FakeAction` outcomes standing in for hardware
//! (the same seam production actions plug into), `$`-references and branch
//! decisions asserted from what each action actually RECEIVED.
//!
//! Also the negative half: the two routine classes the operator would value
//! most are demonstrated to be REJECTED by today's engine — nested output
//! paths (`$s1.indices.k_index`) and numeric branch conditions — pinning the
//! missing links the round-2 build must close rather than hand-waving them.

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
            resolver: Arc::new(FakeResolver::new()),
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

    /// R2 (negative proof) — "Propagation-gated band plan" is the routine an
    /// operator would most want: fetch space weather, branch on the K-index,
    /// pick 80 m vs 40 m. TODAY'S ENGINE REJECTS BOTH HALVES, and these tests
    /// pin the exact failures (the round-2 missing links):
    ///  - `$s1.indices.k_index` — nested output fields are unreachable
    ///    (VarPath is step + ONE flat key).
    ///  - branch on a NUMBER — conditions are strictly boolean; there is no
    ///    comparison form, so a threshold gate cannot be expressed.
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
    async fn r2_nested_output_path_is_unreachable_today() {
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

        // The data IS there (swpc ran) but the deep path cannot be resolved:
        // the run fails and the log step never receives anything.
        assert_eq!(swpc.calls().len(), 1);
        assert_eq!(log.calls().len(), 0);
        assert_eq!(
            outcome.state,
            RunState::Failed,
            "nested $-paths are missing-link #1: this SHOULD work and does not"
        );
    }

    const R2_NUMERIC_BRANCH: &str = r#"{
      "routine": "propagation-gate-branch", "schema_version": 1,
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
    async fn r2_branching_on_a_number_is_a_hard_error_today() {
        let dir = tempfile::tempdir().unwrap();
        // Even with the index FLATTENED into the output, branch rejects it:
        // conditions are strictly boolean.
        let swpc = Arc::new(FakeAction::new("data.spacewx_flat").ok(json!({"k_index": 5.0})));
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));

        let eng = engine_with(vec![swpc.clone(), log.clone()], dir.path());
        let def = RoutineDef::parse(R2_NUMERIC_BRANCH).unwrap();
        let outcome = eng
            .start_run(&def, json!({}))
            .await
            .unwrap()
            .done
            .await
            .unwrap();

        assert_eq!(swpc.calls().len(), 1);
        assert_eq!(log.calls().len(), 0, "neither branch arm can run");
        assert_eq!(
            outcome.state,
            RunState::Failed,
            "numeric branch conditions are missing-link #2: a threshold gate cannot be expressed"
        );
    }
}
