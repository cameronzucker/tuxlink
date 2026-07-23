#!/usr/bin/env bash
# Routine CI battery — matrix orchestration runner (Task 14b).
#
# Loops {model} × {arm} × {corpus prompt}, invoking the `elmer_battery` binary
# once per cell (the binary runs ONE cell/invocation by design; the matrix and
# judging are external — see elmer_battery.rs module doc). Runs on R2, NEVER the
# Pi. Idempotent: a cell whose bundle already has outcome.json is skipped, so an
# interrupted sweep resumes cleanly.
#
# The specific model set / endpoints / judge model are RUN-GATE decisions — this
# script is parameterized; it hardcodes nothing about which models run.
#
# Usage:
#   run-matrix.sh --models <models.tsv> --corpus <corpus.json> --out <root> \
#       [--arms "base matched-control full"] \
#       [--only "A1 C2 EU3"]   (subset of corpus prompt ids; default = all) \
#       [--bin <path-to-elmer_battery>] \
#       [--cell-ceiling-usd N] [--turn-cap N] [--turn-timeout-secs N] \
#       [--dry-run]
#
# models.tsv — one model per line, TAB-separated, comments (#) and blank lines
# ignored:
#   <label>\t<model-id>\t<endpoint>\t<API_KEY_ENV_VAR>
# The <endpoint> is the FULL OpenAI-compatible chat-completions URL that
# AgentEndpoint::parse expects (.../v1/chat/completions), NOT the bare /v1 base;
# elmer_battery derives the origin from it for the /v1/models + credits GETs.
# e.g. (qwen row verified live against twin-bramble 2026-07-22):
#   qwen35     qwen35-122b-nvfp4    https://inference.twin-bramble.ts.net/v1/chat/completions  TWIN_BRAMBLE_KEY
#   glm52      z-ai/glm-5.2         https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
#   gptoss120b openai/gpt-oss-120b  https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
#   nemotron   nvidia/nemotron-...  https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
#   sonnet5    anthropic/claude-... https://openrouter.ai/api/v1/chat/completions              OPENROUTER_API_KEY
#
# The named API_KEY_ENV_VARs must be EXPORTED before running (keys sourced from
# the Pi keyring and piped to env, NEVER written to disk). A model whose key env
# var is empty/unset is skipped with a loud warning (fail-loud, not silent).
set -euo pipefail

ARMS="base matched-control full"
ONLY=""
BIN="elmer_battery"
CORPUS=""
MODELS=""
OUTROOT=""
DRY=0
CELL_CEILING=""
TURN_CAP=""
TURN_TIMEOUT=""

die() { echo "run-matrix: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --models)  MODELS="$2"; shift 2;;
    --corpus)  CORPUS="$2"; shift 2;;
    --out)     OUTROOT="$2"; shift 2;;
    --arms)    ARMS="$2"; shift 2;;
    --only)    ONLY="$2"; shift 2;;
    --bin)     BIN="$2"; shift 2;;
    --cell-ceiling-usd) CELL_CEILING="$2"; shift 2;;
    --turn-cap)         TURN_CAP="$2"; shift 2;;
    --turn-timeout-secs) TURN_TIMEOUT="$2"; shift 2;;
    --dry-run) DRY=1; shift;;
    *) die "unknown arg: $1";;
  esac
done

[ -n "$MODELS" ]  || die "--models <models.tsv> required"
[ -n "$CORPUS" ]  || die "--corpus <corpus.json> required"
[ -n "$OUTROOT" ] || die "--out <root> required"
[ -f "$MODELS" ]  || die "models file not found: $MODELS"
[ -f "$CORPUS" ]  || die "corpus not found: $CORPUS"
command -v jq >/dev/null || die "jq is required to read the corpus"

# Prompt ids: all of the corpus, or the --only subset (validated against it).
mapfile -t ALL_IDS < <(jq -r '.prompts[].id' "$CORPUS")
if [ -n "$ONLY" ]; then
  IDS=()
  for want in $ONLY; do
    found=0
    for have in "${ALL_IDS[@]}"; do [ "$have" = "$want" ] && found=1 && break; done
    [ "$found" = 1 ] || die "--only names '$want', not in the corpus"
    IDS+=("$want")
  done
else
  IDS=("${ALL_IDS[@]}")
fi

# Count the plan up front so a big matrix is never a surprise.
n_models=$(grep -cvE '^\s*(#|$)' "$MODELS" || true)
n_arms=$(wc -w <<<"$ARMS")
echo "run-matrix: ${n_models} model(s) × ${n_arms} arm(s) × ${#IDS[@]} prompt(s) = $(( n_models * n_arms * ${#IDS[@]} )) cells"
echo "run-matrix: arms=[$ARMS] prompts=[${IDS[*]}] out=$OUTROOT"
[ "$DRY" = 1 ] && echo "run-matrix: --dry-run (no cells will run)"

# elmer_battery builds a (windowless) Tauri app that still initializes GTK, so a
# headless box (e.g. R2 over SSH, no DISPLAY) panics at startup. Wrap each cell in
# a per-invocation virtual framebuffer when there is no X display — matching the
# prior R2 run scripts (xvfb-run -a). No-op when a real DISPLAY is present.
WRAP=()
if [ -z "${DISPLAY:-}" ] && command -v xvfb-run >/dev/null 2>&1; then
  WRAP=(xvfb-run -a)
  echo "run-matrix: no DISPLAY — wrapping each cell in 'xvfb-run -a'"
fi

ran=0; skipped=0; failed=0
while IFS=$'\t' read -r label model endpoint keyvar; do
  case "$label" in ''|\#*) continue;; esac   # skip blanks/comments
  [ -n "${keyvar:-}" ] || die "models.tsv row '$label' missing API_KEY_ENV_VAR column"
  key="${!keyvar:-}"
  if [ -z "$key" ]; then
    echo "run-matrix: SKIP model '$label' — env var \$$keyvar is empty/unset (export the key first)" >&2
    continue
  fi
  for arm in $ARMS; do
    for id in "${IDS[@]}"; do
      bundle="$OUTROOT/$label/$arm/$id"
      if [ -f "$bundle/outcome.json" ]; then
        skipped=$((skipped+1)); continue
      fi
      echo "== cell: model=$label arm=$arm prompt=$id -> $bundle"
      [ "$DRY" = 1 ] && { ran=$((ran+1)); continue; }
      mkdir -p "$bundle"
      cmd=("$BIN" --corpus "$CORPUS" --model "$model" --endpoint "$endpoint"
           --prompt "$id" --arm "$arm" --out "$bundle")
      [ -n "$CELL_CEILING" ] && cmd+=(--cell-ceiling-usd "$CELL_CEILING")
      [ -n "$TURN_CAP" ]     && cmd+=(--turn-cap "$TURN_CAP")
      [ -n "$TURN_TIMEOUT" ] && cmd+=(--turn-timeout-secs "$TURN_TIMEOUT")
      # The binary reads the key ONLY from OPENROUTER_API_KEY (that fixed env
      # name for every endpoint, local vLLM or OpenRouter — elmer_battery.rs:936).
      # Set it per-model from this model's key, for this invocation only; never
      # argv, never disk. Local/non-OpenRouter endpoints tolerate a non-OpenRouter
      # key (the credits baseline is non-fatal there, tuxlink-g31en).
      if OPENROUTER_API_KEY="$key" "${WRAP[@]}" "${cmd[@]}"; then ran=$((ran+1)); else
        failed=$((failed+1)); echo "run-matrix: CELL FAILED (continuing): $label/$arm/$id" >&2
      fi
    done
  done
done < "$MODELS"

echo "run-matrix: done — ran=$ran skipped=$skipped failed=$failed"
[ "$failed" = 0 ] || echo "run-matrix: $failed cell(s) failed; inspect their bundles + stderr" >&2
