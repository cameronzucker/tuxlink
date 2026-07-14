//! Trigger-cadence check (spec §5/§14 amendment, 2026-07-14 operator
//! decision "one cadence per routine"): a routine's `triggers` array is
//! manual plus AT MOST ONE `Trigger::Schedule`. Parallel lanes inside one
//! routine are same-cadence work by construction — every lane advances off
//! the same clock — so a routine that legitimately needs more than one
//! cadence is authored as multiple routines, composed via `Control::Call` or
//! simply left to coexist in the fleet (see `fleet.rs`'s
//! `SCHEDULE_COLLISION` / `SAME_EFFECT_OVERLAP`, the cross-routine checks
//! that replace what a second schedule on one routine used to paper over).

use crate::types::{RoutineDef, Trigger};

use super::findings::Finding;

pub const MULTIPLE_SCHEDULES: &str = "MULTIPLE_SCHEDULES";

/// Append a `MULTIPLE_SCHEDULES` warning for `def` into `findings` if it
/// declares more than one `Trigger::Schedule`. `Trigger::Manual` never
/// counts, and exactly one schedule (with or without `Trigger::Manual`
/// alongside it) is the supported shape and stays silent — only a SECOND
/// (or later) schedule trips this.
pub fn check(def: &RoutineDef, findings: &mut Vec<Finding>) {
    let schedule_count = def
        .triggers
        .iter()
        .filter(|t| matches!(t, Trigger::Schedule { .. }))
        .count();

    if schedule_count > 1 {
        findings.push(Finding::warning(
            MULTIPLE_SCHEDULES,
            def.routine.clone(),
            format!(
                "routine \"{}\" declares {schedule_count} schedule triggers, but a routine's \
                 triggers are manual plus at most one schedule — parallel lanes inside one \
                 routine share that one cadence by construction. Split multi-cadence work into \
                 separate routines composed via call, or left to coexist in the fleet, each on \
                 its own schedule.",
                def.routine
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IfMissed, OnInterrupted, RoutineDef, Track, TransmitMode};

    fn schedule(every: &str) -> Trigger {
        Trigger::Schedule {
            every: every.into(),
            align: Some("hour".into()),
            window: None,
            if_missed: IfMissed::Skip,
        }
    }

    fn routine_with_triggers(triggers: Vec<Trigger>) -> RoutineDef {
        RoutineDef {
            routine: "r1".into(),
            schema_version: crate::types::SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers,
            tracks: vec![Track {
                name: "t1".into(),
                steps: vec![],
            }],
        }
    }

    // --- positive: more than one schedule is flagged --------------------

    #[test]
    fn two_schedule_triggers_is_flagged() {
        let def = routine_with_triggers(vec![schedule("30m"), schedule("6h")]);
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, MULTIPLE_SCHEDULES);
        assert_eq!(findings[0].severity, super::super::Severity::Warning);
        assert!(findings[0].message.contains('2'), "{:?}", findings[0]);
        assert!(
            findings[0].message.to_lowercase().contains("cadence"),
            "message should state the one-cadence rule: {:?}",
            findings[0]
        );
        assert!(
            findings[0].message.to_lowercase().contains("split"),
            "message should suggest splitting: {:?}",
            findings[0]
        );
    }

    #[test]
    fn three_schedule_triggers_still_reports_the_actual_count() {
        let def = routine_with_triggers(vec![schedule("30m"), schedule("1h"), schedule("6h")]);
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains('3'), "{:?}", findings[0]);
    }

    // --- negative: manual, or a single schedule, is not flagged --------

    #[test]
    fn manual_only_is_not_flagged() {
        let def = routine_with_triggers(vec![Trigger::Manual]);
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn a_single_schedule_alone_is_not_flagged() {
        let def = routine_with_triggers(vec![schedule("30m")]);
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty());
    }

    #[test]
    fn a_single_schedule_plus_manual_is_not_flagged() {
        let def = routine_with_triggers(vec![Trigger::Manual, schedule("30m")]);
        let mut findings = Vec::new();
        check(&def, &mut findings);
        assert!(findings.is_empty());
    }
}
