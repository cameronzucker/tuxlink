//! Headless Elmer battery SCORER (Routine CI slice 1a, Task 14c).
//!
//! The battery (`elmer_battery`) writes one bundle per {model × corpus prompt}
//! cell but does NOT score it. This binary scores each bundle with two layers:
//!
//!   1. A DETERMINISTIC layer read from the bundle's already-computed
//!      `validate.json` (the same `validate_routine` verdict the MCP tool uses)
//!      plus `outcome.json`'s `workflow_run` — no model call.
//!   2. The project-REQUIRED SUBJECTIVE LLM JUDGE — no longer an API call from
//!      this binary. The judge is the orchestrating Claude AGENT reading the
//!      bundle: this binary's job is to EMIT a self-contained `judge_input`
//!      package per cell (the task, the predicates, the authored def(s), and a
//!      workflow summary) so the agent can score it directly. `judge` is
//!      written as `null` — a placeholder the agent fills in once it judges
//!      the cell.
//!
//! It writes `<bundle>/score.json` per cell, a roll-up `<root>/scores.jsonl`,
//! and a roll-up `<root>/judge-queue.jsonl` (one `judge_input` object per
//! line, across every scored bundle, so the agent can read every cell to
//! judge from a single file).
//!
//!   elmer_score --root <results-root> --corpus <corpus.json> \
//!       [--only <bundle-subdir>] [--redo]
//!
//! This binary makes NO network calls, TRANSMITS NOTHING, and runs NO
//! routines: it is pure file I/O (reads bundle JSON, writes score JSON). No
//! Tauri app, no egress guard, no radio path, no API key.
//!
//! MSRV 1.75. The repo does not compile on the dev Pi; CI is the gate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct CliArgs {
    root: PathBuf,
    corpus: PathBuf,
    only: Option<String>,
    redo: bool,
}

const USAGE: &str = "usage: elmer_score --root <results-root> --corpus <corpus.json> \
     [--only <bundle-subdir>] [--redo]   (pure file I/O — no network call, no \
     API key; the LLM judge is the orchestrating agent reading judge-queue.jsonl)";

fn parse_cli(args: &[String]) -> Result<CliArgs, String> {
    let mut root = None;
    let mut corpus = None;
    let mut only = None;
    let mut redo = false;

    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut val = |name: &str| -> Result<String, String> {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} requires a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--root" => root = Some(PathBuf::from(val("--root")?)),
            "--corpus" => corpus = Some(PathBuf::from(val("--corpus")?)),
            "--only" => only = Some(val("--only")?),
            "--redo" => redo = true,
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }

    Ok(CliArgs {
        root: root.ok_or_else(|| format!("--root is required\n{USAGE}"))?,
        corpus: corpus.ok_or_else(|| format!("--corpus is required\n{USAGE}"))?,
        only,
        redo,
    })
}

// ---------------------------------------------------------------------------
// Corpus (this binary's OWN structs — elmer_battery's are in a bin, not
// importable). Every field the 7 prescriptive entries lack is #[serde(default)].
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Corpus {
    #[serde(default)]
    prompts: Vec<CorpusEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct CorpusEntry {
    id: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    predicates: Vec<String>,
    /// BUILDABLE | PARTIAL-GAP | NON-ROUTINE (absent on the 7 prescriptive
    /// prompts, which take the empty-string default → deterministic
    /// `inconclusive`, judge primary).
    #[serde(default)]
    classification: String,
    #[serde(default)]
    expected_gap: String,
    #[serde(default)]
    judge_primary: bool,
    /// EU3: a no-routine outcome is CORRECT; the deterministic layer must
    /// never auto-fail it.
    #[serde(default)]
    no_routine_expected: bool,
}

fn load_corpus(path: &Path) -> Result<HashMap<String, CorpusEntry>, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read corpus {}: {e}", path.display()))?;
    let corpus: Corpus = serde_json::from_slice(&bytes)
        .map_err(|e| format!("corpus {} did not parse: {e}", path.display()))?;
    Ok(corpus
        .prompts
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect())
}

// ---------------------------------------------------------------------------
// Bundle files (this binary's OWN read structs). outcome.json's top-level keys
// are snake_case (elmer_battery emits them via a json! literal); the nested
// `workflow_run` object is camelCase (serialized from `WorkflowRun`, which is
// `#[serde(rename_all = "camelCase")]`).
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct OutcomeFile {
    #[serde(default)]
    arm: String,
    #[serde(default)]
    workflow_run: Option<WorkflowRunView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRunView {
    #[serde(default)]
    depth: Option<String>,
    #[serde(default)]
    phases_run: Vec<PhaseView>,
    #[serde(default)]
    saved_routine: Option<String>,
    #[serde(default)]
    present: Option<Value>,
    #[serde(default)]
    stopped_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhaseView {
    #[serde(default)]
    name: String,
    #[serde(default)]
    outcome: String,
}

#[derive(Debug, Default, Deserialize)]
struct RunManifest {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prompt: Option<ManifestPrompt>,
}

#[derive(Debug, Default, Deserialize)]
struct ManifestPrompt {
    #[serde(default)]
    id: String,
}

// ---------------------------------------------------------------------------
// Deterministic layer
// ---------------------------------------------------------------------------

/// True iff `routine`'s `validate.json` entry is a findings ARRAY with no
/// Error-severity finding (spec §10: errors block enable/run). A missing entry,
/// an `{"error": ...}` validation-failure object, or any non-array shape is NOT
/// green.
fn validates_green(validate: &Value, routine: &str) -> bool {
    match validate.get(routine) {
        Some(Value::Array(findings)) => !findings
            .iter()
            .any(|f| f.get("severity").and_then(Value::as_str) == Some("error")),
        _ => false,
    }
}

/// The deterministic verdict per the Task-14c rubric:
/// - `no_routine_expected` (EU3) → `n/a` (a no-routine outcome is correct).
/// - `BUILDABLE` → `pass` iff a routine saved AND validates green, else `fail`.
/// - otherwise → `inconclusive` (the judge is primary for PARTIAL-GAP /
///   NON-ROUTINE and for the un-classified prescriptive prompts).
fn deterministic_verdict(
    classification: &str,
    no_routine_expected: bool,
    routine_saved: bool,
    validates_green: bool,
) -> &'static str {
    if no_routine_expected {
        return "n/a";
    }
    match classification {
        "BUILDABLE" => {
            if routine_saved && validates_green {
                "pass"
            } else {
                "fail"
            }
        }
        _ => "inconclusive",
    }
}

/// Non-`enabled` authored defs harvested into `<bundle>/routines/`, as
/// `(stem, raw_json)`. `enabled.json` is store bookkeeping, not a def.
fn collect_defs(bundle: &Path) -> Vec<(String, String)> {
    let mut defs = Vec::new();
    let dir = bundle.join("routines");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return defs;
    };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if stem == "enabled" {
            continue;
        }
        let stem = stem.to_string();
        match std::fs::read_to_string(&path) {
            Ok(content) => defs.push((stem, content)),
            Err(e) => eprintln!("elmer_score: could not read def {}: {e}", path.display()),
        }
    }
    defs
}

/// Parse each harvested def's raw JSON text into a `Value` for the judge
/// package. A def that fails to parse degrades to an inline error object
/// (never panics, never silently drops the cell) — this scorer must not choke
/// on a malformed authored file.
fn parse_defs(defs: &[(String, String)]) -> Vec<Value> {
    defs.iter()
        .map(|(stem, content)| {
            serde_json::from_str(content).unwrap_or_else(|e| {
                json!({ "name": stem, "parse_error": e.to_string() })
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scoring a single bundle
// ---------------------------------------------------------------------------

struct ScoreInputs<'a> {
    bundle: &'a Path,
    entry: &'a CorpusEntry,
    model: &'a str,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{} did not parse: {e}", path.display()))
}

/// Score one bundle: the deterministic layer from `validate.json` +
/// `outcome.json`, plus a `judge_input` package for the orchestrating agent to
/// score subjectively. `judge` is written as `null` — a placeholder the agent
/// fills in once it reads `judge_input` and judges the cell. Returns the
/// `score.json` value.
fn score_bundle(inp: ScoreInputs<'_>) -> Result<Value, String> {
    let ScoreInputs { bundle, entry, model } = inp;

    // ── Read the bundle ─────────────────────────────────────────────────────
    let outcome: OutcomeFile = {
        let v = read_json(&bundle.join("outcome.json"))?;
        serde_json::from_value(v).map_err(|e| format!("outcome.json shape unexpected: {e}"))?
    };
    // validate.json may be absent on a cell that saved no routine.
    let validate = match read_json(&bundle.join("validate.json")) {
        Ok(v) => v,
        Err(_) => Value::Object(serde_json::Map::new()),
    };
    let defs = collect_defs(bundle);
    let wf = outcome.workflow_run.clone();

    // ── Deterministic layer ─────────────────────────────────────────────────
    let saved_name: Option<String> = wf
        .as_ref()
        .and_then(|w| w.saved_routine.clone())
        .or_else(|| defs.first().map(|(stem, _)| stem.clone()));
    let routine_saved =
        wf.as_ref().is_some_and(|w| w.saved_routine.is_some()) || !defs.is_empty();
    let green = saved_name
        .as_deref()
        .is_some_and(|name| validates_green(&validate, name));
    let honest_stop = wf.as_ref().is_some_and(|w| w.stopped_reason.is_some());
    let verdict = deterministic_verdict(
        &entry.classification,
        entry.no_routine_expected,
        routine_saved,
        green,
    );
    let deterministic = json!({
        "verdict": verdict,
        "routine_saved": routine_saved,
        "validates_green": green,
        "honest_stop": honest_stop,
        "saved_routine": saved_name,
    });

    // ── Judge-input package (the judge is now the orchestrating agent) ─────
    let wf_summary = match &wf {
        Some(w) => json!({
            "depth": w.depth,
            "phases": w.phases_run.iter()
                .map(|p| json!({ "name": p.name, "outcome": p.outcome }))
                .collect::<Vec<_>>(),
            "saved_routine": w.saved_routine,
            "stopped_reason": w.stopped_reason,
            "present": w.present,
        }),
        None => json!({
            "note": "base/matched-control arm: no workflow_run (single send, no phase pipeline)"
        }),
    };
    let judge_input = json!({
        "corpus_id": entry.id,
        "arm": outcome.arm,
        "classification": entry.classification,
        "judge_primary": entry.judge_primary,
        "no_routine_expected": entry.no_routine_expected,
        "prompt": entry.prompt,
        "predicates": entry.predicates,
        "expected_gap": entry.expected_gap,
        "deterministic": verdict,
        "artifacts": {
            "def": parse_defs(&defs),
            "workflow_summary": wf_summary,
        },
    });

    Ok(json!({
        "corpus_id": entry.id,
        "model": model,
        "arm": outcome.arm,
        "classification": entry.classification,
        "judge_primary": entry.judge_primary,
        "deterministic": deterministic,
        "judge": Value::Null,
        "judge_input": judge_input,
    }))
}

// ---------------------------------------------------------------------------
// Bundle discovery
// ---------------------------------------------------------------------------

/// Collect every directory under `dir` (inclusive) that holds an `outcome.json`.
/// A bundle is a leaf — recursion stops once one is found. Order is
/// deterministic (sorted).
fn collect_bundles(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.join("outcome.json").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();
    for sub in subdirs {
        collect_bundles(&sub, out);
    }
}

/// Whether a bundle matches an `--only` filter — a whole-component path suffix
/// match or a leaf-name match.
fn bundle_matches_only(bundle: &Path, only: &str) -> bool {
    bundle.ends_with(only)
        || bundle
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == only)
}

/// Resolve a bundle's corpus prompt id: `run_manifest.json`'s `prompt.id` when
/// present, else the bundle's leaf directory name (the sweep lays cells out as
/// `<sweep>/<model>/<prompt-id>/`).
fn resolve_prompt_id(bundle: &Path, manifest: &RunManifest) -> Option<String> {
    if let Some(p) = &manifest.prompt {
        if !p.id.trim().is_empty() {
            return Some(p.id.clone());
        }
    }
    bundle
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
}

/// Resolve a bundle's model: `run_manifest.json`'s `model`, else the bundle's
/// parent directory name, else `"unknown"`.
fn resolve_model(bundle: &Path, manifest: &RunManifest) -> String {
    if let Some(m) = &manifest.model {
        if !m.trim().is_empty() {
            return m.clone();
        }
    }
    bundle
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// Roll-ups — both scan EVERY bundle under root (never the `--only`-filtered
// set) so a targeted re-score (`--only X --redo`) never shrinks either
// roll-up to just the re-scored cell(s). Bundles without a score.json yet, or
// a score.json without a judge_input, are simply skipped.
// ---------------------------------------------------------------------------

/// Rebuild `<root>/scores.jsonl` as one compact `score.json` object per line,
/// across every bundle under `root`.
fn build_rollup(root: &Path) -> String {
    let mut all_bundles = Vec::new();
    collect_bundles(root, &mut all_bundles);
    let mut rollup = String::new();
    for bundle in &all_bundles {
        let score_path = bundle.join("score.json");
        if let Ok(v) = read_json(&score_path) {
            match serde_json::to_string(&v) {
                Ok(line) => {
                    rollup.push_str(&line);
                    rollup.push('\n');
                }
                Err(e) => eprintln!(
                    "elmer_score: could not compact {}: {e}",
                    score_path.display()
                ),
            }
        }
    }
    rollup
}

/// Rebuild `<root>/judge-queue.jsonl` as one `judge_input` object per line,
/// across every bundle under `root` whose `score.json` carries one — the
/// agent reads this single file to judge every cell.
fn build_judge_queue(root: &Path) -> String {
    let mut all_bundles = Vec::new();
    collect_bundles(root, &mut all_bundles);
    let mut queue = String::new();
    for bundle in &all_bundles {
        let score_path = bundle.join("score.json");
        let Ok(v) = read_json(&score_path) else {
            continue;
        };
        let Some(judge_input) = v.get("judge_input") else {
            continue;
        };
        match serde_json::to_string(judge_input) {
            Ok(line) => {
                queue.push_str(&line);
                queue.push('\n');
            }
            Err(e) => eprintln!(
                "elmer_score: could not compact judge_input for {}: {e}",
                score_path.display()
            ),
        }
    }
    queue
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    if let Err(e) = real_main() {
        eprintln!("elmer_score: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = parse_cli(&args)?;

    let corpus = load_corpus(&cli.corpus)?;

    let mut bundles = Vec::new();
    collect_bundles(&cli.root, &mut bundles);
    if let Some(only) = &cli.only {
        bundles.retain(|b| bundle_matches_only(b, only));
    }
    if bundles.is_empty() {
        eprintln!(
            "elmer_score: no bundles with outcome.json under {}{}",
            cli.root.display(),
            cli.only
                .as_deref()
                .map(|o| format!(" matching --only {o:?}"))
                .unwrap_or_default()
        );
    }

    let mut scored = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for bundle in &bundles {
        let score_path = bundle.join("score.json");
        if score_path.is_file() && !cli.redo {
            skipped += 1;
            continue;
        }

        let manifest: RunManifest = match read_json(&bundle.join("run_manifest.json")) {
            Ok(v) => serde_json::from_value(v).unwrap_or_default(),
            Err(_) => RunManifest::default(),
        };
        let Some(prompt_id) = resolve_prompt_id(bundle, &manifest) else {
            eprintln!(
                "elmer_score: could not resolve prompt id for {} — skipped",
                bundle.display()
            );
            failed += 1;
            continue;
        };
        let Some(entry) = corpus.get(&prompt_id) else {
            eprintln!(
                "elmer_score: prompt {prompt_id:?} (bundle {}) not in corpus — skipped",
                bundle.display()
            );
            failed += 1;
            continue;
        };
        let model = resolve_model(bundle, &manifest);

        match score_bundle(ScoreInputs {
            bundle,
            entry,
            model: &model,
        }) {
            Ok(score) => {
                let json = match serde_json::to_vec_pretty(&score) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!(
                            "elmer_score: could not serialize score for {}: {e}",
                            bundle.display()
                        );
                        failed += 1;
                        continue;
                    }
                };
                if let Err(e) = std::fs::write(&score_path, json) {
                    eprintln!("elmer_score: could not write {}: {e}", score_path.display());
                    failed += 1;
                    continue;
                }
                scored += 1;
                println!(
                    "elmer_score: scored {} × {} → {}",
                    model,
                    prompt_id,
                    score_path.display()
                );
            }
            Err(e) => {
                eprintln!("elmer_score: scoring {} failed: {e}", bundle.display());
                failed += 1;
            }
        }
    }

    // ── Roll-ups: rebuilt (not appended) so re-runs and --redo never
    // duplicate lines and skipped-but-already-scored cells still appear. Both
    // scan EVERY bundle under root — see build_rollup / build_judge_queue.
    let rollup_path = cli.root.join("scores.jsonl");
    let rollup = build_rollup(&cli.root);
    let rollup_lines = rollup.lines().count();
    if let Err(e) = std::fs::write(&rollup_path, rollup.as_bytes()) {
        eprintln!(
            "elmer_score: could not write roll-up {}: {e}",
            rollup_path.display()
        );
    } else {
        println!(
            "elmer_score: wrote {} ({rollup_lines} cells)",
            rollup_path.display()
        );
    }

    let queue_path = cli.root.join("judge-queue.jsonl");
    let queue = build_judge_queue(&cli.root);
    let queue_lines = queue.lines().count();
    if let Err(e) = std::fs::write(&queue_path, queue.as_bytes()) {
        eprintln!(
            "elmer_score: could not write judge queue {}: {e}",
            queue_path.display()
        );
    } else {
        println!(
            "elmer_score: wrote {} ({queue_lines} cells to judge)",
            queue_path.display()
        );
    }

    println!("elmer_score: {scored} scored, {skipped} already-scored (skipped), {failed} failed");
    if failed > 0 {
        return Err(format!("{failed} bundle(s) failed to score"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — pure logic + fixture bundles (no network, no app, no fake judge:
// the judge is the orchestrating agent now, outside this binary's process).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn entry(id: &str, classification: &str, no_routine_expected: bool) -> CorpusEntry {
        CorpusEntry {
            id: id.to_string(),
            prompt: format!("do the {id} thing"),
            predicates: vec!["a predicate".to_string()],
            classification: classification.to_string(),
            expected_gap: String::new(),
            judge_primary: false,
            no_routine_expected,
        }
    }

    // ── Deterministic verdict rubric ────────────────────────────────────────

    #[test]
    fn buildable_green_is_pass_missing_or_red_is_fail() {
        assert_eq!(deterministic_verdict("BUILDABLE", false, true, true), "pass");
        assert_eq!(deterministic_verdict("BUILDABLE", false, true, false), "fail");
        assert_eq!(deterministic_verdict("BUILDABLE", false, false, false), "fail");
    }

    #[test]
    fn no_routine_expected_is_na_never_fail() {
        // Even a BUILDABLE-labeled cell (never the real shape) with no routine
        // is n/a when no_routine_expected is set.
        assert_eq!(deterministic_verdict("NON-ROUTINE", true, false, false), "n/a");
        assert_eq!(deterministic_verdict("BUILDABLE", true, false, false), "n/a");
    }

    #[test]
    fn partial_gap_and_unclassified_are_inconclusive() {
        assert_eq!(
            deterministic_verdict("PARTIAL-GAP", false, true, true),
            "inconclusive"
        );
        // The 7 prescriptive prompts have no classification field.
        assert_eq!(deterministic_verdict("", false, true, true), "inconclusive");
        // NON-ROUTINE without the no_routine_expected flag (e.g. EU2) is judge-
        // primary, not a deterministic fail.
        assert_eq!(
            deterministic_verdict("NON-ROUTINE", false, false, false),
            "inconclusive"
        );
    }

    // ── validates_green over the real finding shape ─────────────────────────

    #[test]
    fn validates_green_reads_the_findings_array() {
        let clean = json!({ "r": [] });
        assert!(validates_green(&clean, "r"));

        let warned = json!({ "r": [ { "severity": "warning", "code": "X" } ] });
        assert!(validates_green(&warned, "r"), "warnings do not block");

        let errored = json!({ "r": [ { "severity": "error", "code": "Y" } ] });
        assert!(!validates_green(&errored, "r"), "an error-severity finding is not green");

        // A validation-failure object, a missing entry, and a non-array are not green.
        let failed = json!({ "r": { "error": "validate_routine blew up" } });
        assert!(!validates_green(&failed, "r"));
        assert!(!validates_green(&json!({}), "r"));
    }

    // ── Fixture bundle: BUILDABLE, green → deterministic pass + judge_input ──

    #[test]
    fn buildable_green_bundle_scores_deterministic_pass_and_emits_judge_input() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("sweep/model-x/A2");
        write(
            &bundle.join("outcome.json"),
            r#"{ "outcome": "completed", "arm": "base", "workflow_run": null }"#,
        );
        write(&bundle.join("validate.json"), r#"{ "send-outbound": [] }"#);
        write(
            &bundle.join("routines/send-outbound.json"),
            r#"{ "routine": "send-outbound", "schema_version": 1, "tracks": [] }"#,
        );
        // enabled.json must be ignored as a def.
        write(&bundle.join("routines/enabled.json"), r#"["send-outbound"]"#);

        let e = entry("A2", "BUILDABLE", false);
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
        })
        .expect("scoring must succeed");

        assert_eq!(score["deterministic"]["verdict"], "pass");
        assert_eq!(score["deterministic"]["routine_saved"], true);
        assert_eq!(score["deterministic"]["validates_green"], true);
        assert_eq!(score["deterministic"]["saved_routine"], "send-outbound");
        assert_eq!(score["corpus_id"], "A2");
        assert_eq!(score["arm"], "base");

        // No network judge call was made — judge is a placeholder for the agent.
        assert_eq!(score["judge"], Value::Null);

        // judge_input is a well-formed, self-contained package for the agent.
        let ji = &score["judge_input"];
        assert_eq!(ji["corpus_id"], "A2");
        assert_eq!(ji["arm"], "base");
        assert_eq!(ji["classification"], "BUILDABLE");
        assert_eq!(ji["no_routine_expected"], false);
        assert_eq!(ji["prompt"], "do the A2 thing");
        assert_eq!(ji["predicates"], json!(["a predicate"]));
        assert_eq!(ji["deterministic"], "pass");
        let defs = ji["artifacts"]["def"].as_array().expect("def is an array");
        assert_eq!(defs.len(), 1, "enabled.json must not appear as a def");
        assert_eq!(defs[0]["routine"], "send-outbound");
        assert!(ji["artifacts"]["workflow_summary"]["note"]
            .as_str()
            .unwrap()
            .contains("base/matched-control"));
    }

    // ── Fixture bundle: no_routine_expected, no routine → deterministic n/a ──

    #[test]
    fn no_routine_expected_bundle_scores_na_with_empty_def_list() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("sweep/model-x/EU3");
        write(
            &bundle.join("outcome.json"),
            r#"{ "outcome": "completed", "arm": "base", "workflow_run": null }"#,
        );
        // No validate.json, no routines/ — a pure-diagnosis cell.

        let e = entry("EU3", "NON-ROUTINE", true);
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
        })
        .expect("scoring must succeed");

        assert_eq!(score["deterministic"]["verdict"], "n/a");
        assert_eq!(score["deterministic"]["routine_saved"], false);
        assert_eq!(score["deterministic"]["saved_routine"], Value::Null);
        assert_eq!(score["judge"], Value::Null);

        let ji = &score["judge_input"];
        assert_eq!(ji["no_routine_expected"], true);
        assert_eq!(ji["deterministic"], "n/a");
        assert_eq!(
            ji["artifacts"]["def"].as_array().expect("def is an array").len(),
            0
        );
    }

    // ── Full-arm bundle: saved routine + stopped_reason from workflow_run ────

    #[test]
    fn full_arm_judge_input_carries_workflow_summary_and_parsed_def() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("sweep/model-x/S1");
        // camelCase nested keys, exactly as WorkflowRun serializes them.
        write(
            &bundle.join("outcome.json"),
            r#"{
              "outcome": "invalid_action",
              "arm": "full",
              "workflow_run": {
                "depth": "full",
                "phasesRun": [ { "name": "emit", "promptTokens": 42, "outcome": "saved" } ],
                "savedRoutine": "dial-fallback",
                "present": null,
                "stoppedReason": "routine CI red for \"dial-fallback\""
              }
            }"#,
        );
        write(
            &bundle.join("validate.json"),
            r#"{ "dial-fallback": [ { "severity": "error", "code": "UNRESOLVED_REF", "routine": "dial-fallback", "message": "boom" } ] }"#,
        );
        write(
            &bundle.join("routines/dial-fallback.json"),
            r#"{ "routine": "dial-fallback", "schema_version": 1, "tracks": [] }"#,
        );

        let e = entry("S1", "", false); // prescriptive prompt: unclassified
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
        })
        .expect("scoring must succeed");

        assert_eq!(score["deterministic"]["routine_saved"], true);
        assert_eq!(score["deterministic"]["saved_routine"], "dial-fallback");
        assert_eq!(score["deterministic"]["honest_stop"], true);
        // Error-severity finding → not green.
        assert_eq!(score["deterministic"]["validates_green"], false);
        // Unclassified prescriptive prompt → inconclusive (judge primary).
        assert_eq!(score["deterministic"]["verdict"], "inconclusive");
        assert_eq!(score["arm"], "full");

        let ji = &score["judge_input"];
        assert_eq!(ji["deterministic"], "inconclusive");
        let wf = &ji["artifacts"]["workflow_summary"];
        assert_eq!(wf["depth"], "full");
        assert_eq!(wf["saved_routine"], "dial-fallback");
        assert_eq!(wf["stopped_reason"], "routine CI red for \"dial-fallback\"");
        assert_eq!(wf["phases"][0]["name"], "emit");
        assert_eq!(wf["phases"][0]["outcome"], "saved");
        let defs = ji["artifacts"]["def"].as_array().expect("def is an array");
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0]["routine"], "dial-fallback");
    }

    // ── Bundle discovery + filters ──────────────────────────────────────────

    #[test]
    fn collect_bundles_finds_leaf_dirs_with_outcome_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("sweep/m1/P2/outcome.json"), "{}");
        write(&root.join("sweep/m1/P1/outcome.json"), "{}");
        write(&root.join("sweep/m2/P2/outcome.json"), "{}");
        // A stray non-bundle dir must be ignored.
        std::fs::create_dir_all(root.join("sweep/m2/empty")).unwrap();

        let mut found = Vec::new();
        collect_bundles(root, &mut found);
        assert_eq!(found.len(), 3, "three bundles, deterministic order");
        assert!(found.iter().all(|b| b.join("outcome.json").is_file()));

        // --only by leaf name.
        assert!(bundle_matches_only(&root.join("sweep/m1/P2"), "P2"));
        // --only by path suffix.
        assert!(bundle_matches_only(&root.join("sweep/m1/P2"), "m1/P2"));
        assert!(!bundle_matches_only(&root.join("sweep/m1/P2"), "P1"));
    }

    #[test]
    fn resolve_prompt_id_prefers_manifest_then_leaf() {
        let m = RunManifest {
            model: Some("openai/gpt-5.5".to_string()),
            prompt: Some(ManifestPrompt { id: "P2".to_string() }),
        };
        let b = Path::new("/x/sweep/model/ZZ");
        assert_eq!(resolve_prompt_id(b, &m).as_deref(), Some("P2"));
        assert_eq!(resolve_model(b, &m), "openai/gpt-5.5");

        // Empty manifest → fall back to path components.
        let empty = RunManifest::default();
        assert_eq!(resolve_prompt_id(b, &empty).as_deref(), Some("ZZ"));
        assert_eq!(resolve_model(b, &empty), "model");
    }

    // ── Roll-ups are UNFILTERED — every bundle under root, every time ───────

    #[test]
    fn rollup_and_queue_scan_every_bundle_under_root_not_just_a_filtered_subset() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (leaf, corpus_id) in [("m1/P1", "P1"), ("m1/P2", "P2"), ("m2/P1", "P1")] {
            write(&root.join(format!("sweep/{leaf}/outcome.json")), "{}");
            write(
                &root.join(format!("sweep/{leaf}/score.json")),
                &format!(
                    r#"{{"corpus_id":"{corpus_id}","judge_input":{{"corpus_id":"{corpus_id}"}}}}"#
                ),
            );
        }

        // Simulates a targeted `--only m1/P1 --redo` scoring pass: the
        // roll-ups must still cover all three bundles, not just the one that
        // was just (re-)scored.
        let rollup = build_rollup(root);
        assert_eq!(
            rollup.lines().count(),
            3,
            "roll-up must reflect every score.json under root, unfiltered"
        );

        let queue = build_judge_queue(root);
        assert_eq!(
            queue.lines().count(),
            3,
            "judge queue must reflect every judge_input under root, unfiltered"
        );
        assert!(queue.lines().all(|l| l.contains("\"corpus_id\"")));
    }

    #[test]
    fn rollup_and_queue_degrade_gracefully_for_missing_score_or_judge_input() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Bundle with no score.json yet — must not crash, simply absent.
        write(&root.join("sweep/m1/NOSCORE/outcome.json"), "{}");
        // Bundle with score.json but no judge_input field (e.g. hand-authored
        // or from a pre-refactor run) — present in the roll-up, absent from
        // the judge queue.
        write(&root.join("sweep/m1/NOJUDGEINPUT/outcome.json"), "{}");
        write(
            &root.join("sweep/m1/NOJUDGEINPUT/score.json"),
            r#"{"corpus_id":"X"}"#,
        );

        assert_eq!(build_rollup(root).lines().count(), 1);
        assert_eq!(build_judge_queue(root).lines().count(), 0);
    }

    // ── CLI parsing ─────────────────────────────────────────────────────────

    #[test]
    fn cli_parses_required_and_flags() {
        let args: Vec<String> = [
            "--root", "battery-results",
            "--corpus", "tests/battery/corpus.json",
            "--only", "P2",
            "--redo",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let cli = parse_cli(&args).expect("full args must parse");
        assert_eq!(cli.root, PathBuf::from("battery-results"));
        assert_eq!(cli.corpus, PathBuf::from("tests/battery/corpus.json"));
        assert_eq!(cli.only.as_deref(), Some("P2"));
        assert!(cli.redo);

        assert!(parse_cli(&["--root".to_string(), "x".to_string()]).is_err(), "missing required fails");
        assert!(parse_cli(&["--bogus".to_string()]).is_err(), "unknown flag fails");
    }
}
