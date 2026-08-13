//! Artifact-derived readback renderers — slice (b) substrate of the mutation
//! contract (tuxlink-fb0hc), wording UNDER EVALUATION (tuxlink-k2h9l).
//!
//! A readback is the mechanical answer to "what did the app actually build
//! when I saved?": rendered from the STORED [`RoutineDef`], never from a
//! model's account of it, and returned beside the revision digest. Its one
//! job is making divergence between intent and artifact VISIBLE — so the
//! candidate styles here are being chosen by measured divergence-detection
//! rate (the eval harness in `examples/readback_eval_gen.rs` +
//! `dev/readback-eval/`), not by taste. Do not wire any of these into
//! `routines_save` until the operator pins the winning style.
//!
//! Three candidates:
//! - [`narrative`] — style A: full sentences, one flowing paragraph.
//! - [`scannable`] — style B: labeled `·`-separated summary lines.
//! - [`diff`] — style C: edit-anchored "what changed" lines between two defs.
//!
//! All three are TOTAL over valid defs: an action the renderer has no special
//! phrasing for renders generically (`run data.stationlist_update`), never an
//! error — a readback that can refuse is a readback that can hide.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::types::{
    Control, ControlStep, RoutineDef, Step, TransmitMode, Trigger,
};

/// Context the renderer cannot get from the def alone: display labels for
/// snapshot references (`@draft:<slot-id>` → the operator's label). Slice (b)
/// builds this from the draft library; absence renders the raw reference,
/// which is still honest — just less friendly.
#[derive(Debug, Clone, Default)]
pub struct DisplayNames {
    pub draft_labels: BTreeMap<String, String>,
}

impl DisplayNames {
    fn draft(&self, slot_id: &str) -> String {
        match self.draft_labels.get(slot_id) {
            Some(label) => format!("your saved draft '{label}'"),
            None => format!("saved draft {slot_id}"),
        }
    }
}

fn is_draft_ref(v: &Value) -> Option<&str> {
    v.as_str().and_then(|s| s.strip_prefix("@draft:"))
}

/// Human phrase for one trigger.
fn trigger_phrase(t: &Trigger) -> String {
    match t {
        Trigger::Manual => "runs only when you start it".to_string(),
        Trigger::Schedule {
            every,
            align,
            window,
            if_missed,
        } => {
            let mut s = format!("runs every {every}");
            if let Some(a) = align {
                s.push_str(&format!(" (aligned to the {a})"));
            }
            if let Some(w) = window {
                s.push_str(&format!(" between {}", w.replace('-', " and ")));
            }
            match if_missed {
                crate::types::IfMissed::Skip => s.push_str("; missed runs are skipped"),
                crate::types::IfMissed::RunOnceOnLaunch => {
                    s.push_str("; a missed run fires once at launch")
                }
            }
            s
        }
    }
}

/// The consent posture line. Load-bearing: this is the sentence an operator
/// most needs to be unable to miss.
fn consent_phrase(def: &RoutineDef) -> String {
    match def.transmit_mode {
        TransmitMode::Attended => {
            "waits for your transmit consent before any send".to_string()
        }
        TransmitMode::Automatic => match &def.transmit_ack {
            Some(ack) => format!(
                "TRANSMITS AUTOMATICALLY without asking (acknowledged by {})",
                ack.by
            ),
            None => "set to TRANSMIT AUTOMATICALLY — not yet acknowledged, so it \
                     will not run until you sign it"
                .to_string(),
        },
    }
}

/// Compact human phrase for one param value.
fn value_phrase(v: &Value, names: &DisplayNames) -> String {
    if let Some(slot) = is_draft_ref(v) {
        return names.draft(slot);
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|i| value_phrase(i, names))
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// Action-aware phrasing for the steps that carry operator-meaningful
/// payloads; a generic-but-complete fallback for everything else.
fn action_phrase(action: &str, params: &Value, names: &DisplayNames) -> String {
    let p = |k: &str| params.get(k);
    match action {
        "local.compose" => {
            let to = p("to")
                .map(|v| value_phrase(v, names))
                .unwrap_or_else(|| "(no recipient)".into());
            let what = if let Some(t) = p("template") {
                if let Some(slot) = is_draft_ref(t) {
                    format!("fill a form from {}", names.draft(slot))
                } else if let Some(id) = t.get("id").and_then(Value::as_str) {
                    format!("fill the {id} form")
                } else {
                    "fill a form".to_string()
                }
            } else if let Some(s) = p("subject").and_then(Value::as_str) {
                format!("compose a message '{s}'")
            } else {
                "compose a message".to_string()
            };
            let from = p("from_identity")
                .and_then(|f| f.get("callsign"))
                .and_then(Value::as_str)
                .map(|c| format!(" as {c}"))
                .unwrap_or_default();
            format!("{what}, address it to {to}{from}, and stage it in the Outbox")
        }
        "radio.connect" => {
            let stations = p("stations")
                .map(|v| value_phrase(v, names))
                .unwrap_or_else(|| "the configured station".into());
            let bands = p("bands")
                .map(|v| format!(" on {}", value_phrase(v, names)))
                .unwrap_or_default();
            format!("connect to {stations}{bands} and exchange mail")
        }
        "radio.listen" => "listen on the radio".to_string(),
        "radio.aprs_send" => {
            let dest = p("to")
                .map(|v| format!(" to {}", value_phrase(v, names)))
                .unwrap_or_default();
            format!("send an APRS packet{dest}")
        }
        "local.notify" => {
            let msg = p("message")
                .or_else(|| p("body"))
                .and_then(Value::as_str)
                .map(|m| format!(" '{m}'"))
                .unwrap_or_default();
            format!("show you a notification{msg}")
        }
        "local.log" => "write a log line".to_string(),
        "local.set_identity" => {
            let c = p("callsign")
                .and_then(Value::as_str)
                .unwrap_or("(unspecified)");
            format!("switch the sending identity to {c}")
        }
        "data.spacewx_wwv" => "capture WWV space-weather".to_string(),
        "data.spacewx_swpc" => "fetch SWPC space-weather".to_string(),
        "data.stationlist_update" => "refresh the station list".to_string(),
        other => {
            // Generic fallback: name the action and every param, so nothing
            // the operator set can hide behind an unphrased action.
            let mut s = format!("run {other}");
            if let Some(obj) = params.as_object() {
                if !obj.is_empty() {
                    let kv = obj
                        .iter()
                        .map(|(k, v)| format!("{k}={}", value_phrase(v, names)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    s.push_str(&format!(" with {kv}"));
                }
            }
            s
        }
    }
}

fn control_phrase(c: &ControlStep, names: &DisplayNames) -> String {
    match &c.control {
        Control::Branch {
            on,
            op,
            value,
            then,
            r#else,
        } => {
            let cond = match (op, value) {
                (Some(op), Some(v)) => {
                    let sym = match op {
                        crate::types::CmpOp::Eq => "=",
                        crate::types::CmpOp::Ne => "≠",
                        crate::types::CmpOp::Lt => "<",
                        crate::types::CmpOp::Lte => "≤",
                        crate::types::CmpOp::Gt => ">",
                        crate::types::CmpOp::Gte => "≥",
                    };
                    format!("{on} {sym} {}", value_phrase(v, names))
                }
                _ => on.clone(),
            };
            let then_ids = then.iter().map(|s| s.0.as_str()).collect::<Vec<_>>().join(", ");
            let else_ids = r#else
                .iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if else_ids.is_empty() {
                format!("if {cond}: continue with {then_ids}")
            } else {
                format!("if {cond}: continue with {then_ids}; otherwise {else_ids}")
            }
        }
        Control::Delay { delay } => format!("wait {delay}"),
        Control::Retry {
            step,
            attempts,
            backoff_s,
        } => {
            if *backoff_s > 0 {
                format!("retry {} up to {attempts} times ({backoff_s}s backoff)", step.0)
            } else {
                format!("retry {} up to {attempts} times", step.0)
            }
        }
        Control::Call { routine, sync, .. } => {
            if *sync {
                format!("run the routine '{routine}' and wait for it")
            } else {
                format!("start the routine '{routine}' in the background")
            }
        }
        Control::End { failed, reason } => {
            let r = reason
                .as_ref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            if *failed {
                format!("stop and mark the run FAILED{r}")
            } else {
                format!("stop{r}")
            }
        }
    }
}

fn step_phrase(step: &Step, names: &DisplayNames) -> String {
    match step {
        Step::Action(a) => {
            let mut s = format!("{} — {}", a.id.0, action_phrase(&a.action, &a.params, names));
            if let Some(t) = a.timeout_s {
                s.push_str(&format!(" (give up after {t}s)"));
            }
            s
        }
        Step::Control(c) => format!("{} — {}", c.id.0, control_phrase(c, names)),
    }
}

/// Style A: one flowing narrative paragraph.
pub fn narrative(def: &RoutineDef, names: &DisplayNames) -> String {
    let triggers = def
        .triggers
        .iter()
        .map(trigger_phrase)
        .collect::<Vec<_>>()
        .join("; also ");
    let mut out = format!("'{}' {}.", def.routine, triggers);

    for track in &def.tracks {
        let steps = track
            .steps
            .iter()
            .map(|s| step_phrase(s, names))
            .collect::<Vec<_>>()
            .join("; then ");
        if def.tracks.len() > 1 {
            out.push_str(&format!(" On its '{}' track: {steps}.", track.name));
        } else {
            out.push_str(&format!(" Each run: {steps}."));
        }
    }

    out.push_str(&format!(" It {}.", consent_phrase(def)));
    out
}

/// Style B: labeled scannable lines.
pub fn scannable(def: &RoutineDef, names: &DisplayNames) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Routine: {}", def.routine));
    lines.push(format!(
        "Runs: {}",
        def.triggers
            .iter()
            .map(trigger_phrase)
            .collect::<Vec<_>>()
            .join(" · ")
    ));
    for track in &def.tracks {
        let label = if def.tracks.len() > 1 {
            format!("Steps ({})", track.name)
        } else {
            "Steps".to_string()
        };
        lines.push(format!(
            "{label}: {}",
            track
                .steps
                .iter()
                .map(|s| step_phrase(s, names))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    lines.push(format!("Consent: {}", consent_phrase(def)));
    lines.join("\n")
}

/// Style C: what changed between two stored defs, edit-anchored. Renders
/// "No changes." for identical defs — an honest answer that lets a reader
/// catch "I asked for a change and nothing changed".
pub fn diff(prev: &RoutineDef, next: &RoutineDef, names: &DisplayNames) -> String {
    let mut changes: Vec<String> = Vec::new();

    if prev.routine != next.routine {
        changes.push(format!("renamed '{}' → '{}'", prev.routine, next.routine));
    }
    if prev.transmit_mode != next.transmit_mode {
        changes.push(format!(
            "consent posture: now it {}",
            consent_phrase(next)
        ));
    }
    let pt = prev.triggers.iter().map(trigger_phrase).collect::<Vec<_>>();
    let nt = next.triggers.iter().map(trigger_phrase).collect::<Vec<_>>();
    if pt != nt {
        changes.push(format!(
            "schedule: was '{}', now '{}'",
            pt.join("; "),
            nt.join("; ")
        ));
    }

    // Steps by id across all tracks: added / removed / changed.
    let index = |d: &RoutineDef| -> BTreeMap<String, String> {
        d.tracks
            .iter()
            .flat_map(|t| t.steps.iter())
            .map(|s| (s.id().0.clone(), step_phrase(s, names)))
            .collect()
    };
    let pi = index(prev);
    let ni = index(next);
    for (id, phrase) in &ni {
        match pi.get(id) {
            None => changes.push(format!("added step {phrase}")),
            Some(old) if old != phrase => {
                changes.push(format!("step {id}: was '{old}', now '{phrase}'"))
            }
            _ => {}
        }
    }
    for (id, phrase) in &pi {
        if !ni.contains_key(id) {
            changes.push(format!("removed step {id} ({phrase})"));
        }
    }

    if changes.is_empty() {
        "No changes.".to_string()
    } else {
        let unchanged_consent = prev.transmit_mode == next.transmit_mode;
        let mut out = format!("Changed: {}.", changes.join("; "));
        if unchanged_consent {
            out.push_str(&format!(" Everything else unchanged — it still {}.", consent_phrase(next)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkin_def() -> RoutineDef {
        RoutineDef::parse(
            r#"{
              "routine": "morning-check-in",
              "schema_version": 1,
              "transmit_mode": "attended",
              "triggers": [
                { "type": "schedule", "every": "30m", "window": "07:00-08:00", "if_missed": "skip" }
              ],
              "tracks": [
                { "name": "main", "steps": [
                  { "id": "s1", "action": "radio.connect",
                    "params": { "stations": "@station-set:or-gateways", "bands": ["40m"] },
                    "timeout_s": 300 },
                  { "id": "s2", "control": "branch", "on": "s1.connected",
                    "then": ["s3"], "else": ["s4"] },
                  { "id": "s3", "action": "local.compose",
                    "params": { "to": ["W7XYZ-10"], "template": "@draft:d-morning" } },
                  { "id": "s4", "action": "local.notify",
                    "params": { "message": "no gateway reached" } }
                ] }
              ]
            }"#,
        )
        .unwrap()
    }

    fn names() -> DisplayNames {
        DisplayNames {
            draft_labels: BTreeMap::from([(
                "d-morning".to_string(),
                "Morning check-in".to_string(),
            )]),
        }
    }

    #[test]
    fn narrative_names_every_operator_meaningful_fact() {
        let text = narrative(&checkin_def(), &names());
        for needle in [
            "morning-check-in",
            "every 30m",
            "07:00 and 08:00",
            "W7XYZ-10",
            "Morning check-in",
            "transmit consent",
            "no gateway reached",
        ] {
            assert!(text.contains(needle), "narrative missing '{needle}': {text}");
        }
    }

    #[test]
    fn scannable_carries_the_same_facts_in_labeled_lines() {
        let text = scannable(&checkin_def(), &names());
        assert!(text.starts_with("Routine: morning-check-in"));
        for needle in ["Runs:", "Steps:", "Consent:", "W7XYZ-10", "Morning check-in"] {
            assert!(text.contains(needle), "scannable missing '{needle}': {text}");
        }
    }

    #[test]
    fn automatic_without_ack_is_unmissable() {
        let mut def = checkin_def();
        def.transmit_mode = TransmitMode::Automatic;
        let text = narrative(&def, &names());
        assert!(text.contains("TRANSMIT AUTOMATICALLY"), "{text}");
        assert!(text.contains("not yet acknowledged"), "{text}");
    }

    #[test]
    fn unknown_actions_render_generically_with_their_params() {
        let mut def = checkin_def();
        if let Step::Action(a) = &mut def.tracks[0].steps[0] {
            a.action = "future.mystery".into();
            a.params = serde_json::json!({"knob": 7});
        }
        let text = narrative(&def, &names());
        assert!(text.contains("run future.mystery"), "{text}");
        assert!(text.contains("knob=7"), "{text}");
    }

    #[test]
    fn diff_reports_a_recipient_change_and_the_standing_consent() {
        let prev = checkin_def();
        let mut next = prev.clone();
        if let Step::Action(a) = &mut next.tracks[0].steps[2] {
            a.params["to"] = serde_json::json!(["W7ABC"]);
        }
        let text = diff(&prev, &next, &names());
        assert!(text.contains("s3"), "{text}");
        assert!(text.contains("W7XYZ-10"), "diff must show the old value: {text}");
        assert!(text.contains("W7ABC"), "diff must show the new value: {text}");
        assert!(text.contains("still waits for your transmit consent"), "{text}");
    }

    #[test]
    fn diff_of_identical_defs_is_an_honest_no_changes() {
        let def = checkin_def();
        assert_eq!(diff(&def, &def.clone(), &names()), "No changes.");
    }

    #[test]
    fn diff_reports_added_and_removed_steps() {
        let prev = checkin_def();
        let mut next = prev.clone();
        next.tracks[0].steps.remove(3);
        let text = diff(&prev, &next, &names());
        assert!(text.contains("removed step s4"), "{text}");
        assert!(text.contains("notification"), "{text}");
    }
}
