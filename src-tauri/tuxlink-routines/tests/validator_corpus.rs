//! Fixture corpus + integration test (plan-3 task 6): the failure-taxonomy
//! corpus for `validate()` / `validate_fleet()`. One fixture routine per
//! finding code (valid-but-for-one-defect), two fleet-fixture pairs
//! (`SCHEDULE_COLLISION`, `SAME_EFFECT_OVERLAP`), and three fully-authored
//! grounding-scenario fixtures from spec §1.
//!
//! `tests/fixtures/routines/manifest.json` maps each fixture to its expected
//! finding-code set and a small per-fixture `ValidationContext` spec
//! (actions to register, entities to seed, station profile, inline sibling
//! routines for `Call`-closure fixtures). This file walks the manifest,
//! builds a `StaticContext` per entry, runs `validate()`/`validate_fleet()`,
//! and asserts the actual finding-code set is EXACTLY the expected set (not
//! a superset) — an unexpected extra finding fails the fixture just as
//! loudly as a missing one.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use tuxlink_routines::action::ActionDescriptor;
use tuxlink_routines::types::RoutineDef;
use tuxlink_routines::validate::{
    validate, validate_fleet, Finding, StaticContext, StationProfile,
};

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/routines");

// --- Manifest shape --------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Manifest {
    fixtures: Vec<FixtureEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum FixtureEntry {
    Single {
        name: String,
        file: String,
        expected_codes: Vec<String>,
        context: ContextSpec,
    },
    Fleet {
        name: String,
        files: Vec<String>,
        expected_codes: Vec<String>,
        now_unix: i64,
        context: ContextSpec,
    },
}

#[derive(Debug, Deserialize, Default)]
struct ContextSpec {
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    entities: Vec<EntitySpec>,
    #[serde(default)]
    profile: ProfileSpec,
    /// Inline `RoutineDef` JSON objects registered as sibling routines
    /// (`ctx.routine_def`), for `Call`-closure fixtures (consent, recursion,
    /// missing-target). Inline rather than separate fixture files: these
    /// callees aren't themselves a "one fixture per finding code" entry,
    /// just context the caller fixture needs to resolve its closure.
    #[serde(default)]
    sibling_routines: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct EntitySpec {
    kind: String,
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct ProfileSpec {
    #[serde(default)]
    has_internet: bool,
    #[serde(default)]
    rigs: Vec<String>,
}

// --- Fixed action catalog ---------------------------------------------------

/// The fixture corpus's action catalog. `ActionDescriptor.name` is a
/// `&'static str` (the production registry's own constraint — see
/// `action.rs`), so fixture context specs reference actions by name and this
/// function is the single lookup table translating a name into its
/// descriptor. A name that isn't in this table is a fixture-authoring typo,
/// not a legitimately "unknown to the registry" action (that case is
/// `UNKNOWN_ACTION`'s own fixture, whose context deliberately omits an
/// action from `actions` instead of adding an unmapped name here).
fn known_action(name: &str) -> ActionDescriptor {
    match name {
        "radio.connect" => ActionDescriptor {
            name: "radio.connect",
            needs_radio: true,
            transmits: true,
            needs_internet: false,
        },
        "data.web_lookup" => ActionDescriptor {
            name: "data.web_lookup",
            needs_radio: false,
            transmits: false,
            needs_internet: true,
        },
        "local.note" => ActionDescriptor {
            name: "local.note",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        },
        "compose.message" => ActionDescriptor {
            name: "compose.message",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        },
        "log.entry" => ActionDescriptor {
            name: "log.entry",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        },
        "data.read" => ActionDescriptor {
            name: "data.read",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        },
        "data.spacewx_wwv" => ActionDescriptor {
            name: "data.spacewx_wwv",
            needs_radio: true,
            transmits: false,
            needs_internet: false,
        },
        other => panic!(
            "fixture context references unknown action \"{other}\" — add it to \
             known_action() in tests/validator_corpus.rs"
        ),
    }
}

// --- Loading helpers ---------------------------------------------------

fn load_manifest() -> Manifest {
    let path = Path::new(FIXTURES_DIR).join("manifest.json");
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"))
}

fn load_routine(file: &str) -> RoutineDef {
    let path = Path::new(FIXTURES_DIR).join(file);
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    RoutineDef::parse(&raw).unwrap_or_else(|e| panic!("parsing {path:?}: {e:?}"))
}

fn build_context(spec: &ContextSpec) -> StaticContext {
    let mut ctx = StaticContext::new();
    for action_name in &spec.actions {
        ctx = ctx.with_action(known_action(action_name));
    }
    for entity in &spec.entities {
        ctx = ctx.with_entity(&entity.kind, &entity.name);
    }
    for sibling in &spec.sibling_routines {
        let def: RoutineDef = serde_json::from_value(sibling.clone()).unwrap_or_else(|e| {
            panic!("sibling routine in manifest.json failed to parse: {e}\n{sibling:#}")
        });
        ctx = ctx.with_routine(def);
    }
    ctx.with_profile(StationProfile {
        has_internet: spec.profile.has_internet,
        rigs: spec.profile.rigs.clone(),
    })
}

fn codes_of(findings: &[Finding]) -> HashSet<&str> {
    findings.iter().map(|f| f.code).collect()
}

fn as_str_set(codes: &[String]) -> HashSet<&str> {
    codes.iter().map(String::as_str).collect()
}

fn single_fixture<'a>(
    manifest: &'a Manifest,
    name: &str,
) -> (&'a str, &'a [String], &'a ContextSpec) {
    for entry in &manifest.fixtures {
        if let FixtureEntry::Single {
            name: n,
            file,
            expected_codes,
            context,
        } = entry
        {
            if n == name {
                return (file.as_str(), expected_codes.as_slice(), context);
            }
        }
    }
    panic!("no single fixture named \"{name}\" in manifest.json");
}

fn fleet_fixture<'a>(
    manifest: &'a Manifest,
    name: &str,
) -> (&'a [String], &'a [String], i64, &'a ContextSpec) {
    for entry in &manifest.fixtures {
        if let FixtureEntry::Fleet {
            name: n,
            files,
            expected_codes,
            now_unix,
            context,
        } = entry
        {
            if n == name {
                return (files.as_slice(), expected_codes.as_slice(), *now_unix, context);
            }
        }
    }
    panic!("no fleet fixture named \"{name}\" in manifest.json");
}

// --- The corpus-wide test ---------------------------------------------------

/// Every finding-code constant defined across `src/validate/*.rs` (checked
/// against source via `grep -n "pub const" src/validate/*.rs` at authoring
/// time). v1 maintenance rule: this is a hardcoded list, not introspected —
/// bump it by hand whenever a `validate/*.rs` module adds a `pub const`
/// finding code, and add/extend a fixture whose `expected_codes` names it in
/// the same change (`fixture_corpus_covers_every_finding_code` below fails
/// loudly otherwise).
const ALL_FINDING_CODES: &[&str] = &[
    // refs.rs
    "UNRESOLVED_REF",
    "UNKNOWN_ACTION",
    // capability.rs
    "NEEDS_INTERNET_OFFGRID",
    "NO_RIG_CONFIGURED",
    "SAME_RIG_PARALLEL_LANES",
    // contracts.rs
    "UNSATISFIABLE_VAR",
    "BRANCH_ON_UNKNOWN",
    "CROSS_TRACK_VAR",
    // structure.rs
    "UNREACHABLE_STEP",
    "NO_TERMINAL_PATH",
    "RETRY_ZERO_ATTEMPTS",
    "RETRY_TARGET_MISSING",
    "RETRY_TARGET_NOT_ACTION",
    "BRANCH_CYCLE",
    "BRANCH_TARGET_MISSING",
    "CALL_RECURSION",
    "CALL_TARGET_MISSING",
    // consent.rs
    "AUTO_TX_UNACKED",
    "MIXED_MODE_STALL",
    "ATTENDED_UNDER_SCHEDULE",
    // fleet.rs
    "SCHEDULE_COLLISION",
    "SAME_EFFECT_OVERLAP",
    // triggers.rs (plan-4 amendment task 1: one-cadence spec change)
    "MULTIPLE_SCHEDULES",
    // capability.rs (plan-4 amendment task 1: WWV timeout heuristic)
    "STEP_TIMEOUT_LIKELY_INSUFFICIENT",
];

#[test]
fn corpus_fixtures_produce_exactly_their_expected_finding_codes() {
    let manifest = load_manifest();
    let mut seen_codes: HashSet<String> = HashSet::new();
    let mut checked = 0usize;

    for entry in &manifest.fixtures {
        match entry {
            FixtureEntry::Single {
                name,
                file,
                expected_codes,
                context,
            } => {
                let def = load_routine(file);
                let ctx = build_context(context);
                let findings = validate(&def, &ctx);
                let actual = codes_of(&findings);
                let expected = as_str_set(expected_codes);
                assert_eq!(
                    actual, expected,
                    "fixture \"{name}\" ({file}): finding codes mismatch. findings={findings:?}"
                );
                seen_codes.extend(expected_codes.iter().cloned());
                checked += 1;
            }
            FixtureEntry::Fleet {
                name,
                files,
                expected_codes,
                now_unix,
                context,
            } => {
                let defs: Vec<RoutineDef> = files.iter().map(|f| load_routine(f)).collect();
                let ctx = build_context(context);
                let findings = validate_fleet(&defs, &ctx, *now_unix, 0);
                let actual = codes_of(&findings);
                let expected = as_str_set(expected_codes);
                assert_eq!(
                    actual, expected,
                    "fleet fixture \"{name}\" ({files:?}): finding codes mismatch. findings={findings:?}"
                );
                seen_codes.extend(expected_codes.iter().cloned());
                checked += 1;
            }
        }
    }

    assert!(
        checked >= ALL_FINDING_CODES.len(),
        "expected at least {} fixtures (one per finding code), found {checked}",
        ALL_FINDING_CODES.len()
    );

    for code in ALL_FINDING_CODES {
        assert!(
            seen_codes.contains(*code),
            "finding code {code} has no fixture in manifest.json's expected_codes — \
             every pub const finding code in src/validate/*.rs must be corpus-tested"
        );
    }
}

#[test]
fn ics_log_cycle_and_net_checkin_grounding_scenarios_are_fully_valid() {
    // Spec §1 grounding scenarios 1 and 3 (net-checkin is the "guided
    // sequence" shape referenced by plan 3 task 6, not a numbered spec
    // scenario on its own): both are authored to have zero errors AND zero
    // warnings — the strongest form of "produce ZERO Errors" the task asks
    // for. Kept as a separate, explicitly named assertion (not just folded
    // into the generic corpus loop above) so the grounding-scenario
    // guarantee stays legible on its own.
    let manifest = load_manifest();
    for name in ["ics-log-cycle", "net-checkin"] {
        let (file, expected_codes, context) = single_fixture(&manifest, name);
        assert!(
            expected_codes.is_empty(),
            "grounding scenario \"{name}\"'s manifest entry should expect zero findings"
        );
        let def = load_routine(file);
        let ctx = build_context(context);
        let findings = validate(&def, &ctx);
        assert!(
            findings.is_empty(),
            "grounding scenario \"{name}\" should have zero errors and zero warnings, got {findings:?}"
        );
    }
}

#[test]
fn deployment_fleet_coexistence_grounding_scenario_has_zero_errors_and_zero_warnings() {
    // Spec §1 grounding scenario 2, re-authored per the 2026-07-14
    // one-cadence spec amendment (§5/§14): the old single "deployment-poll"
    // routine carried TWO schedule triggers (a 30m connect poll and a 6h
    // wx-post) on one routine, which the one-cadence rule now forbids
    // (MULTIPLE_SCHEDULES). It is re-authored as two SEPARATE routines, each
    // with its own single schedule: "deployment-connect-cycle" (30m radio
    // poll) and "wx-post-and-catalog" (6h, reads the last-connected gateway
    // via a same-track `data.read` step — no cross-track var, and no radio
    // touch of its own, so it never contends for the rig). The pair is the
    // SCHEDULE_COLLISION-free demonstration of fleet coexistence the plan-4
    // amendment calls for: both routines individually valid, and the fleet
    // check adds nothing on top (no collision, no shared-effect overlap).
    // This test pins that "zero findings, both individually and as a fleet"
    // guarantee so a validator regression is caught here, not just in the
    // generic set-equality loop above.
    let manifest = load_manifest();
    let (files, expected_codes, now_unix, context) =
        fleet_fixture(&manifest, "deployment-fleet-coexistence");
    let ctx = build_context(context);

    // Each routine is individually clean.
    for file in files {
        let def = load_routine(file);
        let findings = validate(&def, &ctx);
        assert!(
            findings.is_empty(),
            "\"{file}\" should individually validate clean, got {findings:?}"
        );
    }

    // The fleet check adds nothing on top: no SCHEDULE_COLLISION (only one
    // of the pair touches the radio), no SAME_EFFECT_OVERLAP (they share no
    // data.* action name).
    let defs: Vec<RoutineDef> = files.iter().map(|f| load_routine(f)).collect();
    let findings = validate_fleet(&defs, &ctx, now_unix, 0);
    assert!(
        findings.is_empty(),
        "deployment-fleet-coexistence should have zero findings, got {findings:?}"
    );
    assert!(
        expected_codes.is_empty(),
        "manifest.json drifted from the documented zero-findings deployment-fleet-coexistence demo"
    );
}

#[test]
fn fixture_corpus_never_contains_the_word_workflow() {
    // Project-wide naming ban (spec header + plan-3 §"Global Constraints"):
    // "workflow" must not appear anywhere, including fixture JSON content
    // (a routine field value, a comment-shaped string, anything) — the
    // corpus is meant to double as documentation an operator might read.
    let dir = Path::new(FIXTURES_DIR);
    let mut checked = 0usize;
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("reading {dir:?}: {e}")) {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
        assert!(
            !content.to_lowercase().contains("workflow"),
            "{path:?} contains the banned word \"workflow\""
        );
        checked += 1;
    }
    assert!(
        checked > 0,
        "expected at least one fixture JSON file in {dir:?}"
    );
}
