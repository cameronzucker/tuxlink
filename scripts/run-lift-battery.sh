#!/usr/bin/env bash
#
# run-lift-battery.sh — the ONE confound-proof way to run the Build-Carefully
# lift battery (tuxlink-t3jci) on a local model. Encodes, as ENFORCED TOOLING,
# the methodology a prose handoff cannot reliably carry. If you are running the
# lift, run THIS — do not hand-roll a launcher.
#
# WHY THIS EXISTS (2026-07-23 incident, tuxlink-m0n38): a stale hand-rolled
# launcher requested a `skill` arm a pre-#1248 binary did not have; the mismatch
# only surfaced AFTER burning model time, and the resulting cells spanned
# different commits — not a clean Base-vs-Skill comparison. Codex GPT-5.6-sol
# root-caused it: interrogate the actual binary, pin the SHA, never mix cells
# across builds. This script makes all of that non-optional.
#
# CONFOUND GUARDS (each is enforced, not documented-and-hoped):
#   1. CLEAN TREE: refuses to run on a dirty git tree (a run's provenance SHA
#      must actually describe the code). Override only for dev smoke via
#      TUXLINK_LIFT_ALLOW_DIRTY=1 (recorded loudly in the manifest).
#   2. PINNED SHA: the sweep dir is NAMESPACED BY THE BUILD SHA
#      (battery-results/lift-<sha>/), so cells from different builds CANNOT be
#      mixed into one comparison — the classic confound is structurally impossible.
#   3. ONE BINARY, ALL CELLS: builds once, runs every prompt x arm from that
#      single binary. A valid Base-vs-Skill lift needs identical binary/config.
#   4. ARM PREFLIGHT: asserts every requested arm is in the binary's --list-arms
#      output BEFORE any model call. A missing arm aborts loudly, spending nothing.
#   5. ENDPOINT PREFLIGHT: verifies the model endpoint answers before starting.
#   6. ARTIFACT ACCOUNTING: per cell, records tool-use artifacts (deserialize
#      errors, panics, arm errors) SEPARATELY from the task outcome, so scoring
#      never conflates a tool-call-formatting artifact with task quality.
#   7. MANIFEST: writes run-manifest.json (sha, branch, dirty, model, endpoint,
#      arms, prompts, params, utc) for full reproducibility.
#
# USAGE:
#   scripts/run-lift-battery.sh [--preflight-only]
# Env overrides (all optional):
#   MODEL, ENDPOINT, PROMPTS, ARMS, TURN_CAP, TEMPERATURE, CARGO, CORPUS,
#   OPENROUTER_API_KEY (required for a real run; keyless local vLLM = any value),
#   TUXLINK_LIFT_ALLOW_DIRTY=1 (dev smoke only).
#
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODEL="${MODEL:-qwen35-122b-nvfp4}"
ENDPOINT="${ENDPOINT:-https://inference.twin-bramble.ts.net/v1/chat/completions}"
PROMPTS="${PROMPTS:-C2 S3 EU3}"
ARMS="${ARMS:-base skill}"
TURN_CAP="${TURN_CAP:-40}"
TEMPERATURE="${TEMPERATURE:-0.2}"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
CORPUS="${CORPUS:-$ROOT/tests/battery/corpus.json}"
PREFLIGHT_ONLY=0
[[ "${1:-}" == "--preflight-only" ]] && PREFLIGHT_ONLY=1

die() { echo "run-lift-battery: FATAL: $*" >&2; exit 1; }
step() { echo ">>> $*"; }

# --- Guard 1: clean git tree (provenance must be real) ----------------------
DIRTY=0
if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
  DIRTY=1
  if [[ "${TUXLINK_LIFT_ALLOW_DIRTY:-0}" == "1" ]]; then
    echo "run-lift-battery: WARNING: dirty tree; TUXLINK_LIFT_ALLOW_DIRTY=1 set. This run is NOT reproducible from its SHA — dev smoke only." >&2
  else
    die "working tree is dirty; an authoritative run's SHA must describe the code. Commit/stash, or set TUXLINK_LIFT_ALLOW_DIRTY=1 for a dev smoke."
  fi
fi
SHA="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
[[ "$BRANCH" != "main" ]] && echo "run-lift-battery: NOTE: HEAD is '$BRANCH', not main. The authoritative lift should run from a merged main SHA (Codex 2026-07-23)." >&2

# --- Prereqs ----------------------------------------------------------------
[[ -x "$CARGO" ]] || die "cargo not found/executable at $CARGO (set CARGO=...). On R2 this is ~/.cargo/bin/cargo (1.96); system cargo 1.75 fails edition2024."
command -v xvfb-run >/dev/null || die "xvfb-run not found (the battery needs a virtual display)."
[[ -f "$CORPUS" ]] || die "corpus not found at $CORPUS."

# --- Build once (all cells share this binary) -------------------------------
BIN="$ROOT/src-tauri/target/debug/elmer_battery"
step "building elmer_battery + elmer_score from $SHA (branch $BRANCH, dirty=$DIRTY)"
"$CARGO" build --manifest-path src-tauri/Cargo.toml --bin elmer_battery --bin elmer_score \
  || die "build failed."
[[ -x "$BIN" ]] || die "binary missing after build: $BIN"

# --- Guard 4: arm preflight against THIS binary -----------------------------
SUPPORTED="$("$BIN" --list-arms 2>/dev/null || true)"
[[ -n "$SUPPORTED" ]] || die "binary does not support --list-arms; rebuild from a version that does (tuxlink-m0n38+)."
step "binary supports arms: $(echo $SUPPORTED | tr '\n' ' ')"
for ARM in $ARMS; do
  grep -qx "$ARM" <<<"$SUPPORTED" \
    || die "requested arm '$ARM' is NOT in this binary (has: $(echo $SUPPORTED | tr '\n' ' ')). Aborting before any model call."
done

# --- Guard 2: SHA-namespaced sweep (cannot mix builds) ----------------------
SWEEP="$ROOT/battery-results/lift-$SHA"
mkdir -p "$SWEEP"

# --- Guard 5: endpoint preflight --------------------------------------------
MODELS_URL="${ENDPOINT%/chat/completions}/models"
if [[ "$PREFLIGHT_ONLY" == "0" ]]; then
  [[ -n "${OPENROUTER_API_KEY:-}" ]] || die "OPENROUTER_API_KEY unset (keyless local vLLM: export OPENROUTER_API_KEY=local-vllm-nokey)."
  code="$(curl -sS -m 20 -o /dev/null -w '%{http_code}' "$MODELS_URL" 2>/dev/null || echo 000)"
  [[ "$code" == "200" ]] || die "endpoint $MODELS_URL not reachable (HTTP $code)."
  step "endpoint reachable (HTTP 200): $ENDPOINT"
fi

# --- Guard 7: manifest ------------------------------------------------------
python3 - "$SWEEP/run-manifest.json" <<PY
import json, sys, os, subprocess, time
utc = subprocess.run(["date","-u","+%Y-%m-%dT%H:%M:%SZ"], capture_output=True, text=True).stdout.strip()
json.dump({
  "sha": "$SHA", "branch": "$BRANCH", "dirty": bool($DIRTY),
  "model": "$MODEL", "endpoint": "$ENDPOINT",
  "arms": "$ARMS".split(), "prompts": "$PROMPTS".split(),
  "turn_cap": $TURN_CAP, "temperature": $TEMPERATURE,
  "utc": utc, "preflight_only": bool($PREFLIGHT_ONLY),
}, open(sys.argv[1],"w"), indent=2)
print(">>> wrote", sys.argv[1])
PY

if [[ "$PREFLIGHT_ONLY" == "1" ]]; then
  step "PREFLIGHT-ONLY: build + arm preflight + manifest OK. No model calls made. Sweep: $SWEEP"
  exit 0
fi

# --- Run: every prompt x arm from the ONE binary ----------------------------
for ST in $PROMPTS; do
  for ARM in $ARMS; do
    OUT="$SWEEP/$ARM/$ST"
    if [[ -f "$OUT/outcome.json" ]]; then echo "SKIP $ARM/$ST (exists in this SHA's sweep)"; continue; fi
    mkdir -p "$OUT"
    step "CELL $MODEL $ARM x $ST (sha $SHA) @ $(date -u +%H:%M:%SZ)"
    xvfb-run -a "$BIN" --corpus "$CORPUS" --model "$MODEL" --endpoint "$ENDPOINT" \
      --arm "$ARM" --prompt "$ST" --out "$OUT" --ledger "$SWEEP/ledger.json" \
      --temperature "$TEMPERATURE" --turn-cap "$TURN_CAP" \
      2>&1 | tee "$OUT/harness.log" | grep -E "CELL|outcome|error" || true
    # Guard 6: tool-use artifacts recorded SEPARATELY from task outcome.
    ds=$(grep -rc "invalid type: string" "$OUT" 2>/dev/null | grep -v ':0' | wc -l)
    pn=$(grep -rc "panicked" "$OUT/harness.log" 2>/dev/null || echo 0)
    am=$(grep -rc "unknown --arm" "$OUT/harness.log" 2>/dev/null || echo 0)
    oc=$(python3 -c "import json;print(json.load(open('$OUT/outcome.json'))['outcome'])" 2>/dev/null || echo MISSING)
    python3 -c "import json;json.dump({'deserialize_errors':int('$ds' or 0),'panics':int('$pn' or 0),'arm_errors':int('$am' or 0)},open('$OUT/tool-use-artifacts.json','w'))" 2>/dev/null || true
    echo "--- $ARM/$ST outcome=$oc  artifacts: deserialize=$ds panics=$pn arm_errors=$am"
  done
done
step "LIFT COMPLETE — sha=$SHA sweep=$SWEEP. Score with: $ROOT/src-tauri/target/debug/elmer_score (see manifest for provenance)."
