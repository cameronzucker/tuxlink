//! Station-capability checks (spec §10 layer 1, plan-3 task 2): does the
//! step's declared action actually run on THIS station right now?
//!
//! Every check here derives from `ctx.action_descriptor(name).{needs_radio,
//! needs_internet}` — never from the action's name string. Name-sniffing
//! (`name.starts_with("radio.")`) would silently stop tracking new radio
//! actions the moment a descriptor's flags diverge from its name; the
//! descriptor is the single source of truth the executor itself uses
//! (`action.rs`), so the validator agrees with runtime by construction.
//!
//! An `ActionStep` whose action isn't in the registry (`action_descriptor`
//! returns `None`) is already reported once by `refs::check`'s
//! `UNKNOWN_ACTION` (task 2, same commit) — this module skips it outright
//! so an unknown action never also produces a capability finding, and never
//! counts toward `SAME_RIG_PARALLEL_LANES` track membership (we can't know
//! whether an unknown action needs a radio).

use crate::action::ActionDescriptor;
use crate::types::{RoutineDef, Step};

use super::context::{StationProfile, ValidationContext};
use super::findings::Finding;

pub const NEEDS_INTERNET_OFFGRID: &str = "NEEDS_INTERNET_OFFGRID";
pub const NO_RIG_CONFIGURED: &str = "NO_RIG_CONFIGURED";
pub const SAME_RIG_PARALLEL_LANES: &str = "SAME_RIG_PARALLEL_LANES";

/// Append every capability finding for `def` into `findings`. Called by
/// `validate()` (task 2 wiring) alongside `refs::check`.
pub fn check(def: &RoutineDef, ctx: &dyn ValidationContext, findings: &mut Vec<Finding>) {
    let profile = ctx.station_profile();
    let mut radio_track_names: Vec<String> = Vec::new();

    for track in &def.tracks {
        let mut track_needs_radio = false;

        for step in &track.steps {
            let Step::Action(action_step) = step else { continue };
            let Some(descriptor) = ctx.action_descriptor(&action_step.action) else {
                // UNKNOWN_ACTION already fired in refs::check; skip here so
                // it never double-fires a capability finding, and never
                // counts toward SAME_RIG_PARALLEL_LANES membership.
                continue;
            };

            if descriptor.needs_radio {
                track_needs_radio = true;
            }

            check_step_capability(def, &track.name, &action_step.id.0, descriptor, &profile, findings);
        }

        if track_needs_radio {
            radio_track_names.push(track.name.clone());
        }
    }

    if radio_track_names.len() >= 2 {
        findings.push(same_rig_parallel_lanes_finding(def, &radio_track_names));
    }
}

fn check_step_capability(
    def: &RoutineDef,
    track_name: &str,
    step_id: &str,
    descriptor: ActionDescriptor,
    profile: &StationProfile,
    findings: &mut Vec<Finding>,
) {
    if descriptor.needs_internet && !profile.has_internet {
        findings.push(
            Finding::warning(
                NEEDS_INTERNET_OFFGRID,
                def.routine.clone(),
                format!(
                    "step \"{step_id}\" runs action \"{}\", which needs internet, but this station has no internet configured",
                    descriptor.name
                ),
            )
            .with_track(track_name.to_string())
            .with_step(crate::types::StepId(step_id.to_string())),
        );
    }

    if descriptor.needs_radio && profile.rigs.is_empty() {
        findings.push(
            Finding::warning(
                NO_RIG_CONFIGURED,
                def.routine.clone(),
                format!(
                    "step \"{step_id}\" runs action \"{}\", which needs a radio, but no rig is configured for this station",
                    descriptor.name
                ),
            )
            .with_track(track_name.to_string())
            .with_step(crate::types::StepId(step_id.to_string())),
        );
    }
}

fn same_rig_parallel_lanes_finding(def: &RoutineDef, track_names: &[String]) -> Finding {
    let list = track_names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
    Finding::warning(
        SAME_RIG_PARALLEL_LANES,
        def.routine.clone(),
        format!(
            "tracks {list} each run a radio action; v1 has every radio action share the station's \
             single default rig, so these tracks will serialize instead of running in parallel"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ActionStep, BusyPolicy, OnInterrupted, RoutineDef, Step, StepId, Track, TransmitMode, Trigger,
    };
    use crate::validate::context::StaticContext;
    use crate::validate::findings::Severity;
    use serde_json::json;

    const RADIO_CONNECT: ActionDescriptor =
        ActionDescriptor { name: "radio.connect", needs_radio: true, transmits: true, needs_internet: false };
    const WEB_LOOKUP: ActionDescriptor =
        ActionDescriptor { name: "data.web_lookup", needs_radio: false, transmits: false, needs_internet: true };
    const LOCAL_NOTE: ActionDescriptor =
        ActionDescriptor { name: "local.note", needs_radio: false, transmits: false, needs_internet: false };

    fn action_step(id: &str, action: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: action.into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn routine(tracks: Vec<Track>) -> RoutineDef {
        RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks,
        }
    }

    #[test]
    fn needs_internet_action_offgrid_is_flagged() {
        let def = routine(vec![Track { name: "t1".into(), steps: vec![action_step("s1", "data.web_lookup")] }]);
        let ctx = StaticContext::new().with_action(WEB_LOOKUP); // has_internet defaults false
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, NEEDS_INTERNET_OFFGRID);
        assert_eq!(f.severity, Severity::Warning);
        assert_eq!(f.track, Some("t1".to_string()));
        assert_eq!(f.step, Some(StepId("s1".into())));
        assert!(f.message.contains("data.web_lookup"));
    }

    #[test]
    fn needs_internet_action_online_produces_no_finding() {
        let def = routine(vec![Track { name: "t1".into(), steps: vec![action_step("s1", "data.web_lookup")] }]);
        let ctx = StaticContext::new()
            .with_action(WEB_LOOKUP)
            .with_profile(StationProfile { has_internet: true, rigs: vec![] });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn needs_radio_action_with_no_rig_is_flagged() {
        let def = routine(vec![Track { name: "t1".into(), steps: vec![action_step("s1", "radio.connect")] }]);
        let ctx = StaticContext::new().with_action(RADIO_CONNECT); // rigs defaults empty
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, NO_RIG_CONFIGURED);
        assert_eq!(f.track, Some("t1".to_string()));
        assert_eq!(f.step, Some(StepId("s1".into())));
        assert!(f.message.contains("radio.connect"));
    }

    #[test]
    fn needs_radio_action_with_rig_configured_produces_no_finding() {
        let def = routine(vec![Track { name: "t1".into(), steps: vec![action_step("s1", "radio.connect")] }]);
        let ctx = StaticContext::new()
            .with_action(RADIO_CONNECT)
            .with_profile(StationProfile { has_internet: false, rigs: vec!["FT-710".into()] });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn local_action_never_flagged() {
        let def = routine(vec![Track { name: "t1".into(), steps: vec![action_step("s1", "local.note")] }]);
        let ctx = StaticContext::new().with_action(LOCAL_NOTE);
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn two_radio_tracks_trigger_same_rig_parallel_lanes() {
        let def = routine(vec![
            Track { name: "connect-cycle".into(), steps: vec![action_step("s1", "radio.connect")] },
            Track { name: "listen-cycle".into(), steps: vec![action_step("s2", "radio.connect")] },
        ]);
        let ctx = StaticContext::new()
            .with_action(RADIO_CONNECT)
            .with_profile(StationProfile { has_internet: false, rigs: vec!["FT-710".into()] });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        let parallel: Vec<_> = findings.iter().filter(|f| f.code == SAME_RIG_PARALLEL_LANES).collect();
        assert_eq!(parallel.len(), 1);
        assert_eq!(parallel[0].severity, Severity::Warning);
        assert!(parallel[0].message.contains("connect-cycle"));
        assert!(parallel[0].message.contains("listen-cycle"));
        assert!(parallel[0].message.to_lowercase().contains("rig"));
    }

    #[test]
    fn single_radio_track_does_not_trigger_same_rig_parallel_lanes() {
        let def = routine(vec![
            Track { name: "connect-cycle".into(), steps: vec![action_step("s1", "radio.connect")] },
            Track { name: "notes-cycle".into(), steps: vec![action_step("s2", "local.note")] },
        ]);
        let ctx = StaticContext::new()
            .with_action(RADIO_CONNECT)
            .with_action(LOCAL_NOTE)
            .with_profile(StationProfile { has_internet: false, rigs: vec!["FT-710".into()] });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);
        assert!(findings.iter().all(|f| f.code != SAME_RIG_PARALLEL_LANES));
    }

    #[test]
    fn unknown_action_step_is_skipped_by_capability_checks_entirely() {
        // refs::check would fire UNKNOWN_ACTION for this step (task-2, separate
        // module); capability::check must not ALSO fire for it, and must not
        // count it toward SAME_RIG_PARALLEL_LANES track membership.
        let def = routine(vec![
            Track { name: "t1".into(), steps: vec![action_step("s1", "radio.mystery")] },
            Track { name: "t2".into(), steps: vec![action_step("s2", "radio.connect")] },
        ]);
        let ctx = StaticContext::new()
            .with_action(RADIO_CONNECT) // "radio.mystery" NOT seeded
            .with_profile(StationProfile { has_internet: false, rigs: vec!["FT-710".into()] });
        let mut findings = Vec::new();
        check(&def, &ctx, &mut findings);

        // Only t2 truly has a known needs_radio action, so no parallel-lanes
        // warning (only one real radio track), and nothing at all fires for
        // the unknown-action step in t1 (no capability finding for it, and
        // it does not count toward SAME_RIG_PARALLEL_LANES membership).
        assert!(findings.is_empty(), "expected no capability findings, got {findings:?}");
    }
}
