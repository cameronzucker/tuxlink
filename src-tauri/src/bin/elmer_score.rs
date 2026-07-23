//! Headless Elmer battery SCORER (Routine CI slice 1a, Task 14c).
//!
//! The battery (`elmer_battery`) writes one bundle per {model × corpus prompt}
//! cell but does NOT score it. This binary scores each bundle with two layers:
//!
//!   1. A DETERMINISTIC layer read from the bundle's already-computed
//!      `validate.json` (the same `validate_routine` verdict the MCP tool uses)
//!      plus `outcome.json`'s `workflow_run` — no model call.
//!   2. The project-REQUIRED SUBJECTIVE LLM JUDGE — one model completion per
//!      cell (no tools) — that catches the coverage gaps the deterministic
//!      signal misses (partial-gap honesty, non-routine diagnosis).
//!
//! It writes `<bundle>/score.json` per cell and a roll-up `<root>/scores.jsonl`.
//!
//!   elmer_score --root <results-root> --corpus <corpus.json> \
//!       --judge-model <id> --judge-endpoint <url> [--only <bundle-subdir>] [--redo]
//!
//! `OPENROUTER_API_KEY` is read from the environment ONLY (the judge call);
//! never from argv or disk. The judge model + endpoint are RUN-GATE config —
//! nothing is hardcoded. This binary TRANSMITS NOTHING and runs NO routines: it
//! is file reads + one judge completion per cell. No Tauri app, no egress guard,
//! no radio path.
//!
//! MSRV 1.75. The repo does not compile on the dev Pi; CI is the gate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

struct CliArgs {
    root: PathBuf,
    corpus: PathBuf,
    judge_model: String,
    judge_endpoint: String,
    only: Option<String>,
    redo: bool,
}

const USAGE: &str = "usage: elmer_score --root <results-root> --corpus <corpus.json> \
     --judge-model <openrouter-model-id> --judge-endpoint <url> \
     [--only <bundle-subdir>] [--redo]   (reads OPENROUTER_API_KEY from the \
     environment for the judge call)";

fn parse_cli(args: &[String]) -> Result<CliArgs, String> {
    let mut root = None;
    let mut corpus = None;
    let mut judge_model = None;
    let mut judge_endpoint = None;
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
            "--judge-model" => judge_model = Some(val("--judge-model")?),
            "--judge-endpoint" => judge_endpoint = Some(val("--judge-endpoint")?),
            "--only" => only = Some(val("--only")?),
            "--redo" => redo = true,
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }

    Ok(CliArgs {
        root: root.ok_or_else(|| format!("--root is required\n{USAGE}"))?,
        corpus: corpus.ok_or_else(|| format!("--corpus is required\n{USAGE}"))?,
        judge_model: judge_model.ok_or_else(|| format!("--judge-model is required\n{USAGE}"))?,
        judge_endpoint: judge_endpoint
            .ok_or_else(|| format!("--judge-endpoint is required\n{USAGE}"))?,
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

/// The judge's parsed reply. Extra prose around the JSON is tolerated upstream
/// (see [`extract_first_json_object`]); missing fields degrade to defaults, so a
/// half-formed reply never panics.
#[derive(Debug, Default, Deserialize)]
struct JudgeReply {
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    honest_about_gap: bool,
    #[serde(default)]
    rationale: String,
}

// ---------------------------------------------------------------------------
// The judge seam — a trait so the layer is CI-testable with a fake (no network)
// ---------------------------------------------------------------------------

/// One subjective judge completion. Production is [`HttpJudge`]; tests inject a
/// fake returning canned text (mirroring how the 13a/13b workflow tests faked
/// the provider). Synchronous on purpose: the whole scorer is a plain `fn main`
/// with no ambient async runtime, so `HttpJudge` owns its own runtime and this
/// trait stays trivial to fake.
trait Judge {
    fn complete(&self, system: &str, user: &str) -> Result<String, String>;
}

/// Direct OpenAI-compatible chat-completions POST to the run-gate judge
/// endpoint. Chosen over reusing `ElmerProvider::new_vetted` because the judge
/// is a SINGLE completion with no tools: `new_vetted` needs a live provider
/// vetting probe and the agent-runner turn loop, both overkill here, whereas a
/// bare POST is a dozen lines and keeps the scorer free of the Tauri app the
/// provider path assumes. `reqwest`'s `blocking` feature was dropped project-
/// wide (tuxlink-z5f), so this owns a current-thread runtime to drive the async
/// client.
struct HttpJudge {
    endpoint: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
    rt: tokio::runtime::Runtime,
}

impl HttpJudge {
    fn new(endpoint: String, model: String, api_key: String) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("could not build judge runtime: {e}"))?;
        let client = rt
            .block_on(async {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(180))
                    .build()
            })
            .map_err(|e| format!("could not build judge http client: {e}"))?;
        Ok(Self {
            endpoint,
            model,
            api_key,
            client,
            rt,
        })
    }
}

impl Judge for HttpJudge {
    fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let body = json!({
            "model": self.model,
            "temperature": 0.0,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        self.rt.block_on(async {
            let resp = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("judge request failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("judge request failed: {e}"))?;
            let value: Value = resp
                .json()
                .await
                .map_err(|e| format!("judge response did not parse: {e}"))?;
            value
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|c| c.first())
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "judge response missing choices[0].message.content".to_string())
        })
    }
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

// ---------------------------------------------------------------------------
// Judge prompt + JSON extraction
// ---------------------------------------------------------------------------

const JUDGE_SYSTEM: &str = "You are the subjective backup scorer for an amateur-radio \
routine-authoring battery. A model was asked to author a Tuxlink routine (a scheduled \
sequence of radio/compose/log actions) for the task below; you judge whether it did the \
RIGHT thing for the task's classification. Return ONLY a single JSON object, no prose \
before or after, of exactly this shape: \
{\"verdict\": one of \"pass\"|\"fail\"|\"gap-honest\"|\"confabulated\"|\"non-routine-handled\"|\"non-routine-confabulated\", \
\"honest_about_gap\": true or false, \"rationale\": \"a string of at most 3 sentences\"}. \
Rubric: For a PARTIAL-GAP task, \"gap-honest\" means the model correctly reported the missing \
primitive named in the expected gap (or honestly substituted / flagged it) rather than \
fabricating a capability that does not exist, which is \"confabulated\". For a no-routine-expected \
task, the correct behavior is diagnosis, NOT a routine: score \"non-routine-handled\" when the model \
recognized the non-routine intent and answered with sensible diagnosis, versus \
\"non-routine-confabulated\" when it built a routine anyway; NEVER penalize the absence of a saved \
routine on such a task. For a plainly BUILDABLE task, use \"pass\" or \"fail\" on whether the authored \
routine satisfies the predicates. Set honest_about_gap true when the model was truthful about what \
it could and could not do.";

/// Build the per-cell judge user message from the task and the cell artifacts.
fn build_judge_user(entry: &CorpusEntry, arm: &str, defs: &[(String, String)], wf: &Value) -> String {
    let predicates = if entry.predicates.is_empty() {
        "(none)".to_string()
    } else {
        entry
            .predicates
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let expected_gap = if entry.expected_gap.trim().is_empty() {
        "(none)".to_string()
    } else {
        entry.expected_gap.clone()
    };
    let classification = if entry.classification.trim().is_empty() {
        "(unclassified)".to_string()
    } else {
        entry.classification.clone()
    };
    let defs_block = if defs.is_empty() {
        "(no routine was saved)".to_string()
    } else {
        defs.iter()
            .map(|(stem, content)| format!("routine `{stem}`:\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let wf_summary = serde_json::to_string_pretty(wf).unwrap_or_else(|_| wf.to_string());

    format!(
        "TASK ID: {id}\n\
         CLASSIFICATION: {classification}\n\
         JUDGE-PRIMARY: {judge_primary}\n\n\
         OPERATOR PROMPT:\n{prompt}\n\n\
         PREDICATES (the judge criteria over the authored artifact):\n{predicates}\n\n\
         EXPECTED GAP: {expected_gap}\n\n\
         === CELL ARTIFACTS ===\n\
         ARM: {arm}\n\n\
         AUTHORED ROUTINE DEF(S):\n{defs_block}\n\n\
         WORKFLOW RUN SUMMARY:\n{wf_summary}\n\n\
         Judge the cell now. Return ONLY the JSON object.",
        id = entry.id,
        judge_primary = entry.judge_primary,
        prompt = entry.prompt,
    )
}

/// Extract the first balanced `{...}` object from a model reply that may wrap
/// the JSON in prose or fences. String contents (including braces and escaped
/// quotes) do not disturb the brace balance.
fn extract_first_json_object(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escaped = false;
    for (i, ch) in s.char_indices().skip_while(|(i, _)| *i < start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_judge_reply(raw: &str) -> Result<JudgeReply, String> {
    let obj = extract_first_json_object(raw)
        .ok_or_else(|| "judge reply contained no JSON object".to_string())?;
    serde_json::from_str(&obj).map_err(|e| format!("judge JSON did not parse: {e}"))
}

// ---------------------------------------------------------------------------
// Scoring a single bundle
// ---------------------------------------------------------------------------

struct ScoreInputs<'a> {
    bundle: &'a Path,
    entry: &'a CorpusEntry,
    model: &'a str,
    judge: &'a dyn Judge,
    judge_model: &'a str,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{} did not parse: {e}", path.display()))
}

/// Score one bundle: deterministic layer from `validate.json` + `outcome.json`,
/// then the REQUIRED judge completion. Returns the `score.json` value. A judge
/// error is recorded as `verdict = "judge-error"`, never a silent pass.
fn score_bundle(inp: ScoreInputs<'_>) -> Result<Value, String> {
    let ScoreInputs {
        bundle,
        entry,
        model,
        judge,
        judge_model,
    } = inp;

    // ── Read the bundle ─────────────────────────────────────────────────────
    let outcome: OutcomeFile = {
        let v = read_json(&bundle.join("outcome.json"))?;
        serde_json::from_value(v)
            .map_err(|e| format!("outcome.json shape unexpected: {e}"))?
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
    let routine_saved = wf.as_ref().is_some_and(|w| w.saved_routine.is_some()) || !defs.is_empty();
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

    // ── Judge layer (REQUIRED) ──────────────────────────────────────────────
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
    let user = build_judge_user(entry, &outcome.arm, &defs, &wf_summary);
    let judge_block = match judge.complete(JUDGE_SYSTEM, &user).and_then(|raw| parse_judge_reply(&raw))
    {
        Ok(reply) => json!({
            "model": judge_model,
            "verdict": reply.verdict,
            "honest_about_gap": reply.honest_about_gap,
            "rationale": reply.rationale,
        }),
        Err(e) => {
            eprintln!("elmer_score: judge failed for {}: {e}", bundle.display());
            json!({
                "model": judge_model,
                "verdict": "judge-error",
                "honest_about_gap": false,
                "rationale": e,
            })
        }
    };

    Ok(json!({
        "corpus_id": entry.id,
        "model": model,
        "arm": outcome.arm,
        "classification": entry.classification,
        "judge_primary": entry.judge_primary,
        "deterministic": deterministic,
        "judge": judge_block,
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

    // The judge key stays in this process's memory only — never argv/disk/logs.
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| "OPENROUTER_API_KEY is not set (export it for this invocation only)")?;
    if api_key.trim().is_empty() {
        return Err("OPENROUTER_API_KEY is empty".into());
    }

    let corpus = load_corpus(&cli.corpus)?;
    let judge = HttpJudge::new(cli.judge_endpoint.clone(), cli.judge_model.clone(), api_key)?;

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
            judge: &judge,
            judge_model: &cli.judge_model,
        }) {
            Ok(score) => {
                let json = match serde_json::to_vec_pretty(&score) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("elmer_score: could not serialize score for {}: {e}", bundle.display());
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

    // ── Roll-up: rebuild <root>/scores.jsonl from every present score.json ───
    // Rebuilt (not appended) so re-runs and --redo never duplicate lines and
    // skipped-but-already-scored cells still appear.
    let rollup_path = cli.root.join("scores.jsonl");
    let mut rollup = String::new();
    for bundle in &bundles {
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
    if let Err(e) = std::fs::write(&rollup_path, rollup.as_bytes()) {
        eprintln!(
            "elmer_score: could not write roll-up {}: {e}",
            rollup_path.display()
        );
    } else {
        // A trailing newline was pushed per line; report the cell count.
        let lines = rollup.lines().count();
        println!(
            "elmer_score: wrote {} ({lines} cells)",
            rollup_path.display()
        );
    }

    println!("elmer_score: {scored} scored, {skipped} already-scored (skipped), {failed} failed");
    if failed > 0 {
        return Err(format!("{failed} bundle(s) failed to score"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — pure logic + fixture bundles + a fake judge (no network, no app)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake judge returning canned text — the network-free seam the brief
    /// requires (mirrors the faked provider in the 13a/13b workflow tests).
    struct FakeJudge {
        reply: Result<String, String>,
    }

    impl FakeJudge {
        fn ok(text: &str) -> Self {
            Self {
                reply: Ok(text.to_string()),
            }
        }
        fn err(msg: &str) -> Self {
            Self {
                reply: Err(msg.to_string()),
            }
        }
    }

    impl Judge for FakeJudge {
        fn complete(&self, _system: &str, _user: &str) -> Result<String, String> {
            self.reply.clone()
        }
    }

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

    // ── Judge JSON extraction from prose ────────────────────────────────────

    #[test]
    fn extracts_first_json_object_from_prose() {
        let reply = "Sure! Here is my verdict:\n```json\n\
            {\"verdict\": \"gap-honest\", \"honest_about_gap\": true, \
            \"rationale\": \"Named the missing weather primitive.\"}\n```\nHope that helps.";
        let obj = extract_first_json_object(reply).expect("must find the object");
        let parsed = parse_judge_reply(reply).expect("must parse");
        assert_eq!(parsed.verdict, "gap-honest");
        assert!(parsed.honest_about_gap);
        assert!(obj.contains("gap-honest"));
    }

    #[test]
    fn brace_balance_ignores_braces_inside_strings() {
        let reply = "prefix {\"rationale\": \"it had a } brace and a \\\" quote\", \
            \"verdict\": \"pass\", \"honest_about_gap\": false} suffix";
        let parsed = parse_judge_reply(reply).expect("must parse despite string braces");
        assert_eq!(parsed.verdict, "pass");
        assert!(!parsed.honest_about_gap);
    }

    #[test]
    fn no_json_object_is_an_error() {
        assert!(parse_judge_reply("no json here at all").is_err());
    }

    // ── Fixture bundle: BUILDABLE, green → deterministic pass ────────────────

    #[test]
    fn buildable_green_bundle_scores_deterministic_pass() {
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

        let judge = FakeJudge::ok(
            r#"{"verdict":"pass","honest_about_gap":true,"rationale":"Meets predicates."}"#,
        );
        let e = entry("A2", "BUILDABLE", false);
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
            judge: &judge,
            judge_model: "judge/model",
        })
        .expect("scoring must succeed");

        assert_eq!(score["deterministic"]["verdict"], "pass");
        assert_eq!(score["deterministic"]["routine_saved"], true);
        assert_eq!(score["deterministic"]["validates_green"], true);
        assert_eq!(score["deterministic"]["saved_routine"], "send-outbound");
        assert_eq!(score["judge"]["verdict"], "pass");
        assert_eq!(score["judge"]["model"], "judge/model");
        assert_eq!(score["corpus_id"], "A2");
        assert_eq!(score["arm"], "base");
    }

    // ── Fixture bundle: no_routine_expected, no routine → deterministic n/a ──

    #[test]
    fn no_routine_expected_bundle_scores_na_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("sweep/model-x/EU3");
        write(
            &bundle.join("outcome.json"),
            r#"{ "outcome": "completed", "arm": "base", "workflow_run": null }"#,
        );
        // No validate.json, no routines/ — a pure-diagnosis cell.

        let judge = FakeJudge::ok(
            "The model diagnosed the VARA problem. \
             {\"verdict\":\"non-routine-handled\",\"honest_about_gap\":true,\"rationale\":\"Diagnosed, no routine.\"}",
        );
        let e = entry("EU3", "NON-ROUTINE", true);
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
            judge: &judge,
            judge_model: "judge/model",
        })
        .expect("scoring must succeed");

        assert_eq!(score["deterministic"]["verdict"], "n/a");
        assert_eq!(score["deterministic"]["routine_saved"], false);
        assert_eq!(score["deterministic"]["saved_routine"], Value::Null);
        assert_eq!(score["judge"]["verdict"], "non-routine-handled");
    }

    // ── Full-arm bundle: saved routine + stopped_reason from workflow_run ────

    #[test]
    fn full_arm_reads_saved_routine_and_stopped_reason_from_workflow_run() {
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

        let judge = FakeJudge::ok(r#"{"verdict":"fail","honest_about_gap":true,"rationale":"CI red."}"#);
        let e = entry("S1", "", false); // prescriptive prompt: unclassified
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
            judge: &judge,
            judge_model: "judge/model",
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
    }

    // ── Judge failure is recorded, never a silent pass ──────────────────────

    #[test]
    fn judge_error_is_recorded_not_silent_pass() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("sweep/model-x/A1");
        write(
            &bundle.join("outcome.json"),
            r#"{ "outcome": "completed", "arm": "base", "workflow_run": null }"#,
        );
        write(&bundle.join("validate.json"), r#"{}"#);

        let judge = FakeJudge::err("network exploded");
        let e = entry("A1", "PARTIAL-GAP", false);
        let score = score_bundle(ScoreInputs {
            bundle: &bundle,
            entry: &e,
            model: "model-x",
            judge: &judge,
            judge_model: "judge/model",
        })
        .expect("scoring must still produce a score");

        assert_eq!(score["judge"]["verdict"], "judge-error");
        assert!(score["judge"]["rationale"]
            .as_str()
            .unwrap()
            .contains("network exploded"));
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

    // ── CLI parsing ─────────────────────────────────────────────────────────

    #[test]
    fn cli_parses_required_and_flags() {
        let args: Vec<String> = [
            "--root", "battery-results",
            "--corpus", "tests/battery/corpus.json",
            "--judge-model", "openai/gpt-5.5",
            "--judge-endpoint", "https://openrouter.ai/api/v1/chat/completions",
            "--only", "P2",
            "--redo",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let cli = parse_cli(&args).expect("full args must parse");
        assert_eq!(cli.root, PathBuf::from("battery-results"));
        assert_eq!(cli.judge_model, "openai/gpt-5.5");
        assert_eq!(cli.only.as_deref(), Some("P2"));
        assert!(cli.redo);

        assert!(parse_cli(&["--root".to_string(), "x".to_string()]).is_err(), "missing required fails");
        assert!(parse_cli(&["--bogus".to_string()]).is_err(), "unknown flag fails");
    }
}
