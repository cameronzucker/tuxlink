//! Readback-style eval case generator (tuxlink-k2h9l).
//!
//! Emits JSONL to stdout: one case per (routine, mutation, style). Each case
//! pairs the operator's REQUEST (faithful prose of the true intent) with the
//! READBACK rendered from a build that may carry an injected divergence. The
//! judge driver (`dev/readback-eval/judge.py`) asks reader models whether the
//! readback matches the request; per-style divergence-DETECTION rates decide
//! the slice (b) wording (measured, not tasted — operator direction
//! 2026-08-13).
//!
//! Deterministic by construction: fixed corpus, fixed mutation tables, no
//! randomness — a re-run emits byte-identical cases.

use std::collections::BTreeMap;

use serde_json::json;
use tuxlink_routines::readback::{diff, narrative, scannable, DisplayNames};
use tuxlink_routines::types::{Control, RoutineDef, Step, TransmitMode, Trigger};

struct CorpusEntry {
    key: &'static str,
    request: &'static str,
    def: RoutineDef,
}

fn parse(json: &str) -> RoutineDef {
    RoutineDef::parse(json).expect("corpus def must parse")
}

fn corpus() -> Vec<CorpusEntry> {
    vec![
        CorpusEntry {
            key: "checkin",
            request: "Every half hour between 07:00 and 08:00, try to connect to my Oregon \
                      gateway set on 40 meters; if a connection succeeds, fill the check-in \
                      form from my saved draft 'Morning check-in', send it to W7XYZ-10, and \
                      stage it in the Outbox; if not, notify me that no gateway was reached. \
                      Always wait for my transmit consent before sending anything.",
            def: parse(
                r#"{"routine":"morning-check-in","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"30m","window":"07:00-08:00","if_missed":"skip"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"radio.connect","params":{"stations":"@station-set:or-gateways","bands":["40m"]},"timeout_s":300},
                  {"id":"s2","control":"branch","on":"s1.connected","then":["s3"],"else":["s4"]},
                  {"id":"s3","action":"local.compose","params":{"to":["W7XYZ-10"],"template":"@draft:d-morning"}},
                  {"id":"s4","action":"local.notify","params":{"message":"no gateway reached"}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "wx-pull",
            request: "When I start it manually, capture WWV space-weather, then write a log \
                      line. It should never transmit without my consent.",
            def: parse(
                r#"{"routine":"wx-pull","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"manual"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"data.spacewx_wwv","params":{}},
                  {"id":"s2","action":"local.log","params":{}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "aprs-beacon",
            request: "Every 10 minutes, send an APRS packet to APRS-IS via WIDE1-1. I have \
                      acknowledged automatic transmission for this one — it may transmit \
                      without asking each time.",
            def: parse(
                r#"{"routine":"aprs-beacon","schema_version":1,"transmit_mode":"automatic",
                "transmit_ack":{"by":"KK7ABC","at":"2026-08-01T00:00:00Z","closure_digest":"abc"},
                "triggers":[{"type":"schedule","every":"10m","if_missed":"skip"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"radio.aprs_send","params":{"to":"WIDE1-1"}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "stationlist",
            request: "Every day, aligned to the day, refresh the station list, and if a \
                      missed run happens, run once at launch. No transmission involved; it \
                      still waits for consent on anything that would send.",
            def: parse(
                r#"{"routine":"stationlist-refresh","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"1d","align":"day","if_missed":"run_once_on_launch"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"data.stationlist_update","params":{}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "position-report",
            request: "Every 2 hours between 06:00 and 22:00, fill the Position Report form \
                      from my saved draft 'Field position', address it to W7SAR-5, and stage \
                      it; wait for my consent before any transmit.",
            def: parse(
                r#"{"routine":"position-report","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"2h","window":"06:00-22:00","if_missed":"skip"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"local.compose","params":{"to":["W7SAR-5"],"template":"@draft:d-position"}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "rig-cycle",
            request: "When started manually: validate the '40m-digital' rig preset, apply \
                      it, then read back the rig state; retry the apply up to 3 times with \
                      15 seconds between attempts.",
            def: parse(
                r#"{"routine":"rig-cycle","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"manual"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"rig.validate_preset","params":{"preset":"40m-digital"}},
                  {"id":"s2","action":"rig.apply_preset","params":{"preset":"40m-digital"}},
                  {"id":"r1","control":"retry","step":"s2","attempts":3,"backoff_s":15},
                  {"id":"s3","action":"rig.read_state","params":{}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "listen-then-connect",
            request: "When started manually: listen on the radio, wait 5 minutes, then \
                      connect to my Washington gateways on 80 meters and exchange mail, \
                      waiting for my transmit consent.",
            def: parse(
                r#"{"routine":"listen-then-connect","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"manual"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"radio.listen","params":{}},
                  {"id":"d1","control":"delay","delay":"+5m"},
                  {"id":"s2","action":"radio.connect","params":{"stations":"@station-set:wa-gateways","bands":["80m"]}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "kindex-guard",
            request: "Every hour: fetch SWPC space-weather; if the K-index is 4 or higher, \
                      stop and mark the run failed with the reason 'geomagnetic storm'; \
                      otherwise connect to my Oregon gateways. Waits for my consent to send.",
            def: parse(
                r#"{"routine":"kindex-guard","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"1h","if_missed":"skip"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"data.spacewx_swpc","params":{}},
                  {"id":"b1","control":"branch","on":"s1.k_index","op":"gte","value":4,"then":["e1"],"else":["s2"]},
                  {"id":"e1","control":"end","failed":true,"reason":"geomagnetic storm"},
                  {"id":"s2","action":"radio.connect","params":{"stations":"@station-set:or-gateways"}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "call-child",
            request: "Every 6 hours, run my 'wx-pull' routine and wait for it to finish, \
                      then show me a notification 'weather updated'. Consent required for \
                      any transmission.",
            def: parse(
                r#"{"routine":"wx-cycle","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"6h","if_missed":"skip"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"c1","control":"call","routine":"wx-pull","sync":true},
                  {"id":"s1","action":"local.notify","params":{"message":"weather updated"}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "identity-switch",
            request: "When started manually: switch the sending identity to KK7TAC, then \
                      compose a message with subject 'Tactical check' to W7EOC and stage it \
                      as KK7TAC. Waits for my transmit consent.",
            def: parse(
                r#"{"routine":"tactical-check","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"manual"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"local.set_identity","params":{"callsign":"KK7TAC"}},
                  {"id":"s2","action":"local.compose","params":{"to":["W7EOC"],"subject":"Tactical check","from_identity":{"callsign":"KK7TAC"}}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "dual-track",
            request: "Every 45 minutes: on its watch track, listen on the radio; on its \
                      mail track, connect to my Oregon gateways on 40 meters and exchange \
                      mail. Waits for my transmit consent.",
            def: parse(
                r#"{"routine":"watch-and-mail","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"schedule","every":"45m","if_missed":"skip"}],
                "tracks":[
                  {"name":"watch","steps":[{"id":"w1","action":"radio.listen","params":{}}]},
                  {"name":"mail","steps":[{"id":"m1","action":"radio.connect","params":{"stations":"@station-set:or-gateways","bands":["40m"]}}]}]}"#,
            ),
        },
        CorpusEntry {
            key: "docs-notify",
            request: "When started manually, search the docs for 'antenna tuning' and show \
                      me a notification 'lookup done'. Consent required for anything that \
                      transmits.",
            def: parse(
                r#"{"routine":"docs-lookup","schema_version":1,"transmit_mode":"attended",
                "triggers":[{"type":"manual"}],
                "tracks":[{"name":"main","steps":[
                  {"id":"s1","action":"data.docs_search","params":{"query":"antenna tuning"}},
                  {"id":"s2","action":"local.notify","params":{"message":"lookup done"}}]}]}"#,
            ),
        },
    ]
}

/// One applied mutation: the divergent def, plus the strings whose presence
/// in a judge's answer counts as STRICT detection (old or new value named).
struct Mutation {
    class: &'static str,
    def: RoutineDef,
    detect_keys: Vec<String>,
}

fn first_compose(def: &RoutineDef) -> Option<(usize, usize)> {
    for (ti, t) in def.tracks.iter().enumerate() {
        for (si, s) in t.steps.iter().enumerate() {
            if let Step::Action(a) = s {
                if a.action == "local.compose" {
                    return Some((ti, si));
                }
            }
        }
    }
    None
}

fn mutations(entry: &CorpusEntry) -> Vec<Mutation> {
    let mut out = Vec::new();
    let base = &entry.def;

    // recipient: first compose's first `to` gets a near-miss callsign.
    if let Some((ti, si)) = first_compose(base) {
        let mut m = base.clone();
        if let Step::Action(a) = &mut m.tracks[ti].steps[si] {
            let old = a.params["to"][0].as_str().unwrap().to_string();
            let new = format!("{}1", old.trim_end_matches(|c: char| c.is_ascii_digit()));
            a.params["to"] = json!([new.clone()]);
            out.push(Mutation {
                class: "recipient",
                def: m,
                detect_keys: vec![old, new],
            });
        }
    }

    // schedule: every-interval swapped.
    if base
        .triggers
        .iter()
        .any(|t| matches!(t, Trigger::Schedule { .. }))
    {
        let mut m = base.clone();
        let mut keys = Vec::new();
        for t in &mut m.triggers {
            if let Trigger::Schedule { every, .. } = t {
                let old = every.clone();
                let new = if old == "2h" { "4h".to_string() } else { "2h".to_string() };
                keys = vec![old, new.clone()];
                *every = new;
            }
        }
        out.push(Mutation {
            class: "schedule",
            def: m,
            detect_keys: keys,
        });
    }

    // window: shifted, where present.
    {
        let mut m = base.clone();
        let mut keys = Vec::new();
        for t in &mut m.triggers {
            if let Trigger::Schedule {
                window: Some(w), ..
            } = t
            {
                let old = w.clone();
                let new = "09:00-10:00".to_string();
                keys = vec![old, "09:00".to_string()];
                *w = new;
            }
        }
        if !keys.is_empty() {
            out.push(Mutation {
                class: "window",
                def: m,
                detect_keys: keys,
            });
        }
    }

    // consent: attended flipped to automatic WITHOUT an ack.
    if base.transmit_mode == TransmitMode::Attended {
        let mut m = base.clone();
        m.transmit_mode = TransmitMode::Automatic;
        out.push(Mutation {
            class: "consent",
            def: m,
            detect_keys: vec!["automat".to_string(), "consent".to_string()],
        });
    }

    // draft: the @draft: reference swapped to a different saved draft.
    if let Some((ti, si)) = first_compose(base) {
        if let Step::Action(a) = &base.tracks[ti].steps[si] {
            if a.params.get("template").and_then(|t| t.as_str()).is_some() {
                let mut m = base.clone();
                if let Step::Action(a) = &mut m.tracks[ti].steps[si] {
                    let old = a.params["template"].as_str().unwrap().to_string();
                    let new = if old.ends_with("d-morning") {
                        "@draft:d-position"
                    } else {
                        "@draft:d-morning"
                    };
                    a.params["template"] = json!(new);
                    out.push(Mutation {
                        class: "draft",
                        def: m,
                        detect_keys: vec![
                            "Morning check-in".to_string(),
                            "Field position".to_string(),
                        ],
                    });
                }
            }
        }
    }

    // params: bands swapped on the first radio.connect that has them.
    'bands: for (ti, t) in base.tracks.iter().enumerate() {
        for (si, s) in t.steps.iter().enumerate() {
            if let Step::Action(a) = s {
                if a.action == "radio.connect" && a.params.get("bands").is_some() {
                    let mut m = base.clone();
                    if let Step::Action(a) = &mut m.tracks[ti].steps[si] {
                        let old = a.params["bands"][0].as_str().unwrap().to_string();
                        let new = if old == "40m" { "20m" } else { "40m" };
                        a.params["bands"] = json!([new]);
                        out.push(Mutation {
                            class: "params",
                            def: m,
                            detect_keys: vec![old, new.to_string()],
                        });
                    }
                    break 'bands;
                }
            }
        }
    }

    // step_dropped: last step of the last track, provided nothing branches to it.
    {
        let referenced: Vec<String> = base
            .tracks
            .iter()
            .flat_map(|t| t.steps.iter())
            .filter_map(|s| match s {
                Step::Control(c) => match &c.control {
                    Control::Branch { then, r#else, .. } => Some(
                        then.iter()
                            .chain(r#else.iter())
                            .map(|i| i.0.clone())
                            .collect::<Vec<_>>(),
                    ),
                    Control::Retry { step, .. } => Some(vec![step.0.clone()]),
                    _ => None,
                },
                _ => None,
            })
            .flatten()
            .collect();
        let mut m = base.clone();
        if let Some(track) = m.tracks.last_mut() {
            if track.steps.len() > 1 {
                let last_id = track.steps.last().unwrap().id().0.clone();
                if !referenced.contains(&last_id) {
                    let dropped = track.steps.pop().unwrap();
                    let key = match &dropped {
                        Step::Action(a) => a.action.clone(),
                        Step::Control(_) => last_id.clone(),
                    };
                    out.push(Mutation {
                        class: "step_dropped",
                        def: m,
                        detect_keys: vec![last_id, key],
                    });
                }
            }
        }
    }

    // retry: attempts collapsed to 1 where a retry exists.
    {
        let mut m = base.clone();
        let mut hit = false;
        for t in &mut m.tracks {
            for s in &mut t.steps {
                if let Step::Control(c) = s {
                    if let Control::Retry { attempts, .. } = &mut c.control {
                        *attempts = 1;
                        hit = true;
                    }
                }
            }
        }
        if hit {
            out.push(Mutation {
                class: "retry",
                def: m,
                detect_keys: vec!["retry".to_string(), "1 time".to_string()],
            });
        }
    }

    out
}

fn names() -> DisplayNames {
    DisplayNames {
        draft_labels: BTreeMap::from([
            ("d-morning".to_string(), "Morning check-in".to_string()),
            ("d-position".to_string(), "Field position".to_string()),
        ]),
    }
}

fn emit(case: serde_json::Value) {
    println!("{case}");
}

fn main() {
    let names = names();

    for entry in corpus() {
        let muts = mutations(&entry);

        // Styles A and B: one clean case + one per mutation.
        for (style, render) in [
            ("A", narrative as fn(&RoutineDef, &DisplayNames) -> String),
            ("B", scannable as fn(&RoutineDef, &DisplayNames) -> String),
        ] {
            emit(json!({
                "case_id": format!("{}.none.{style}", entry.key),
                "style": style,
                "mutation": "none",
                "clean": true,
                "request": entry.request,
                "readback": render(&entry.def, &names),
                "detect_keys": [],
            }));
            for m in &muts {
                emit(json!({
                    "case_id": format!("{}.{}.{style}", entry.key, m.class),
                    "style": style,
                    "mutation": m.class,
                    "clean": false,
                    "request": entry.request,
                    "readback": render(&m.def, &names),
                    "detect_keys": m.detect_keys,
                }));
            }
        }

        // Style C: edit scenarios. The request is ONE edit; the readback is
        // the diff between the previous def and what the app actually built.
        for m in &muts {
            // The "requested edit" is the mutation itself, described plainly;
            // C's divergence cases build something OTHER than that edit.
            let edit_request = format!(
                "I asked for exactly one change to my existing routine '{}': make its \
                 {} what the summary below should reflect — specifically: {}. Nothing \
                 else was to change.",
                entry.def.routine,
                m.class.replace('_', " "),
                m.detect_keys.last().cloned().unwrap_or_default(),
            );

            // Faithful: the app applied the requested edit.
            emit(json!({
                "case_id": format!("{}.{}.C-faithful", entry.key, m.class),
                "style": "C",
                "mutation": "none",
                "clean": true,
                "request": edit_request,
                "readback": diff(&entry.def, &m.def, &names),
                "detect_keys": [],
            }));

            // Not applied: the app changed nothing.
            emit(json!({
                "case_id": format!("{}.{}.C-not-applied", entry.key, m.class),
                "style": "C",
                "mutation": "edit_not_applied",
                "clean": false,
                "request": edit_request,
                "readback": diff(&entry.def, &entry.def, &names),
                "detect_keys": ["No changes"],
            }));

            // Extra change: the requested edit PLUS a smuggled consent flip.
            if m.def.transmit_mode == TransmitMode::Attended {
                let mut extra = m.def.clone();
                extra.transmit_mode = TransmitMode::Automatic;
                emit(json!({
                    "case_id": format!("{}.{}.C-extra-change", entry.key, m.class),
                    "style": "C",
                    "mutation": "edit_extra_change",
                    "clean": false,
                    "request": edit_request,
                    "readback": diff(&entry.def, &extra, &names),
                    "detect_keys": ["automat", "consent"],
                }));
            }
        }
    }
}
