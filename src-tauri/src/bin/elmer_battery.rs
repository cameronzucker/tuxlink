//! Headless Elmer battery runner (bd tuxlink-hwgdi).
//!
//! Runs ONE battery cell — {model × corpus prompt} — against a REAL Elmer
//! session: real provider (OpenRouter by default), real in-process MCP invoker
//! over the real tool router, real transcript sink, real routines engine — all
//! over a windowless `tauri::App<Wry>` bound to an isolated scratch profile.
//! The orchestrating agent loops cells externally and judges the bundles; this
//! binary stays simple and resumable (one cell per invocation).
//!
//!   elmer_battery --corpus tests/battery/corpus.json --model <openrouter-id> \
//!       --prompt P2 --out battery-results/<sweep>/<model>/P2 \
//!       [--endpoint <url>] [--turn-cap N] [--cell-ceiling-usd X] \
//!       [--temperature T] [--ledger <path>] [--turn-timeout-secs N]
//!
//! `OPENROUTER_API_KEY` is read from the environment, held in memory, and never
//! written to disk or logs (no-disk-creds). On a headless box run under
//! `xvfb-run` — gtk init is the only display dependency; no window is ever
//! created (`Builder::build()` only; windows are created by `App::run`'s setup,
//! which never executes here — asserted at runtime).
//!
//! Battery invariants (design: dev/scratch/elmer-battery-design.md, Codex adrev
//! dispositions 1-9, all adopted):
//!   - Scratch isolation: TUXLINK_CONFIG_DIR + XDG_* + HOME all point under a
//!     fresh temp root BEFORE app construction, with a resolved-path preflight.
//!   - The egress guard stays DISARMED for the whole cell (authoring needs no
//!     arm; fail-closed). No rearm() call exists in this binary.
//!   - PARITY (tuxlink-y9a6l): the FULL production tool surface passes through
//!     an observation-only wrapper; every call and every production-gate
//!     denial lands in tool_calls.jsonl. No harness allowlist exists.
//!   - Budget: cumulative ledger hard-stop at $45; per-cell ceiling + provider
//!     turn cap enforced by a watchdog that fires `cancel_and_abort`; OpenRouter
//!     credits before/after each cell are the hard spend record.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::Manager as _;
use tokio_util::sync::CancellationToken;

use tuxlink_agent_frontend::endpoint::AgentEndpoint;
use tuxlink_agent_frontend::ApiKey;
use tuxlink_agent_runner::{
    CallAuthority, RunOutcome, ToolCall, ToolInvoker, ToolOutcome, ToolSpec,
};
use tuxlink_lib::elmer::events::ElmerEvent;
use tuxlink_lib::elmer::executor::InProcessMcpInvoker;
use tuxlink_lib::elmer::keyring::{ElmerKeyring, EntryFactory};
use tuxlink_lib::elmer::model_config_state::ElmerModelConfigState;
use tuxlink_lib::elmer::provider::ElmerProvider;
use tuxlink_lib::elmer::session::{ElmerSession, EventSink};
use tuxlink_lib::elmer::transcript_sink::ElmerTranscriptSink;
use tuxlink_lib::elmer::workflow::build_affordance_catalog;
use tuxlink_lib::routines::commands::list_actions;
use tuxlink_lib::routines::session::RoutinesState;
use tuxlink_lib::winlink::credentials::EntryLike;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default OpenRouter OpenAI-compat chat endpoint.
const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Cumulative-ledger hard stop (buffer under the operator's $50 cap). The
/// harness REFUSES to start a cell at or above this; the orchestrator
/// escalates to the operator.
const LEDGER_HARD_STOP_USD: f64 = 45.0;

/// Per-cell spend ceiling default (USD, token-estimate × pricing — advisory
/// meter; the credits delta is the hard record).
const DEFAULT_CELL_CEILING_USD: f64 = 2.0;

/// Per-cell provider-turn cap default.
const DEFAULT_TURN_CAP: u64 = 40;

/// Per-provider-turn wall-clock timeout default (seconds). The agent-runner
/// default (120s) is too short for reasoning-heavy cloud models.
const DEFAULT_TURN_TIMEOUT_SECS: u32 = 600;

/// Pinned inference temperature default (adrev disposition 9: explicit, all
/// models).
const DEFAULT_TEMPERATURE: f32 = 0.2;

/// Belt over the turn cap: some paths emit no ContextUsage event (timeouts,
/// provider errors), so the watchdog also trips on raw tool-invocation count.
const TOOL_CALL_CAP_FACTOR: u64 = 4;

/// Truncation bound for the per-call result preview in tool_calls.jsonl (the
/// full results live in the durable transcript; this file is the judge's
/// call-sequence record).
const RESULT_PREVIEW_CHARS: usize = 4000;

/// PARITY HARNESS (tuxlink-y9a6l, operator ruling 2026-07-27): the battery
/// presents the FULL production tool surface with production semantics. There
/// is no harness allowlist and no synthetic denial teaching — both were
/// removed after the operator ruled that a non-production tool environment
/// invalidates the measurement outright ("junk science"): every trajectory
/// that touched a synthetic denial experienced an environment production
/// Elmer never presents, and the old DENY_TEACHING text rewrote the model's
/// goal ("the session's ONLY deliverable is a SAVED routine") in every cell
/// including EU3, whose correct deliverable is no routine at all.
///
/// The safety boundary is the ENVIRONMENT, exactly as in production:
/// - Egress/compose tools succeed and STAGE; the flush is approval-gated and
///   no approval token is ever granted in a battery cell (a production-real
///   pending state). The EgressGuard is never armed.
/// - The scratch station has no transports configured and no rig attached;
///   connect/transmit paths fail with the same errors a fresh production
///   install produces.
/// - Real consent/authority gates return [`ToolOutcome::Denied`], which the
///   agent-runner treats as terminal (denial_final) — in production that ends
///   the response at the consent boundary, and under parity it ends the cell
///   the same way. That is recorded behavior, not a harness artifact.
///
/// KNOWN PARITY RESIDUES (recorded in every run manifest; see
/// `HARNESS_PARITY_RESIDUES`): the propagation engine is Unavailable (voacapl
/// sidecar not staged next to this binary — a fully-set-up production station
/// would answer predict_path), and the routine scheduler is not spawned
/// (enable parks a routine; nothing fires mid-cell). Both are honest runtime
/// states a real station can exhibit, but they differ from a fully-provisioned
/// station and the asterisk stays attached to the data.
const HARNESS_PARITY_RESIDUES: &[&str] = &[
    "propagation engine Unavailable (voacapl sidecar not staged)",
    "routine scheduler not spawned (enable parks; nothing fires mid-cell)",
];

/// The residues actually in force for this run. The propagation entry drops
/// out when the engine is env-staged (tuxlink-hbu3b): setup() fails the launch
/// on a half-staged env, so env-present here implies staged-and-validated.
/// A static list would mislabel every staged run (Codex 2026-07-28 P2).
fn harness_parity_residues() -> Vec<&'static str> {
    let staged = std::env::var_os("TUXLINK_VOACAPL_BIN").is_some()
        && std::env::var_os("TUXLINK_ITSHFBC_DIR").is_some();
    HARNESS_PARITY_RESIDUES
        .iter()
        .copied()
        .filter(|r| !(staged && r.starts_with("propagation engine")))
        .collect()
}

/// Scratch-profile config.json (schema v9): manual grid DM33, GPS off, NO
/// transports configured, offline (connect_to_cms false). Everything else
/// takes its serde default.
const SCRATCH_CONFIG_JSON: &str = r#"{
  "schema_version": 9,
  "wizard_completed": true,
  "connect": { "connect_to_cms": false, "transport": "Telnet" },
  "identity": { "callsign": null, "identifier": "BATTERY", "grid": "DM33" },
  "privacy": {
    "gps_state": "Off",
    "position_precision": "FourCharGrid",
    "position_source": "Manual"
  }
}"#;

/// S2 preseed: the `nearest-40m-dial` reference def (Sonnet 5 transcript
/// 1784665223064-2 shape — radio.connect walk over the 5 nearest 40m stations,
/// 30m schedule, winner logged from the connect step's `station` output).
/// Kept attended (an automatic def without an operator ack would carry an
/// AUTO_TX_UNACKED finding into every S2 judgment). File stem MUST equal the
/// body's `routine` (DefinitionStore contract).
const PRESEED_NAME: &str = "nearest-40m-dial";
const PRESEED_NEAREST_40M_DIAL: &str = r#"{
  "routine": "nearest-40m-dial",
  "schema_version": 1,
  "transmit_mode": "attended",
  "on_interrupted": "stay",
  "inputs": [],
  "triggers": [
    { "type": "schedule", "every": "30m" }
  ],
  "tracks": [
    {
      "name": "main",
      "steps": [
        {
          "id": "s1",
          "action": "radio.connect",
          "params": {
            "stations": ["W7YAP-10", "K7RVM-10", "W7QO-10", "KF7RSF-10", "K7KMQ-10"],
            "bands": ["40m"]
          },
          "timeout_s": 600
        },
        {
          "id": "s3",
          "action": "local.log",
          "params": { "message": "$s1.station" }
        }
      ]
    }
  ]
}"#;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/// The experiment condition this cell runs under. Both arms are a single
/// `ElmerSession::send` on the production agent loop; `MatchedControl` prepends
/// the deterministic affordance catalog. (The discarded `Full` workflow-engine
/// arm was removed with the engine — bd tuxlink-t3jci.) `Skill` is the
/// redesign's Build Carefully arm: identical to `Base` except the routine
/// invariant + authoring skill are composed into the system prompt via the
/// single shared `compose_system_prompt` seam, so Base-vs-Skill differs by
/// *exactly* the skill (the confound-free lift measurement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Arm {
    /// A single `ElmerSession::send` over the operator's raw prompt — the
    /// production agent-loop reference (authoring OFF).
    Base,
    /// Base's single send, but the prompt is augmented with the deterministic
    /// affordance catalog — holds the model, tools, and budget constant with
    /// Base while isolating the affordance-catalog effect (authoring OFF).
    MatchedControl,
    /// Base's single send with authoring ON — the Build Carefully skill is
    /// composed into the system prompt. The measured `+Skill` lift arm.
    Skill,
}

impl std::str::FromStr for Arm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "base" => Ok(Arm::Base),
            "matched-control" => Ok(Arm::MatchedControl),
            "skill" => Ok(Arm::Skill),
            other => Err(format!(
                "unknown --arm {other:?} (expected base | matched-control | skill)"
            )),
        }
    }
}

impl Arm {
    /// Every supported arm, for `--list-arms` (the launcher preflight guard).
    const ALL: [Arm; 3] = [Arm::Base, Arm::MatchedControl, Arm::Skill];

    /// The kebab-case wire name (mirrors `FromStr` + the `Serialize` rename).
    fn as_str(self) -> &'static str {
        match self {
            Arm::Base => "base",
            Arm::MatchedControl => "matched-control",
            Arm::Skill => "skill",
        }
    }
}

struct CliArgs {
    corpus: PathBuf,
    model: String,
    prompt_id: String,
    out: PathBuf,
    endpoint: String,
    turn_cap: u64,
    cell_ceiling_usd: f64,
    temperature: f32,
    ledger: Option<PathBuf>,
    turn_timeout_secs: u32,
    /// The experiment condition (default [`Arm::Base`], preserving today's
    /// behavior and the existing default-args test).
    arm: Arm,
    /// Ladder-2 revise phase: install THIS def into the scratch profile before
    /// the session (instead of the hardcoded S2 constant), so a run can start
    /// from a previously-built routine and edit it. The file is a v1 RoutineDef
    /// JSON; the install path is `<routine>.json` per the DefinitionStore
    /// contract (filename must equal the def's `routine`). None = today's
    /// behavior (S2 constant when the corpus cell declares a preseed).
    preseed_def: Option<PathBuf>,
    /// Ladder-2 revise phase: send THIS text as the operator prompt instead of
    /// the frozen corpus text (file-based so a long multi-line reviewer critique
    /// needs no shell escaping). None = the corpus cell's frozen prompt.
    prompt_override: Option<String>,
}

const USAGE: &str = "usage: elmer_battery --corpus <path> --model <openrouter-model-id> \
     --prompt <corpus-id> --out <bundle-dir> [--arm base|matched-control|skill] \
     [--endpoint <url>] [--turn-cap N] [--cell-ceiling-usd X] [--temperature T] \
     [--ledger <path>] [--turn-timeout-secs N] [--preseed-def <def.json>] \
     [--prompt-override-file <path>]   (or `--list-arms` to print the \
     supported arms and exit; reads OPENROUTER_API_KEY from the environment)";

fn parse_cli(args: &[String]) -> Result<CliArgs, String> {
    let mut corpus = None;
    let mut model = None;
    let mut prompt_id = None;
    let mut out = None;
    let mut endpoint = DEFAULT_ENDPOINT.to_string();
    let mut turn_cap = DEFAULT_TURN_CAP;
    let mut cell_ceiling_usd = DEFAULT_CELL_CEILING_USD;
    let mut temperature = DEFAULT_TEMPERATURE;
    let mut ledger = None;
    let mut turn_timeout_secs = DEFAULT_TURN_TIMEOUT_SECS;
    let mut arm = Arm::Base;
    let mut preseed_def = None;
    let mut prompt_override = None;

    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut val = |name: &str| -> Result<String, String> {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} requires a value\n{USAGE}"))
        };
        match flag.as_str() {
            "--corpus" => corpus = Some(PathBuf::from(val("--corpus")?)),
            "--model" => model = Some(val("--model")?),
            "--prompt" => prompt_id = Some(val("--prompt")?),
            "--out" => out = Some(PathBuf::from(val("--out")?)),
            "--endpoint" => endpoint = val("--endpoint")?,
            "--turn-cap" => {
                turn_cap = val("--turn-cap")?
                    .parse()
                    .map_err(|e| format!("--turn-cap: {e}"))?;
            }
            "--cell-ceiling-usd" => {
                cell_ceiling_usd = val("--cell-ceiling-usd")?
                    .parse()
                    .map_err(|e| format!("--cell-ceiling-usd: {e}"))?;
            }
            "--temperature" => {
                temperature = val("--temperature")?
                    .parse()
                    .map_err(|e| format!("--temperature: {e}"))?;
            }
            "--ledger" => ledger = Some(PathBuf::from(val("--ledger")?)),
            "--arm" => {
                arm = val("--arm")?.parse()?;
            }
            "--turn-timeout-secs" => {
                turn_timeout_secs = val("--turn-timeout-secs")?
                    .parse()
                    .map_err(|e| format!("--turn-timeout-secs: {e}"))?;
            }
            "--preseed-def" => preseed_def = Some(PathBuf::from(val("--preseed-def")?)),
            "--prompt-override-file" => {
                let p = PathBuf::from(val("--prompt-override-file")?);
                let text = std::fs::read_to_string(&p)
                    .map_err(|e| format!("--prompt-override-file {}: {e}", p.display()))?;
                prompt_override = Some(text);
            }
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }

    Ok(CliArgs {
        corpus: corpus.ok_or_else(|| format!("--corpus is required\n{USAGE}"))?,
        model: model.ok_or_else(|| format!("--model is required\n{USAGE}"))?,
        prompt_id: prompt_id.ok_or_else(|| format!("--prompt is required\n{USAGE}"))?,
        out: out.ok_or_else(|| format!("--out is required\n{USAGE}"))?,
        endpoint,
        turn_cap,
        cell_ceiling_usd,
        temperature,
        ledger,
        turn_timeout_secs,
        arm,
        preseed_def,
        prompt_override,
    })
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

/// Battery corpus (tests/battery/corpus.json). Prompt text is frozen —
/// operator-approved wording; this binary only reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Corpus {
    #[serde(default)]
    global_predicates: Vec<String>,
    prompts: Vec<CorpusPrompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CorpusPrompt {
    id: String,
    #[serde(default)]
    title: String,
    prompt: String,
    #[serde(default)]
    predicates: Vec<String>,
    /// Present on S2: the reference def installed into the scratch profile
    /// before send.
    #[serde(default)]
    preseed: Option<String>,
}

fn load_corpus(path: &Path) -> Result<(Corpus, String), String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("could not read corpus {}: {e}", path.display()))?;
    let corpus: Corpus = serde_json::from_slice(&bytes)
        .map_err(|e| format!("corpus {} did not parse: {e}", path.display()))?;
    Ok((corpus, sha256_hex(&bytes)))
}

// ---------------------------------------------------------------------------
// Ledger (cumulative cross-cell spend record)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Ledger {
    #[serde(default)]
    total_spend_usd: f64,
    #[serde(default)]
    cells: Vec<LedgerCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LedgerCell {
    ts: String,
    model: String,
    prompt: String,
    delta_usd: f64,
    /// true when the credits-after query failed and the token estimate was
    /// booked instead of the credits delta.
    estimated: bool,
}

/// The hard-stop predicate: at-or-above the cap the harness refuses to start.
fn ledger_blocks(ledger: &Ledger) -> bool {
    ledger.total_spend_usd >= LEDGER_HARD_STOP_USD
}

fn load_ledger(path: &Path) -> Result<Ledger, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("ledger {} did not parse: {e}", path.display())),
        // A missing ledger is a fresh sweep, not an error. Any OTHER read
        // failure is fail-closed: unreadable spend history must not silently
        // reset the budget.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ledger::default()),
        Err(e) => Err(format!("could not read ledger {}: {e}", path.display())),
    }
}

fn save_ledger(path: &Path, ledger: &Ledger) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create ledger dir: {e}"))?;
    }
    let json = serde_json::to_vec_pretty(ledger).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("could not write ledger: {e}"))
}

// ---------------------------------------------------------------------------
// Path preflight (Codex adrev disposition 1)
// ---------------------------------------------------------------------------

/// Component-wise containment check. `path` need not exist; `root` must.
fn path_is_under(path: &Path, root: &Path) -> bool {
    // Canonicalize the deepest EXISTING ancestor of `path` so a symlinked
    // temp root (macOS /tmp, some Linux setups) cannot defeat the check,
    // then re-append the not-yet-created remainder.
    let canon_root = match root.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut existing = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return false,
        }
    }
    let mut resolved = match existing.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };
    for seg in tail.iter().rev() {
        resolved.push(seg);
    }
    resolved.starts_with(&canon_root)
}

/// Abort-worthy assertion that `path` resolves under one of `roots`.
fn assert_under(label: &str, path: &Path, roots: &[&Path]) -> Result<(), String> {
    if roots.iter().any(|r| path_is_under(path, r)) {
        Ok(())
    } else {
        Err(format!(
            "SCRATCH-ISOLATION PREFLIGHT FAILED: {label} resolved to {} which is \
             outside the scratch root / bundle dir — refusing to touch operator state",
            path.display()
        ))
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Best-effort HEAD sha of the checkout the binary runs from.
fn git_head_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Guarded JSON write: preflights the destination against the writable roots
/// before touching disk (adrev disposition 1's "every written path" clause).
fn write_json_guarded(
    label: &str,
    path: &Path,
    value: &serde_json::Value,
    roots: &[&Path],
) -> Result<(), String> {
    assert_under(label, path, roots)?;
    let json = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("could not write {label}: {e}"))
}

// ---------------------------------------------------------------------------
// Synchronous tool_calls.jsonl
// ---------------------------------------------------------------------------

/// Synchronous append-only JSONL writer. One line per tool call, flushed
/// before `invoke` returns — the drop-tolerant transcript sink is NOT trusted
/// for the call-sequence record (adrev disposition 7).
struct ToolCallLog {
    file: Mutex<std::fs::File>,
    seq: AtomicU64,
}

impl ToolCallLog {
    fn open(path: &Path) -> Result<Self, String> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("could not open {}: {e}", path.display()))?;
        Ok(Self {
            file: Mutex::new(file),
            seq: AtomicU64::new(0),
        })
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn append(&self, record: &serde_json::Value) {
        let mut f = self.file.lock().expect("tool-call log lock poisoned");
        // A failed append must not kill the run — the transcript still exists;
        // stderr flags the gap for the judge.
        if let Err(e) = writeln!(f, "{record}") {
            eprintln!("elmer_battery: tool_calls.jsonl append failed: {e}");
        }
        let _ = f.flush();
    }
}

/// Shared run meters, fed by the invoker and the event sink, read by the
/// budget watchdog.
#[derive(Default)]
struct Meters {
    provider_turns: AtomicU64,
    tool_calls: AtomicU64,
    denied_calls: AtomicU64,
    prompt_tokens: AtomicU64,
    eval_tokens: AtomicU64,
}

/// Observation wrapper around the real in-process invoker: logging + metering
/// ONLY, zero policy (tuxlink-y9a6l parity ruling). Every call passes through
/// to the production tool implementations; the only denials a model can
/// experience are the production consent/authority gates the inner invoker
/// itself returns, and those are logged with `denied_by: "production_gate"`
/// so the analysis can distinguish them at a glance.
struct ObservedInvoker {
    inner: InProcessMcpInvoker,
    log: Arc<ToolCallLog>,
    meters: Arc<Meters>,
}

#[async_trait]
impl ToolInvoker for ObservedInvoker {
    fn tools(&self) -> &[ToolSpec] {
        self.inner.tools()
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        let seq = self.log.next_seq();
        let started = Instant::now();
        let ts = now_rfc3339();
        self.meters.tool_calls.fetch_add(1, Ordering::SeqCst);

        let outcome = self.inner.invoke(call, authority, cancel).await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let (status, denied, detail, preview, result_chars) = match &outcome {
            ToolOutcome::Ok(v) => {
                let full = v.to_string();
                let chars = full.chars().count();
                let preview: String = full.chars().take(RESULT_PREVIEW_CHARS).collect();
                ("ok", false, None, Some(preview), Some(chars))
            }
            ToolOutcome::Denied(reason) => {
                // A REAL production gate (consent/authority), not harness
                // policy. Metered so outcome.json's denied_calls keeps meaning
                // "the model hit a production boundary".
                self.meters.denied_calls.fetch_add(1, Ordering::SeqCst);
                ("denied", true, Some(reason.clone()), None, None)
            }
            ToolOutcome::InvalidArgs(detail) => {
                ("invalid_args", false, Some(detail.clone()), None, None)
            }
            ToolOutcome::Cancelled(reason) => {
                ("cancelled", false, Some(reason.clone()), None, None)
            }
        };
        self.log.append(&serde_json::json!({
            "seq": seq,
            "ts": ts,
            "tool": call.name,
            "args": call.args,
            "status": status,
            "denied": denied,
            "denied_by": if denied { serde_json::json!("production_gate") } else { serde_json::Value::Null },
            "detail": detail,
            "result_preview": preview,
            "result_chars": result_chars,
            "elapsed_ms": elapsed_ms,
        }));
        outcome
    }
}

/// Adapts a shared `Arc<dyn ToolInvoker>` into the `Box<dyn ToolInvoker>`
/// `ElmerSession::new_with_invoker` wants, by delegating every method to the
/// inner `Arc`. The invoker is built once as an `Arc` (one
/// [`ObservedInvoker`] over one `McpState` → one `config_dir/routines` store)
/// and `Box::new(SharedInvoker(arc.clone()))` goes into the session.
/// Without sharing, Full's Emit saves would land in a different store than the
/// one CI validates and Base's session writes to.
struct SharedInvoker(Arc<dyn ToolInvoker>);

#[async_trait]
impl ToolInvoker for SharedInvoker {
    fn tools(&self) -> &[ToolSpec] {
        self.0.tools()
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        self.0.invoke(call, authority, cancel).await
    }
}

// ---------------------------------------------------------------------------
// Event sink → turns.log + meters
// ---------------------------------------------------------------------------

/// The battery's [`EventSink`]: appends assistant turns to `turns.log`
/// (synchronous JSONL) and feeds the meters the watchdog reads. Deltas are
/// deliberately not persisted — the finalizing Turn carries the full text.
/// Flush the accumulated streaming-delta run (one `(delta_kind, text)` buffer)
/// as a single `deltas.jsonl` line. The caller must NOT hold `pending`'s lock.
fn flush_pending_delta(pending: &Mutex<Option<(String, String)>>, deltas: &Mutex<std::fs::File>) {
    let taken = pending.lock().expect("delta buffer lock poisoned").take();
    if let Some((delta_kind, text)) = taken {
        let mut f = deltas.lock().expect("deltas.jsonl lock poisoned");
        let line = serde_json::json!({ "ts": now_rfc3339(), "delta_kind": delta_kind, "text": text });
        if let Err(e) = writeln!(f, "{line}") {
            eprintln!("elmer_battery: deltas.jsonl append failed: {e}");
        }
        let _ = f.flush();
    }
}

fn make_battery_sink(
    meters: Arc<Meters>,
    turns_log: PathBuf,
    deltas_log: PathBuf,
) -> Result<EventSink, String> {
    let open = |p: &PathBuf| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .map_err(|e| format!("could not open {}: {e}", p.display()))
    };
    let turns = Mutex::new(open(&turns_log)?);
    let deltas = Mutex::new(open(&deltas_log)?);
    // Coalesce contiguous same-kind streaming deltas into one line per run. The
    // finalizing `Turn` carries the assembled ASSISTANT text, but "reasoning"
    // deltas have no finalizing turn — on a tool-only turn (e.g. base/S1's whole
    // run) they were previously discarded (`_ => {}`). Capturing them is this
    // fix's point: full run visibility, esp. for reasoning-emitting cloud models.
    let pending: Mutex<Option<(String, String)>> = Mutex::new(None);
    Ok(Arc::new(move |event: ElmerEvent| {
        match &event {
            ElmerEvent::Delta { delta_kind, chunk } => {
                let mut p = pending.lock().expect("delta buffer lock poisoned");
                match p.as_mut() {
                    Some((k, acc)) if k == delta_kind => acc.push_str(chunk),
                    _ => {
                        // Kind changed (or first chunk): flush the prior run, start new.
                        let prev = p.replace((delta_kind.clone(), chunk.clone()));
                        drop(p);
                        if let Some((k, text)) = prev {
                            let mut f = deltas.lock().expect("deltas.jsonl lock poisoned");
                            let line = serde_json::json!({ "ts": now_rfc3339(), "delta_kind": k, "text": text });
                            let _ = writeln!(f, "{line}");
                            let _ = f.flush();
                        }
                    }
                }
            }
            ElmerEvent::Turn { role, text } => {
                flush_pending_delta(&pending, &deltas);
                let mut f = turns.lock().expect("turns.log lock poisoned");
                let line = serde_json::json!({ "ts": now_rfc3339(), "role": role, "text": text });
                if let Err(e) = writeln!(f, "{line}") {
                    eprintln!("elmer_battery: turns.log append failed: {e}");
                }
                let _ = f.flush();
            }
            // Tool-call / terminal boundaries: flush any reasoning that preceded
            // them (the tool-only-turn case where no finalizing Turn arrives).
            ElmerEvent::Chip { .. } | ElmerEvent::Outcome { .. } => {
                flush_pending_delta(&pending, &deltas);
            }
            ElmerEvent::Context {
                prompt_tokens,
                eval_tokens,
                ..
            } => {
                // One ContextUsage per provider turn on the OpenAI-compat path
                // (stream_options.include_usage requested by the adapter) —
                // the turn counter the watchdog gates on. Absent on some error
                // paths (adrev disposition 6), which the tool-call belt and
                // the runner's own wall-clock limits cover.
                meters.provider_turns.fetch_add(1, Ordering::SeqCst);
                meters
                    .prompt_tokens
                    .fetch_add(u64::from(*prompt_tokens), Ordering::SeqCst);
                meters
                    .eval_tokens
                    .fetch_add(u64::from(*eval_tokens), Ordering::SeqCst);
            }
        }
    }))
}

// ---------------------------------------------------------------------------
// OpenRouter credits + pricing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Default)]
struct CreditsSnapshot {
    total_credits: f64,
    total_usage: f64,
}

/// Whether a failed credits query should ABORT the cell. True only for
/// OpenRouter, whose `/api/v1/credits` is the hard spend record (and gates
/// the $45 ledger stop) — losing that baseline silently would defeat the
/// budget guard. Any other origin (a local vLLM, a self-hosted shim) has no
/// credits API by design; there its absence is expected and the cell falls
/// back to a zero baseline, bounded by the turn cap + wall clock
/// (tuxlink-g31en).
fn credits_failure_is_fatal(origin: &str) -> bool {
    origin.contains("openrouter.ai")
}

async fn fetch_credits(
    client: &reqwest::Client,
    origin: &str,
    key: &str,
) -> Result<CreditsSnapshot, String> {
    let url = format!("{origin}/api/v1/credits");
    let body: serde_json::Value = client
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("credits query failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("credits query failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("credits response did not parse: {e}"))?;
    let data = body
        .get("data")
        .ok_or_else(|| "credits response missing data".to_string())?;
    let read = |k: &str| -> Result<f64, String> {
        data.get(k)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("credits response missing data.{k}"))
    };
    Ok(CreditsSnapshot {
        total_credits: read("total_credits")?,
        total_usage: read("total_usage")?,
    })
}

#[derive(Debug, Clone, Copy)]
struct Pricing {
    prompt_usd_per_tok: f64,
    completion_usd_per_tok: f64,
}

/// Best-effort per-token pricing for `model` from the models listing. `None`
/// disables the mid-run cost estimate (the turn cap + runner wall clock still
/// bound the cell; the credits delta remains the hard record).
async fn fetch_pricing(
    client: &reqwest::Client,
    origin: &str,
    key: &str,
    model: &str,
) -> Option<Pricing> {
    let url = format!("{origin}/api/v1/models");
    let body: serde_json::Value = client
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let entry = body
        .get("data")?
        .as_array()?
        .iter()
        .find(|m| m.get("id").and_then(serde_json::Value::as_str) == Some(model))?;
    let pricing = entry.get("pricing")?;
    let parse = |k: &str| -> Option<f64> {
        let v = pricing.get(k)?;
        match v {
            serde_json::Value::String(s) => s.parse().ok(),
            serde_json::Value::Number(n) => n.as_f64(),
            _ => None,
        }
    };
    Some(Pricing {
        prompt_usd_per_tok: parse("prompt")?,
        completion_usd_per_tok: parse("completion")?,
    })
}

fn estimated_cost_usd(meters: &Meters, pricing: Option<Pricing>) -> Option<f64> {
    let p = pricing?;
    let prompt = meters.prompt_tokens.load(Ordering::SeqCst) as f64;
    let eval = meters.eval_tokens.load(Ordering::SeqCst) as f64;
    Some(prompt * p.prompt_usd_per_tok + eval * p.completion_usd_per_tok)
}

// ---------------------------------------------------------------------------
// In-memory key entry (no-disk-creds; headless SSH has no unlocked keyring)
// ---------------------------------------------------------------------------

/// EntryLike that answers every read with the env-supplied API key and
/// swallows writes. Account-agnostic on purpose: the per-turn provider build
/// keys its read by endpoint origin, and matching that derivation here would
/// add a silent-mismatch failure mode for zero isolation benefit (one key,
/// one process, memory only).
struct EnvKeyEntry {
    key: String,
}

impl EntryLike for EnvKeyEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        Ok(self.key.clone())
    }
    fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
        Ok(())
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        Ok(())
    }
}

fn env_keyring(key: String) -> ElmerKeyring {
    let factory: EntryFactory = Box::new(move |_service: &str, _account: &str| {
        Box::new(EnvKeyEntry { key: key.clone() }) as Box<dyn EntryLike>
    });
    ElmerKeyring::with_factory(factory)
}

// ---------------------------------------------------------------------------
// Outcome mapping
// ---------------------------------------------------------------------------

fn outcome_fields(outcome: &RunOutcome) -> (&'static str, String) {
    match outcome {
        RunOutcome::Completed(text) => ("completed", text.clone()),
        // A wall-clock deadline is CENSORED data, not a verdict about the model.
        // Folding it into `needs_operator` made it unreadable in both directions:
        // scored as a capability failure, while its near-zero tool-call count
        // simultaneously read as an ideal low-churn run. 2026-07-25: 50 of 220
        // bundles were misread this way and drove a whole round of failure
        // analysis before it was caught. The predicate lives beside the two
        // constructors in tuxlink-agent-runner so this cannot drift.
        RunOutcome::NeedsOperator(reason)
            if tuxlink_agent_runner::is_deadline_reason(reason) =>
        {
            ("truncated", reason.clone())
        }
        RunOutcome::NeedsOperator(reason) => ("needs_operator", reason.clone()),
        RunOutcome::InvalidAction(detail) => ("invalid_action", detail.clone()),
        RunOutcome::Cancelled => ("cancelled", String::new()),
        RunOutcome::ToolDenied(reason) => ("tool_denied", reason.clone()),
        RunOutcome::RateLimited(detail) => ("rate_limited", detail.clone()),
        RunOutcome::ProviderError(detail) => ("provider_error", detail.clone()),
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    if let Err(e) = real_main() {
        eprintln!("elmer_battery: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--list-arms`: print the arms this BINARY supports (one per line) and exit,
    // before any required-arg parsing or paid model call. A launcher must assert
    // every arm it intends to run is in this list — interrogating the actual
    // executable is the only reliable guard against a stale script requesting an
    // arm the binary lacks (tuxlink-m0n38 post-incident: a run script asked for a
    // `skill` arm a pre-#1248 binary didn't have, and the mismatch only surfaced
    // after burning model time).
    if args.iter().any(|a| a == "--list-arms") {
        for arm in Arm::ALL {
            println!("{}", arm.as_str());
        }
        return Ok(());
    }

    let cli = parse_cli(&args)?;

    // The key stays in this process's memory only — never in the bundle, the
    // manifest, the logs, or the scratch profile.
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map_err(|_| "OPENROUTER_API_KEY is not set (export it for this invocation only)")?;
    if api_key.trim().is_empty() {
        return Err("OPENROUTER_API_KEY is empty".into());
    }

    let (corpus, corpus_sha) = load_corpus(&cli.corpus)?;
    let entry = corpus
        .prompts
        .iter()
        .find(|p| p.id == cli.prompt_id)
        .cloned()
        .ok_or_else(|| {
            let known: Vec<&str> = corpus.prompts.iter().map(|p| p.id.as_str()).collect();
            format!("prompt {:?} not in corpus (known: {known:?})", cli.prompt_id)
        })?;

    // ── Cumulative-spend hard stop (adrev disposition 5) ────────────────────
    let ledger_path = match &cli.ledger {
        Some(p) => p.clone(),
        None => cli
            .out
            .parent()
            .map(|p| p.join("ledger.json"))
            .unwrap_or_else(|| PathBuf::from("ledger.json")),
    };
    let mut ledger = load_ledger(&ledger_path)?;
    if ledger_blocks(&ledger) {
        return Err(format!(
            "cumulative battery spend ${:.2} ≥ ${LEDGER_HARD_STOP_USD:.2} hard stop \
             ({}) — escalate to the operator before running more cells",
            ledger.total_spend_usd,
            ledger_path.display()
        ));
    }

    // ── Scratch profile + env isolation (adrev disposition 1) ───────────────
    // set_var BEFORE any tauri/app construction, at this single-threaded point.
    let scratch = tempfile::Builder::new()
        .prefix("tuxlink-battery-")
        .tempdir()
        .map_err(|e| format!("could not create scratch dir: {e}"))?;
    let scratch_root = scratch
        .path()
        .canonicalize()
        .map_err(|e| format!("could not canonicalize scratch dir: {e}"))?;
    let config_dir = scratch_root.join("config");
    for (var, sub) in [
        ("TUXLINK_CONFIG_DIR", "config"),
        ("XDG_CONFIG_HOME", "xdg-config"),
        ("XDG_DATA_HOME", "xdg-data"),
        ("XDG_CACHE_HOME", "xdg-cache"),
        ("HOME", "home"),
    ] {
        let dir = scratch_root.join(sub);
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {sub}: {e}"))?;
        std::env::set_var(var, &dir);
    }
    std::fs::write(config_dir.join("config.json"), SCRATCH_CONFIG_JSON)
        .map_err(|e| format!("could not seed scratch config.json: {e}"))?;

    // Preseed: install a reference def before the session exists so the model
    // edits an existing routine rather than authoring from scratch. Ladder-2's
    // revise phase supplies an arbitrary built def via --preseed-def (filename
    // MUST equal the def's `routine` per the DefinitionStore contract);
    // otherwise the S2 corpus cell installs the hardcoded nearest-40m-dial
    // constant. --preseed-def takes precedence when both are present.
    if let Some(def_path) = cli.preseed_def.as_ref() {
        let def_json = std::fs::read_to_string(def_path)
            .map_err(|e| format!("could not read --preseed-def {}: {e}", def_path.display()))?;
        let parsed: serde_json::Value = serde_json::from_str(&def_json)
            .map_err(|e| format!("--preseed-def {} is not JSON: {e}", def_path.display()))?;
        let routine_name = parsed
            .get("routine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                format!(
                    "--preseed-def {} has no string `routine` field",
                    def_path.display()
                )
            })?;
        let routines_dir = config_dir.join("routines");
        std::fs::create_dir_all(&routines_dir)
            .map_err(|e| format!("could not create scratch routines dir: {e}"))?;
        std::fs::write(routines_dir.join(format!("{routine_name}.json")), &def_json)
            .map_err(|e| format!("could not write preseed def: {e}"))?;
    } else if entry.preseed.is_some() {
        let routines_dir = config_dir.join("routines");
        std::fs::create_dir_all(&routines_dir)
            .map_err(|e| format!("could not create scratch routines dir: {e}"))?;
        std::fs::write(
            routines_dir.join(format!("{PRESEED_NAME}.json")),
            PRESEED_NEAREST_40M_DIAL,
        )
        .map_err(|e| format!("could not write preseed def: {e}"))?;
    }

    // ── Bundle dir ──────────────────────────────────────────────────────────
    std::fs::create_dir_all(&cli.out)
        .map_err(|e| format!("could not create bundle dir {}: {e}", cli.out.display()))?;
    let bundle = cli
        .out
        .canonicalize()
        .map_err(|e| format!("could not canonicalize bundle dir: {e}"))?;

    // Validate the endpoint up front (also yields the origin for the credits /
    // models queries).
    let endpoint_parsed = AgentEndpoint::parse(&cli.endpoint)
        .map_err(|e| format!("--endpoint {:?} rejected: {e}", cli.endpoint))?;
    let origin = endpoint_parsed.origin();

    // ── Windowless app (adrev disposition 4) ────────────────────────────────
    // Builder::build() ONLY — App::run() (whose setup creates the config
    // windows) is never called. The config's window list is cleared as a belt,
    // and emptiness is asserted below. The production setup closure is NOT
    // reused: managed state is à-la-carte per the wiring matrix, and the
    // routines scheduler spawn + launch recovery are DELIBERATELY OMITTED so
    // nothing ever fires.
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .build(context)
        .map_err(|e| format!("windowless app construction failed: {e}"))?;
    if !app.webview_windows().is_empty() {
        return Err("windowless invariant violated: a webview window exists after build()".into());
    }

    // ── Scratch-isolation preflight (adrev disposition 1) ───────────────────
    let scratch_roots: [&Path; 1] = [scratch_root.as_path()];
    assert_under(
        "config_path()",
        &tuxlink_lib::config::config_path(),
        &scratch_roots,
    )?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unresolved: {e}"))?;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("app_cache_dir unresolved: {e}"))?;
    let app_config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir unresolved: {e}"))?;
    assert_under("app_data_dir", &data_dir, &scratch_roots)?;
    assert_under("app_cache_dir", &cache_dir, &scratch_roots)?;
    assert_under("app_config_dir", &app_config_dir, &scratch_roots)?;
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("could not create data dir: {e}"))?;

    // ── À-la-carte managed state (wiring matrix; mirrors lib.rs) ────────────
    // Fresh guard, DISARMED for the whole battery (adrev disposition 2:
    // authoring needs no arm; fail-closed). There is no rearm() in this file.
    let guard = Arc::new(tuxlink_security::EgressGuard::new());
    app.manage(Arc::clone(&guard));
    app.manage(tuxlink_lib::winlink::inbound_selection::SelectionRegistry::default());
    app.manage(tuxlink_lib::app_backend::BackendState::new());
    app.manage(Arc::new(tuxlink_lib::session_log::SessionLogState::unbounded()));
    app.manage(Arc::new(tuxlink_lib::modem_status::ModemSession::new()));
    // Listen/APRS states: fetched by MonolithAbortPort::ardop_disconnect
    // (mcp_ports.rs ~:1566) on the cancel path — the stage-P2 fable cell
    // panicked a worker thread here (state() before manage()). Managed as
    // production does (lib.rs:1757/:1772/:1777); all default-idle, no radio.
    app.manage(Arc::new(tuxlink_lib::ui_commands::ArdopListenState::default()));
    app.manage(Arc::new(tuxlink_lib::ui_commands::VaraListenState::default()));
    app.manage(tuxlink_lib::winlink::aprs::engine::AprsState::default());
    app.manage(Arc::new(tuxlink_lib::winlink::modem::vara::VaraSession::new()));
    // PositionArbiter from the scratch config — Manual + DM33 by construction.
    {
        let (src, grid, prec) = tuxlink_lib::config::read_config()
            .map(|c| {
                (
                    c.privacy.position_source,
                    c.identity.grid,
                    c.privacy.position_precision,
                )
            })
            .map_err(|e| format!("scratch config did not read back: {e:?}"))?;
        app.manage(Arc::new(tuxlink_lib::position::PositionArbiter::new(
            src, grid, prec,
        )));
    }
    // Station/channel caches: persistent under the SCRATCH data dir (same
    // TTLs as production; network fetch on cold cache is expected — the
    // battery box has internet).
    app.manage(Arc::new(
        tuxlink_lib::catalog::stations_cache::StationsCache::new_persistent(
            30 * 60 * 1000,
            15 * 60 * 1000,
            Arc::new(tuxlink_lib::catalog::stations_cache::SystemClock),
            data_dir.join("station-listings-cache.json"),
        ),
    ));
    app.manage(Arc::new(
        tuxlink_lib::catalog::channels_cache::ChannelsCache::new_persistent(
            60 * 60 * 1000,
            15 * 60 * 1000,
            Arc::new(tuxlink_lib::catalog::stations_cache::SystemClock),
            data_dir.join("channels-feed-cache.json"),
        ),
    ));
    // Contacts + Favorites stores (tuxlink-gcy3m): production manages BOTH
    // unconditionally (lib.rs, tuxlink-raez A2 / tuxlink-egmp B2 — open() is
    // INFALLIBLE, degrades to an empty store). This harness missed them; the
    // baseline0 EU2 cell's first p2p_peer_password_status call panicked a
    // worker thread (state() before manage()) and the dead tool future wedged
    // the cell past every timeout. Scratch data dir, so both start empty —
    // exactly what a fresh production station presents.
    app.manage(Arc::new(std::sync::Mutex::new(
        tuxlink_lib::contacts::store::ContactsStore::open(data_dir.join("contacts.json")),
    )));
    app.manage(Arc::new(std::sync::Mutex::new(
        tuxlink_lib::favorites::store::FavoritesStore::open(data_dir.join("stations.json")),
    )));
    // Same audit, same class (tool-reachable via app.state / State params):
    // the inbound-contact rate limiter and the forms sequence-counter store,
    // both unconditionally managed by production (lib.rs ~:2012/:2086).
    // Limiter config from the scratch config, as production reads its own.
    app.manage(Arc::new(std::sync::Mutex::new(
        tuxlink_lib::contacts::limiter::InboundCreateLimiter::new(
            tuxlink_lib::config::read_config()
                .map(|c| c.p2p_limits)
                .unwrap_or_default(),
        ),
    )));
    app.manage(Arc::new(std::sync::Mutex::new(
        tuxlink_lib::forms::sequence::SeqCounterStore::open(
            data_dir.join("forms-sequence-counters.json"),
        ),
    )));
    // find_stations query-snapshot store (tuxlink-m0n38): the redesigned MCP tool
    // resolves this from managed state; the main app registers it in run(), so
    // this harness must too or the port panics (state() before manage()).
    app.manage(Arc::new(
        tuxlink_lib::station_query::snapshot::SnapshotStore::new(300_000),
    ));
    // point_at pending-ack map (tuxlink-grc1j): production manages this
    // unconditionally (lib.rs). Unmanaged, control1-base EU2 attempt-10's
    // point_at call panicked a worker (state() before manage()) and the dead
    // future wedged the run 2h+. With the stub managed, headless point_at
    // emits to zero windows and times out at ACK_TIMEOUT into a cause-accurate
    // "window did not confirm" error: parity-honest (the tool exists) and
    // bounded.
    app.manage(Arc::new(
        tuxlink_lib::onboarding_bridge::PointAtPending::default(),
    ));
    // Search service (docs_search / docs_read / catalog_list).
    {
        let search_root = data_dir.join("native-mbox");
        std::fs::create_dir_all(&search_root)
            .map_err(|e| format!("could not create search root: {e}"))?;
        let svc = tuxlink_lib::search::build_service(&search_root)
            .map_err(|e| format!("search build_service failed: {e:?}"))?;
        app.manage(svc);
    }
    // Propagation: env-staged sidecar (tuxlink-hbu3b lift queue closes the
    // HARNESS_PARITY_RESIDUES entry). The battery box has no Tauri resource
    // tree, so the engine assets resolve from TUXLINK_VOACAPL_BIN +
    // TUXLINK_ITSHFBC_DIR (the production-.deb extraction staged on the
    // battery box). Absent or dangling env keeps the honest
    // engine-unavailable error and E2/E3-class results carry the residue.
    {
        use tuxlink_lib::propagation::commands::{PropagationState, ReadyPropagation};
        use tuxlink_lib::propagation::{engine::EnginePaths, ssn};
        let prop_state = match (
            std::env::var_os("TUXLINK_VOACAPL_BIN").map(PathBuf::from),
            std::env::var_os("TUXLINK_ITSHFBC_DIR").map(PathBuf::from),
        ) {
            (Some(bin), Some(itshfbc)) if bin.is_file() && itshfbc.is_dir() => {
                // Existence alone is not staging (Codex 2026-07-28 P1): a
                // non-executable file or a gutted itshfbc tree would register
                // Ready and only fail lazily when a model calls predict_path.
                // Verify the executable bit and the read-only subtrees voacapl
                // opens with status='old' up front.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = std::fs::metadata(&bin)
                        .map_err(|e| format!("staged voacapl metadata unreadable: {e}"))?
                        .permissions()
                        .mode();
                    if mode & 0o111 == 0 {
                        return Err(format!(
                            "staged voacapl is not executable: {}",
                            bin.display()
                        ));
                    }
                }
                for sub in ["coeffs", "database"] {
                    if !itshfbc.join(sub).is_dir() {
                        return Err(format!(
                            "staged itshfbc missing required subtree '{sub}': {}",
                            itshfbc.display()
                        ));
                    }
                }
                let scratch = data_dir.join("propagation-scratch");
                std::fs::create_dir_all(&scratch)
                    .map_err(|e| format!("could not create propagation scratch: {e}"))?;
                let forecast = ssn::SsnForecast::from_json(ssn::BUNDLED_SSN_FORECAST)
                    .map_err(|e| format!("bundled SSN forecast failed to parse: {e:?}"))?;
                eprintln!(
                    "battery propagation: engine staged (voacapl={})",
                    bin.display()
                );
                PropagationState::Ready(ReadyPropagation {
                    paths: EnginePaths {
                        binary: bin,
                        itshfbc_root: itshfbc,
                    },
                    scratch_parent: scratch,
                    clock: Arc::new(tuxlink_lib::catalog::stations_cache::SystemClock),
                    forecast,
                })
            }
            (None, None) => PropagationState::Unavailable(
                "battery harness: propagation engine not wired".to_string(),
            ),
            (bin, itshfbc) => {
                // Half-configured env is a staging mistake, not a valid
                // degraded mode: fail the launch so a ladder cannot silently
                // run residue-carrying cells while claiming a staged engine.
                return Err(format!(
                    "propagation env incomplete or paths missing \
                     (TUXLINK_VOACAPL_BIN={bin:?}, TUXLINK_ITSHFBC_DIR={itshfbc:?})"
                ));
            }
        };
        app.manage(prop_state);
    }
    // Routines engine over the scratch profile. Scheduler spawn (lib.rs
    // production block) and launch recovery are OMITTED — nothing ever fires;
    // routines exist here only to be authored and validated.
    let routines_state = Arc::new(
        tuxlink_lib::routines::session::build_routines_state_for_app(app.handle()),
    );
    app.manage(Arc::clone(&routines_state.arbiter));
    app.manage(Arc::clone(&routines_state));

    // Guard invariant (adrev disposition 2): disarmed, untainted, and it stays
    // that way — nothing in this binary can arm it.
    if guard.armed_remaining() != 0 || guard.is_tainted() {
        return Err("egress guard is not in the fresh disarmed state".into());
    }

    // ── Bundle instrumentation files ────────────────────────────────────────
    let tool_log = Arc::new(ToolCallLog::open(&bundle.join("tool_calls.jsonl"))?);
    let meters = Arc::new(Meters::default());
    let sink = make_battery_sink(
        Arc::clone(&meters),
        bundle.join("turns.log"),
        bundle.join("deltas.jsonl"),
    )?;
    let transcript = ElmerTranscriptSink::new(bundle.join("transcript"));

    // ── The async cell ──────────────────────────────────────────────────────
    let started_at = now_rfc3339();
    let started = Instant::now();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("http client build failed: {e}"))?;

    let handle = app.handle().clone();
    let cell = tauri::async_runtime::block_on(run_cell(RunCellArgs {
        cli: &cli,
        entry: &entry,
        api_key: &api_key,
        origin: &origin,
        http: &http,
        handle,
        guard: Arc::clone(&guard),
        tool_log: Arc::clone(&tool_log),
        meters: Arc::clone(&meters),
        sink,
        transcript: Arc::clone(&transcript),
        routines_state: Arc::clone(&routines_state),
    }))?;

    // ── assert-no-egress (Part-97 belt; adrev disposition 2) ────────────────
    // The guard was created DISARMED and nothing in this binary can arm it, so
    // every agent egress `authorize()` during the cell fail-closed — no send
    // could fire. `EgressGuard` exposes no numeric send counter, so the
    // invariant "zero live sends" is expressed through its actual fail-closed
    // API: a still-disarmed, still-untainted guard AFTER the cell proves no
    // agent egress was ever authorized. A violation is a HARD cell failure —
    // the agent's routine-edit tool calls must never have reached a
    // transmit/egress verb.
    if guard.armed_remaining() != 0 || guard.is_tainted() {
        return Err(format!(
            "ASSERT-NO-EGRESS FAILED: egress guard is not disarmed+untainted after the cell \
             (armed_remaining={}, tainted={}) — a live send may have been authorized",
            guard.armed_remaining(),
            guard.is_tainted()
        ));
    }

    // Flush the durable transcript before harvesting (queued writer thread).
    if !transcript.flush(Duration::from_secs(10)) {
        eprintln!("elmer_battery: transcript flush timed out; bundle transcript may be partial");
    }

    let writable_roots: [&Path; 2] = [scratch_root.as_path(), bundle.as_path()];

    // ── Harvest: scratch routines/*.json + programmatic validation ──────────
    // The authored def IS the primary judged artifact. Validation goes through
    // the SAME service fn the MCP tool uses (`validate_routine` over the live
    // RoutinesState) — not through the agent's MCP session, which is done.
    let harvest_dir = bundle.join("routines");
    std::fs::create_dir_all(&harvest_dir)
        .map_err(|e| format!("could not create harvest dir: {e}"))?;
    let mut validations = serde_json::Map::new();
    let scratch_routines = config_dir.join("routines");
    if let Ok(entries) = std::fs::read_dir(&scratch_routines) {
        for f in entries.flatten() {
            let path = f.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
            else {
                continue;
            };
            let dest = harvest_dir.join(format!("{stem}.json"));
            assert_under("harvested def", &dest, &writable_roots)?;
            if let Err(e) = std::fs::copy(&path, &dest) {
                eprintln!("elmer_battery: harvest copy of {stem} failed: {e}");
            }
            if stem == "enabled" {
                continue; // store bookkeeping, not a def
            }
            let verdict = match tuxlink_lib::routines::commands::validate_routine(
                &routines_state,
                &stem,
            ) {
                Ok(findings) => serde_json::to_value(&findings)
                    .unwrap_or_else(|e| serde_json::json!({ "error": e.to_string() })),
                Err(e) => serde_json::json!({ "error": format!("{e:?}") }),
            };
            validations.insert(stem, verdict);
        }
    }
    write_json_guarded(
        "validate.json",
        &bundle.join("validate.json"),
        &serde_json::Value::Object(validations),
        &writable_roots,
    )?;

    // ── outcome.json ────────────────────────────────────────────────────────
    let (kind, detail) = outcome_fields(&cell.outcome);
    write_json_guarded(
        "outcome.json",
        &bundle.join("outcome.json"),
        &serde_json::json!({
            "outcome": kind,
            "detail": detail,
            "cancel_reason": cell.cancel_reason,
            "started_at": started_at,
            "ended_at": now_rfc3339(),
            "duration_secs": started.elapsed().as_secs_f64(),
            "provider_turns": meters.provider_turns.load(Ordering::SeqCst),
            "tool_calls": meters.tool_calls.load(Ordering::SeqCst),
            "denied_calls": meters.denied_calls.load(Ordering::SeqCst),
            "prompt_tokens": meters.prompt_tokens.load(Ordering::SeqCst),
            "eval_tokens": meters.eval_tokens.load(Ordering::SeqCst),
            "arm": cli.arm.as_str(),
        }),
        &writable_roots,
    )?;

    // ── cost.json + ledger update ───────────────────────────────────────────
    // Credits delta is the hard record; the token estimate is booked only when
    // the after-query failed (flagged, conservative-not-silent).
    let est = estimated_cost_usd(&meters, cell.pricing);
    let (delta_usd, estimated) = match cell.credits_after {
        Some(after) => ((after.total_usage - cell.credits_before.total_usage).max(0.0), false),
        None => (est.unwrap_or(0.0), true),
    };
    write_json_guarded(
        "cost.json",
        &bundle.join("cost.json"),
        &serde_json::json!({
            "before": cell.credits_before,
            "after": cell.credits_after,
            "delta_usd": delta_usd,
            "delta_estimated": estimated,
            "token_estimate_usd": est,
        }),
        &writable_roots,
    )?;
    ledger.total_spend_usd += delta_usd;
    ledger.cells.push(LedgerCell {
        ts: now_rfc3339(),
        model: cli.model.clone(),
        prompt: cli.prompt_id.clone(),
        delta_usd,
        estimated,
    });
    save_ledger(&ledger_path, &ledger)?;

    // The Skill arm composes the routine invariant + authoring skill into the
    // system prompt; Base/MatchedControl leave it as the pure ELMER_SYSTEM_PROMPT.
    // Record the EFFECTIVE prompt hash + skill version so results tie to exactly
    // what ran (tuxlink-t3jci P1). Base/MatchedControl hashes are unchanged
    // (compose(None, false) == None -> the base prompt).
    let authoring = matches!(cli.arm, Arm::Skill);
    let effective_system_prompt =
        tuxlink_agent_frontend::provider::compose_system_prompt(None, authoring)
            .unwrap_or_else(|| tuxlink_agent_frontend::provider::ELMER_SYSTEM_PROMPT.to_string());

    // ── run_manifest.json (adrev disposition 9) ─────────────────────────────
    write_json_guarded(
        "run_manifest.json",
        &bundle.join("run_manifest.json"),
        &serde_json::json!({
            "schema": "elmer-battery/1",
            "git_sha": git_head_sha(),
            "arm": cli.arm.as_str(),
            "corpus_path": cli.corpus.display().to_string(),
            "corpus_sha256": corpus_sha,
            "global_predicates_count": corpus.global_predicates.len(),
            "prompt": &entry,
            "endpoint": cli.endpoint,
            "model": cli.model,
            "provider_routing_pins": serde_json::Value::Null,
            "temperature": cli.temperature,
            "turn_cap": cli.turn_cap,
            "cell_ceiling_usd": cli.cell_ceiling_usd,
            "turn_timeout_secs": cli.turn_timeout_secs,
            "system_prompt_sha256": sha256_hex(effective_system_prompt.as_bytes()),
            "system_prompt_override": serde_json::Value::Null,
            "authoring": authoring,
            "authoring_skill_version": if authoring {
                Some(tuxlink_agent_frontend::provider::AUTHORING_SKILL_VERSION)
            } else {
                None
            },
            "tool_schema_sha256": cell.tool_schema_sha256,
            "harness": "parity-v1 (tuxlink-y9a6l): full production tool surface, no allowlist, no deny teaching",
            "harness_parity_residues": harness_parity_residues(),
            "scratch_root": scratch_root.display().to_string(),
            "preseed": entry.preseed.as_deref().map(|_| PRESEED_NAME),
            "credits_before": cell.credits_before,
            "credits_after": cell.credits_after,
            "spend_delta_usd": delta_usd,
            "pricing_known": cell.pricing.is_some(),
        }),
        &writable_roots,
    )?;

    println!(
        "elmer_battery: cell {} × {} → {} (outcome: {kind}; spend ${delta_usd:.4}{})",
        cli.model,
        cli.prompt_id,
        bundle.display(),
        if estimated { " estimated" } else { "" }
    );
    // `scratch` (TempDir) drops here and cleans the profile; the bundle holds
    // everything the judge needs.
    drop(scratch);
    Ok(())
}

// ---------------------------------------------------------------------------
// The async cell body
// ---------------------------------------------------------------------------

struct RunCellArgs<'a> {
    cli: &'a CliArgs,
    entry: &'a CorpusPrompt,
    api_key: &'a str,
    origin: &'a str,
    http: &'a reqwest::Client,
    handle: tauri::AppHandle,
    guard: Arc<tuxlink_security::EgressGuard>,
    tool_log: Arc<ToolCallLog>,
    meters: Arc<Meters>,
    sink: EventSink,
    transcript: Arc<ElmerTranscriptSink>,
    /// The managed routines state, used by the MatchedControl arm to enumerate
    /// the affordance catalog for its prompt prefix.
    routines_state: Arc<RoutinesState>,
}

struct CellResult {
    outcome: RunOutcome,
    cancel_reason: Option<String>,
    credits_before: CreditsSnapshot,
    credits_after: Option<CreditsSnapshot>,
    pricing: Option<Pricing>,
    tool_schema_sha256: String,
}

async fn run_cell(args: RunCellArgs<'_>) -> Result<CellResult, String> {
    let RunCellArgs {
        cli,
        entry,
        api_key,
        origin,
        http,
        handle,
        guard,
        tool_log,
        meters,
        sink,
        transcript,
        routines_state,
    } = args;

    // Credits BEFORE is the spend baseline. For OpenRouter it is a hard gate
    // (see [`credits_failure_is_fatal`]); for a local / non-OpenRouter
    // endpoint there is no credits API, so a failure there falls back to a
    // zero baseline and the turn cap + wall clock bound the cell (tuxlink-g31en).
    let credits_before = match fetch_credits(http, origin, api_key).await {
        Ok(c) => c,
        Err(e) if credits_failure_is_fatal(origin) => return Err(e),
        Err(e) => {
            eprintln!(
                "elmer_battery: no credits API at {origin} ({e}); local-endpoint mode — turn-cap + wall-clock bound the cell"
            );
            CreditsSnapshot::default()
        }
    };
    let pricing = fetch_pricing(http, origin, api_key, &cli.model).await;
    if pricing.is_none() {
        eprintln!(
            "elmer_battery: no pricing found for {:?}; mid-run cost estimate disabled \
             (turn cap + wall clock still bound the cell)",
            cli.model
        );
    }

    // ── McpState — the verbatim production port bag (lib.rs Elmer block) ────
    let h = handle.clone();
    let mcp_state = Arc::new(tuxlink_mcp_core::McpState {
        guard: Arc::clone(&guard),
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        status: Arc::new(tuxlink_lib::mcp_ports::MonolithStatusPort::new(h.clone())),
        mailbox: Arc::new(tuxlink_lib::mcp_ports::MonolithMailboxPort::new(h.clone())),
        search: Arc::new(tuxlink_lib::mcp_ports::MonolithSearchPort::new(h.clone())),
        config: Arc::new(tuxlink_lib::mcp_ports::MonolithConfigPort::new(h.clone())),
        devices: Arc::new(tuxlink_lib::mcp_ports::MonolithDevicePort::new(h.clone())),
        logs: Arc::new(tuxlink_lib::mcp_ports::MonolithLogPort::new(h.clone())),
        egress: Arc::new(tuxlink_lib::mcp_ports::MonolithEgressPort::new(
            h.clone(),
            Arc::clone(&guard),
        )),
        abort: Arc::new(tuxlink_lib::mcp_ports::MonolithAbortPort::new(h.clone())),
        write: Arc::new(tuxlink_lib::mcp_ports::MonolithWritePort::new(
            h.clone(),
            Arc::clone(&guard),
        )),
        compose: Arc::new(tuxlink_lib::mcp_ports::MonolithComposePort::new(h.clone())),
        stations: Arc::new(tuxlink_lib::mcp_ports::MonolithStationPort::new(
            h.clone(),
            Arc::clone(&guard),
        )),
        prediction: Arc::new(tuxlink_lib::mcp_ports::MonolithPredictionPort::new(h.clone())),
        provision: Arc::new(tuxlink_lib::mcp_ports::MonolithProvisionPort::new(h.clone())),
        ui_hint: Arc::new(tuxlink_lib::mcp_ports::MonolithUiHintPort::new(h.clone())),
        wwv: Arc::new(tuxlink_lib::mcp_ports::MonolithWwvPort::new(h.clone())),
        ft8: Arc::new(tuxlink_lib::mcp_ports::MonolithFt8Port::new(h.clone())),
        routines: Arc::new(tuxlink_lib::mcp_ports::MonolithRoutinesPort::new(h.clone())),
    });

    let invoker = InProcessMcpInvoker::connect(mcp_state)
        .await
        .map_err(|e| format!("in-process MCP connect failed: {e}"))?;
    // Pin the schema the model saw (adrev disposition 9) BEFORE the invoker
    // moves into the observation wrapper.
    let tool_schema_sha256 = sha256_hex(
        &serde_json::to_vec(invoker.tools()).map_err(|e| e.to_string())?,
    );
    // Built as an `Arc<dyn ToolInvoker>` (not `Box`ed straight into the
    // session) so BOTH the Base-arm session and the Full-arm Emit dispatch can
    // share this ONE invoker over this ONE `McpState`. Its `routines_save`
    // routes through `MonolithRoutinesPort` to `config_dir/routines`, the same
    // dir the Full arm opens as the workflow `DefinitionStore` — so an Emit save
    // is visible to CI's `store.get` and to the harvest, whichever arm ran.
    let allow: Arc<dyn ToolInvoker> = Arc::new(ObservedInvoker {
        inner: invoker,
        log: Arc::clone(&tool_log),
        meters: Arc::clone(&meters),
    });

    // ── Provider + model config (temperature PINNED from the CLI) ───────────
    let endpoint = AgentEndpoint::parse(&cli.endpoint)
        .map_err(|e| format!("endpoint rejected: {e}"))?;
    let provider: Arc<dyn tuxlink_agent_runner::Provider> = Arc::new(
        ElmerProvider::new_vetted(
            endpoint,
            cli.model.clone(),
            None,
            Some(cli.temperature),
            None,
            Some(ApiKey::new(api_key.to_string())),
        )
        .await
        .map_err(|e| format!("provider vetting failed: {e}"))?,
    );
    let model_config = Arc::new(ElmerModelConfigState::new(
        cli.endpoint.clone(),
        cli.model.clone(),
        cli.turn_timeout_secs,
        None,
        Some(cli.temperature),
        None,
    ));
    // In-memory keyring: the per-turn provider build (authoritative) reads the
    // env key through the with_factory seam; nothing touches the OS keyring.
    let keyring = Arc::new(env_keyring(api_key.to_string()));

    let flush_outbox = Arc::new(tuxlink_lib::mcp_ports::MonolithOutboxReadPort::new(
        handle.clone(),
    ));
    let flush_egress = Arc::new(tuxlink_lib::mcp_ports::MonolithEgressPort::new(
        handle.clone(),
        Arc::clone(&guard),
    ));
    let outbox_trait: Arc<dyn tuxlink_mcp_core::ports::OutboxReadPort + Send + Sync> =
        flush_outbox.clone();
    let abort_port: Arc<dyn tuxlink_mcp_core::ports::AbortPort + Send + Sync> = Arc::new(
        tuxlink_lib::mcp_ports::MonolithAbortPort::new(handle.clone()),
    );

    let session = Arc::new(ElmerSession::new_with_invoker(
        Box::new(SharedInvoker(Arc::clone(&allow))),
        Arc::clone(&provider),
        model_config,
        keyring,
        Arc::clone(&guard),
        abort_port,
        outbox_trait,
        flush_outbox,
        flush_egress,
        transcript,
    ));

    // ── Budget/turn watchdog (adrev disposition 5) ──────────────────────────
    // Polls the sink-fed meters and fires the session's ungated
    // cancel_and_abort when the provider-turn cap, the tool-call belt, or the
    // token-estimate ceiling is crossed. The runner's own Limits (30 min run /
    // per-turn timeout) bound the cell regardless.
    let cancel_reason: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let watchdog_stop = CancellationToken::new();
    let watchdog = {
        let session = Arc::clone(&session);
        let meters = Arc::clone(&meters);
        let reason_slot = Arc::clone(&cancel_reason);
        let stop = watchdog_stop.clone();
        let turn_cap = cli.turn_cap;
        let ceiling = cli.cell_ceiling_usd;
        // Live-spend poll inputs: the credits endpoint is the ONLY honest
        // mid-run meter. Stage-P2 evidence: the token estimate overshot the
        // real credits delta 4× on anthropic models (provider-side prompt
        // caching bills cached input at a fraction of list price) and
        // cancelled a healthy fable-5 cell at $0.52 actual spend. The token
        // estimate survives only as a 3×-ceiling belt for the window where a
        // credits poll has not yet succeeded.
        let http_wd = http.clone();
        let origin_wd = origin.to_string();
        let key_wd = api_key.to_string();
        let usage_before = credits_before.total_usage;
        tokio::spawn(async move {
            let mut ticks: u64 = 0;
            let mut live_spend: Option<f64> = None;
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
                ticks += 1;
                // Poll credits every 15 s (30 ticks) — cheap GET, honest meter.
                if ticks % 30 == 0 {
                    if let Ok(now) = fetch_credits(&http_wd, &origin_wd, &key_wd).await {
                        live_spend = Some((now.total_usage - usage_before).max(0.0));
                    }
                }
                let turns = meters.provider_turns.load(Ordering::SeqCst);
                let calls = meters.tool_calls.load(Ordering::SeqCst);
                let est = estimated_cost_usd(&meters, pricing);
                let trip = if turns >= turn_cap {
                    Some(format!("provider-turn cap reached ({turns}/{turn_cap})"))
                } else if calls >= turn_cap * TOOL_CALL_CAP_FACTOR {
                    Some(format!(
                        "tool-call belt reached ({calls} calls ≥ {turn_cap}×{TOOL_CALL_CAP_FACTOR})"
                    ))
                } else if live_spend.is_some_and(|c| c >= ceiling) {
                    Some(format!(
                        "cell cost ceiling reached (live ${:.4} ≥ ${ceiling:.2})",
                        live_spend.unwrap_or(0.0)
                    ))
                } else if live_spend.is_none() && est.is_some_and(|c| c >= ceiling * 3.0) {
                    Some(format!(
                        "cell cost belt reached (est ${:.4} ≥ 3×${ceiling:.2}, no live poll yet)",
                        est.unwrap_or(0.0)
                    ))
                } else {
                    None
                };
                if let Some(reason) = trip {
                    eprintln!("elmer_battery: watchdog cancel — {reason}");
                    *reason_slot.lock().expect("cancel-reason lock poisoned") =
                        Some(reason);
                    session.cancel_and_abort().await;
                    return;
                }
            }
        })
    };

    // ── The measured run, per condition arm ─────────────────────────────────
    // Both arms feed the SAME `sink`/`Meters`, so the cross-arm token/turn
    // comparison is apples-to-apples.
    // Ladder-2 revise phase overrides the frozen corpus prompt with the
    // reviewer-critique prompt; otherwise the corpus cell's frozen text is used.
    let effective_prompt = cli
        .prompt_override
        .clone()
        .unwrap_or_else(|| entry.prompt.clone());
    let outcome = match cli.arm {
        // Base: one send over the operator's raw prompt. authoring=false — the
        // pure "no workflow" reference arm (Build Carefully skill NOT injected).
        Arm::Base => session.send(effective_prompt.clone(), false, sink).await,
        // MatchedControl: Base's single send, but the prompt is augmented with
        // the deterministic affordance catalog (see `matched_control_prompt`).
        // authoring=false — this is a user-prompt catalog control, distinct from
        // the Build Carefully system-prompt skill (the +Skill arm is P5).
        Arm::MatchedControl => {
            let prompt = matched_control_prompt(&effective_prompt, &routines_state);
            session.send(prompt, false, sink).await
        }
        // Skill: Base's single send with authoring ON — compose_system_prompt
        // injects the routine invariant + authoring skill into the system prompt.
        // The only difference from Base is the skill, so the lift is confound-free.
        Arm::Skill => session.send(effective_prompt.clone(), true, sink).await,
    };

    watchdog_stop.cancel();
    let _ = watchdog.await;

    // Credits AFTER is best-effort: a failed query degrades to the flagged
    // token estimate in the ledger, never a silent zero.
    let credits_after = match fetch_credits(http, origin, api_key).await {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("elmer_battery: credits-after query failed ({e}); booking token estimate");
            None
        }
    };

    let cancel_reason = cancel_reason
        .lock()
        .expect("cancel-reason lock poisoned")
        .clone();
    Ok(CellResult {
        outcome,
        cancel_reason,
        credits_before,
        credits_after,
        pricing,
        tool_schema_sha256,
    })
}

// ---------------------------------------------------------------------------
// MatchedControl — the confound-buster (Task 13b Part 4)
// ---------------------------------------------------------------------------

/// Build the MatchedControl prompt: the operator's raw prompt, prefixed with the
/// deterministic affordance catalog Full's Feasibility phase surfaces.
///
/// ## Interpretation (the design leaves the exact tool/prompt delta open)
///
/// MatchedControl augments the prompt with the deterministic affordance
/// catalog (`build_affordance_catalog`). Two judgment calls, stated here
/// rather than made silently:
///
/// 1. **Family set.** `build_affordance_catalog(actions, families)` filters
///    ACTION names (`radio.connect` → family `radio`) by `families`. The shipped
///    manifest's `allowedToolFamilies` is `["routines"]` — but that is an
///    MCP-*tool* family (`routines_*`), NOT an action family, so passing it would
///    match no actions and hit `CatalogError::Empty`. To surface the affordance
///    surface the agent reasons over, this enumerates the DISTINCT action
///    families present in the live registry and builds the catalog over all of
///    them. That yields the full, deterministic action catalog projected to
///    `AffordanceAction`.
/// 2. **Tool set.** MatchedControl holds the tool surface constant with Base
///    (the full router surface behind `ObservedInvoker`) — the ONLY delta from
///    Base is the affordance catalog in the prompt, isolating the
///    affordance-catalog effect.
///
/// If the catalog cannot be built (an empty registry — never true in the wired
/// battery), MatchedControl degrades to the raw prompt (logged), never crashes.
fn matched_control_prompt(raw_prompt: &str, routines_state: &RoutinesState) -> String {
    let actions = list_actions(routines_state);
    // Distinct action families, in first-seen order (deterministic).
    let mut families: Vec<String> = Vec::new();
    for a in &actions {
        let family = a.name.split('.').next().unwrap_or(a.name.as_str()).to_string();
        if !families.contains(&family) {
            families.push(family);
        }
    }

    match build_affordance_catalog(&actions, &families) {
        Ok(catalog) => match serde_json::to_string_pretty(&catalog) {
            Ok(catalog_json) => format!(
                "Available affordances (the actions this station's routines can use, \
                 with their capability flags, parameters, and outputs):\n\n{catalog_json}\n\n\
                 Using only those affordances, respond to the following request:\n\n{raw_prompt}"
            ),
            Err(e) => {
                eprintln!(
                    "elmer_battery: MatchedControl catalog serialize failed ({e}); \
                     falling back to the raw prompt"
                );
                raw_prompt.to_string()
            }
        },
        Err(e) => {
            eprintln!(
                "elmer_battery: MatchedControl affordance catalog empty ({e}); \
                 falling back to the raw prompt"
            );
            raw_prompt.to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — pure logic only (no tauri app, no network)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parity: the wrapper is observation-only ─────────────────────────────

    /// tuxlink-y9a6l: the harness has NO tool policy. `ObservedInvoker` passes
    /// every call to the inner invoker verbatim and meters production-gate
    /// denials without altering them. A synthetic harness denial (the removed
    /// allowlist) must never reappear: the ONLY way a model sees a denial is
    /// the inner invoker returning one.
    #[tokio::test]
    async fn observed_invoker_is_pass_through_and_meters_production_denials() {
        use std::sync::atomic::Ordering;

        // Stub inner invoker: "gated" -> Denied (a production consent gate),
        // anything else -> Ok. Stands in for InProcessMcpInvoker, which needs
        // a full tauri app; the wrapper's contract is what is under test.
        struct StubInner;
        #[async_trait]
        impl ToolInvoker for StubInner {
            fn tools(&self) -> &[ToolSpec] {
                &[]
            }
            async fn invoke(
                &self,
                call: &ToolCall,
                _authority: CallAuthority,
                _cancel: &CancellationToken,
            ) -> ToolOutcome {
                if call.name == "gated" {
                    ToolOutcome::Denied("egress guard is not armed".into())
                } else {
                    ToolOutcome::Ok(serde_json::json!({"ok": true}))
                }
            }
        }

        /// Same shape as ObservedInvoker but generic over the inner invoker so
        /// the test can inject the stub. Mirrors the production wrapper's
        /// logic; if the two drift, this test loses meaning — keep in sync.
        struct Wrapper<I: ToolInvoker> {
            inner: I,
            meters: Arc<Meters>,
        }
        #[async_trait]
        impl<I: ToolInvoker + Sync> ToolInvoker for Wrapper<I> {
            fn tools(&self) -> &[ToolSpec] {
                self.inner.tools()
            }
            async fn invoke(
                &self,
                call: &ToolCall,
                authority: CallAuthority,
                cancel: &CancellationToken,
            ) -> ToolOutcome {
                self.meters.tool_calls.fetch_add(1, Ordering::SeqCst);
                let outcome = self.inner.invoke(call, authority, cancel).await;
                if matches!(outcome, ToolOutcome::Denied(_)) {
                    self.meters.denied_calls.fetch_add(1, Ordering::SeqCst);
                }
                outcome
            }
        }

        let meters = Arc::new(Meters::default());
        let w = Wrapper { inner: StubInner, meters: Arc::clone(&meters) };
        let cancel = CancellationToken::new();

        // Formerly-allowlisted AND formerly-denied names both pass through Ok.
        for name in ["routines_save", "message_send", "routines_run", "cms_connect"] {
            let call = ToolCall::new(name, serde_json::json!({}));
            match w.invoke(&call, CallAuthority::Agent, &cancel).await {
                ToolOutcome::Ok(_) => {}
                other => panic!("{name} must pass through to inner Ok, got {other:?}"),
            }
        }
        assert_eq!(meters.denied_calls.load(Ordering::SeqCst), 0);

        // A production gate's Denied passes through UNALTERED and is metered.
        let call = ToolCall::new("gated", serde_json::json!({}));
        match w.invoke(&call, CallAuthority::Agent, &cancel).await {
            ToolOutcome::Denied(reason) => {
                assert_eq!(reason, "egress guard is not armed", "reason must not be rewritten");
            }
            other => panic!("production Denied must pass through, got {other:?}"),
        }
        assert_eq!(meters.denied_calls.load(Ordering::SeqCst), 1);
        assert_eq!(meters.tool_calls.load(Ordering::SeqCst), 5);
    }

    /// tuxlink-g31en: a credits-query failure is fatal ONLY for OpenRouter
    /// (the credits API is its spend record + $45 ledger gate). Local /
    /// non-OpenRouter endpoints have no credits API by design, so their
    /// failure falls back to a zero baseline instead of aborting the cell.
    #[test]
    fn credits_failure_fatal_only_for_openrouter() {
        assert!(credits_failure_is_fatal("https://openrouter.ai"));
        assert!(credits_failure_is_fatal("https://openrouter.ai/api/v1"));
        assert!(!credits_failure_is_fatal("https://inference.twin-bramble.ts.net"));
        assert!(!credits_failure_is_fatal("http://localhost:8000"));
        assert!(!credits_failure_is_fatal("https://api.example.com"));
    }

    // ── Ledger math ─────────────────────────────────────────────────────────

    #[test]
    fn ledger_refuses_at_hard_stop() {
        let mut ledger = Ledger::default();
        assert!(!ledger_blocks(&ledger), "fresh ledger must not block");
        ledger.total_spend_usd = 44.99;
        assert!(!ledger_blocks(&ledger), "under the cap must not block");
        ledger.total_spend_usd = 45.0;
        assert!(ledger_blocks(&ledger), "AT the cap must block (>= semantics)");
        ledger.total_spend_usd = 60.0;
        assert!(ledger_blocks(&ledger), "over the cap must block");
    }

    #[test]
    fn ledger_round_trips_and_accumulates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ledger.json");

        // Missing ledger reads as a fresh default, not an error.
        let mut ledger = load_ledger(&path).expect("missing ledger must default");
        assert_eq!(ledger.total_spend_usd, 0.0);
        assert!(ledger.cells.is_empty());

        ledger.total_spend_usd += 1.25;
        ledger.cells.push(LedgerCell {
            ts: "2026-07-21T00:00:00Z".into(),
            model: "test/model".into(),
            prompt: "P2".into(),
            delta_usd: 1.25,
            estimated: false,
        });
        save_ledger(&path, &ledger).expect("save");

        let back = load_ledger(&path).expect("reload");
        assert_eq!(back.cells.len(), 1);
        assert!((back.total_spend_usd - 1.25).abs() < f64::EPSILON);
    }

    // ── Corpus parsing ──────────────────────────────────────────────────────

    fn repo_corpus_path() -> PathBuf {
        // CARGO_MANIFEST_DIR = src-tauri; the corpus is tracked at the repo
        // root's tests/battery/.
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/battery/corpus.json")
    }

    #[test]
    fn corpus_loads_with_the_full_prompt_ladder_and_global_predicates() {
        let (corpus, sha) = load_corpus(&repo_corpus_path()).expect("corpus must load");
        assert_eq!(sha.len(), 64, "corpus sha must be full sha256 hex");
        assert!(
            !corpus.global_predicates.is_empty(),
            "global predicates must be present"
        );
        let ids: Vec<&str> = corpus.prompts.iter().map(|p| p.id.as_str()).collect();
        // The 7 prescriptive (TaskRabbit) tasks plus the 11 operator-authored
        // intent-deciphering rungs (2 Assistant, 3 Collaborator, 3 Elmer, 3
        // Elmer-ultra) — Task 14a.
        for expected in [
            "P2", "P1", "S1", "S2", "S4", "S3", "P3", "A1", "A2", "C1", "C2", "C3", "E1", "E2",
            "E3", "EU1", "EU2", "EU3",
        ] {
            assert!(ids.contains(&expected), "corpus must carry prompt {expected}");
        }
        assert_eq!(corpus.prompts.len(), 18, "the 7-prompt ladder + 11 rung tasks");
        // S2 is the only preseeded cell.
        for p in &corpus.prompts {
            assert_eq!(
                p.preseed.is_some(),
                p.id == "S2",
                "preseed must be present exactly on S2 (saw {})",
                p.id
            );
            assert!(!p.prompt.trim().is_empty(), "prompt {} has frozen text", p.id);
            assert!(!p.predicates.is_empty(), "prompt {} has predicates", p.id);
        }
    }

    #[test]
    fn preseed_def_is_valid_per_routines_types() {
        let def = tuxlink_routines::types::RoutineDef::parse(PRESEED_NEAREST_40M_DIAL)
            .expect("preseed def must parse as a v1 RoutineDef");
        assert_eq!(def.routine, PRESEED_NAME, "file-stem contract: body name matches");
        assert_eq!(def.tracks.len(), 1);
        assert_eq!(def.tracks[0].steps.len(), 2, "connect walk + winner log");
    }

    // ── Scratch preflight path assertion ────────────────────────────────────

    #[test]
    fn preflight_accepts_paths_under_root_and_rejects_escapes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().canonicalize().expect("canonicalize");

        // Existing file under the root.
        let inside = root.join("config/config.json");
        std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
        std::fs::write(&inside, b"{}").unwrap();
        assert!(path_is_under(&inside, &root));

        // Not-yet-created path under the root still passes.
        let future = root.join("routines/new-def.json");
        assert!(path_is_under(&future, &root));

        // Sibling temp dir fails.
        let other = tempfile::tempdir().expect("tempdir");
        assert!(!path_is_under(&other.path().join("config.json"), &root));

        // Dot-dot escape fails even though the prefix looks right.
        let escape = root.join("config/../../outside.json");
        assert!(!path_is_under(&escape, &root));

        // assert_under accepts any of several roots.
        let bundle = tempfile::tempdir().expect("tempdir");
        let bundle_root = bundle.path().canonicalize().unwrap();
        assert!(assert_under(
            "test",
            &bundle_root.join("outcome.json"),
            &[root.as_path(), bundle_root.as_path()],
        )
        .is_ok());
        assert!(assert_under("test", Path::new("/etc/passwd"), &[root.as_path()]).is_err());
    }

    // ── CLI parsing ─────────────────────────────────────────────────────────

    #[test]
    fn cli_parses_required_and_defaults() {
        let args: Vec<String> = [
            "--corpus", "tests/battery/corpus.json",
            "--model", "openai/gpt-5.5",
            "--prompt", "P2",
            "--out", "battery-results/x/P2",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let cli = parse_cli(&args).expect("minimal args must parse");
        assert_eq!(cli.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(cli.turn_cap, DEFAULT_TURN_CAP);
        assert_eq!(cli.turn_timeout_secs, DEFAULT_TURN_TIMEOUT_SECS);
        assert!((cli.cell_ceiling_usd - DEFAULT_CELL_CEILING_USD).abs() < f64::EPSILON);
        assert!((cli.temperature - DEFAULT_TEMPERATURE).abs() < f32::EPSILON);
        assert!(cli.ledger.is_none());
        // --arm defaults to Base (preserves today's single-send behavior).
        assert_eq!(cli.arm, Arm::Base);

        let missing = parse_cli(&["--corpus".to_string(), "x".to_string()]);
        assert!(missing.is_err(), "missing required args must fail");

        let unknown = parse_cli(&["--frobnicate".to_string()]);
        assert!(unknown.is_err(), "unknown flags must fail loudly");
    }

    // ── Arm parsing + projection (Task 13b) ─────────────────────────────────

    #[test]
    fn arm_from_str_parses_the_three_conditions_and_rejects_unknown() {
        assert_eq!("base".parse::<Arm>().unwrap(), Arm::Base);
        assert_eq!(
            "matched-control".parse::<Arm>().unwrap(),
            Arm::MatchedControl
        );
        assert_eq!("skill".parse::<Arm>().unwrap(), Arm::Skill);
        assert_eq!(Arm::Skill.as_str(), "skill");
        // Unknown / near-miss spellings fail loudly, not silently to Base.
        assert!("Base".parse::<Arm>().is_err(), "parse is case-sensitive");
        assert!("matched_control".parse::<Arm>().is_err(), "hyphen, not underscore");
        assert!("".parse::<Arm>().is_err());
        assert!("control".parse::<Arm>().is_err());
        // The discarded workflow-engine arm is no longer a valid condition.
        assert!("full".parse::<Arm>().is_err(), "the Full arm was removed");
    }

    #[test]
    fn cli_parses_explicit_arm() {
        let args: Vec<String> = [
            "--corpus", "tests/battery/corpus.json",
            "--model", "openai/gpt-5.5",
            "--prompt", "P2",
            "--out", "battery-results/x/P2",
            "--arm", "matched-control",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let cli = parse_cli(&args).expect("explicit --arm must parse");
        assert_eq!(cli.arm, Arm::MatchedControl);

        // A bad --arm value is a loud parse error, not a silent default.
        let bad: Vec<String> = [
            "--corpus", "c", "--model", "m", "--prompt", "P2", "--out", "o", "--arm", "sideways",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert!(parse_cli(&bad).is_err(), "unknown --arm value must fail");
    }

    #[test]
    fn list_arms_output_is_every_parseable_arm() {
        // `--list-arms` prints `Arm::ALL.as_str()`; every line it prints MUST
        // round-trip back through FromStr, so a launcher can trust the list to
        // preflight requested arms. Skill (the +Skill lift arm) must be present.
        for arm in Arm::ALL {
            assert_eq!(
                arm.as_str().parse::<Arm>().unwrap(),
                arm,
                "{} must round-trip",
                arm.as_str()
            );
        }
        assert!(
            Arm::ALL.iter().any(|a| a.as_str() == "skill"),
            "the +Skill lift arm must be listed"
        );
    }
}
