#!/usr/bin/env bash
# Battery stage driver (bd tuxlink-hwgdi): run ONE corpus stage across the
# five battery models, sequentially, one elmer_battery invocation per cell.
# Stage-gated ladder discipline (operator 2026-07-21): a stage is judged and
# addressed before the next runs — this script never advances stages.
#
# Usage: OPENROUTER_API_KEY=... scripts/battery-stage.sh <prompt-id> <sweep-id> [repo-root]
# Bundles land under <repo-root>/battery-results/<sweep-id>/<model>/<prompt-id>/.
set -euo pipefail

PROMPT_ID="${1:?usage: battery-stage.sh <prompt-id> <sweep-id> [repo-root]}"
SWEEP_ID="${2:?usage: battery-stage.sh <prompt-id> <sweep-id> [repo-root]}"
ROOT="${3:-$(cd "$(dirname "$0")/.." && pwd)}"
: "${OPENROUTER_API_KEY:?OPENROUTER_API_KEY must be set (env-at-invocation; never on disk)}"

BIN="$ROOT/src-tauri/target/debug/elmer_battery"
CORPUS="$ROOT/tests/battery/corpus.json"
RESULTS="$ROOT/battery-results/$SWEEP_ID"
LEDGER="$ROOT/battery-results/ledger.json"

# Order: cheapest first so an early harness defect burns cents, not dollars.
# Fable 5 DISCONTINUED from the ladder (operator 2026-07-21: >95% of account
# usage was Fable; disproportionate at its price tier). Four-model ladder.
MODELS=(
  "qwen/qwen3.5-122b-a10b"
  "z-ai/glm-5.2"
  "anthropic/claude-sonnet-5"
  "openai/gpt-5.5"
)

for MODEL in "${MODELS[@]}"; do
  SAFE_MODEL="${MODEL//\//_}"
  OUT="$RESULTS/$SAFE_MODEL/$PROMPT_ID"
  if [[ -f "$OUT/outcome.json" ]]; then
    echo "SKIP $MODEL $PROMPT_ID (outcome.json exists — delete the bundle to re-run)"
    continue
  fi
  mkdir -p "$OUT"
  echo "=== CELL $MODEL x $PROMPT_ID ==="
  # Per-cell failure must not kill the stage: a crashed cell is itself data.
  if ! xvfb-run -a "$BIN" \
    --corpus "$CORPUS" \
    --model "$MODEL" \
    --prompt "$PROMPT_ID" \
    --out "$OUT" \
    --ledger "$LEDGER" \
    2>&1 | tee "$OUT/harness.log"; then
    echo "CELL-FAILED $MODEL $PROMPT_ID (see $OUT/harness.log)"
  fi
done

echo "=== STAGE $PROMPT_ID COMPLETE — bundles in $RESULTS ==="
