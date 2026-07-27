#!/usr/bin/env bash
# Viability spot-check: qwen3.7-max (reasoning-native, API-only frontier) on the
# five hardest cells, via OpenRouter. NOT a capability measurement -- the
# question is only whether a reasoning-native model survives this harness at all
# (tool-call formatting, turn budget, thinking-token volume, cost).
#
# Deliberately writes OUTSIDE battery-results/ladder2 so the dashboard and the
# judge daemon do not see it: both scan the ladder2 tree, and a foreign model's
# bundles would pollute the live matrix mid-run.
#
# Runs against OpenRouter, so it does NOT contend with the Spark and is safe to
# run alongside the in-flight ladder re-run.
#
# env: OPENROUTER_API_KEY
set -uo pipefail

ROOT="$HOME/tuxlink-eig6e-build"
BIN="$ROOT/src-tauri/target/debug/elmer_battery"
SCORE="$ROOT/src-tauri/target/debug/elmer_score"
CORPUS="$ROOT/tests/battery/corpus.json"
OUT="${SPOT_OUT:-$ROOT/battery-results/spot-37max}"   # sibling of ladder2, NOT inside it
ATTEMPTS="${SPOT_ATTEMPTS:-}"   # empty = flat <cell>/; else <cell>/attempt-N per listed N
MODEL="${SPOT_MODEL:-qwen/qwen3.7-max}"
EP="https://openrouter.ai/api/v1/chat/completions"
CELLS="${SPOT_CELLS:-E1 P3 E2 EU2 S4}"
TURNCAP=40
TEMP=0.2
TURNTO="${SPOT_TURN_TIMEOUT:-1800}"
# The per-cell ceiling meters ACCOUNT-WIDE credits (see the 2026-07-25 analysis),
# so parallel cells each observe the SUM of all of them. Set well above the
# expected total spend or they mass-cancel each other.
CEIL="${SPOT_CEILING_USD:-50}"

mkdir -p "$OUT"
LOG="$OUT/run.log"
PIDS="$OUT/pids"          # exact PIDs, so the cost watchdog never pattern-matches
: > "$PIDS"
log(){ echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$LOG"; }

run_one(){
  local cell="$1" disp="$2" att="${3:-}"
  local out="$OUT/$cell"
  [ -n "$att" ] && out="$OUT/$cell/attempt-$att"
  [ -f "$out/score.json" ] && { log "$cell already scored, skipping"; return 0; }
  rm -rf "$out"; mkdir -p "$out"
  local t0 t1; t0=$(date +%s)
  xvfb-run -n "$disp" "$BIN" --corpus "$CORPUS" --model "$MODEL" --endpoint "$EP" \
      --prompt "$cell" --arm base --turn-cap "$TURNCAP" --temperature "$TEMP" \
      --turn-timeout-secs "$TURNTO" --cell-ceiling-usd "$CEIL" \
      --out "$out" >> "$out/harness.log" 2>&1
  t1=$(date +%s)
  "$SCORE" --root "$out" --corpus "$CORPUS" >/dev/null 2>&1
  log "$cell${att:+/attempt-$att} done in $((t1-t0))s: $(python3 -c 'import json,sys;o=json.load(open(sys.argv[1]));print("%s turns=%s ptok=%s etok=%s"%(o["outcome"],o.get("provider_turns"),o.get("prompt_tokens"),o.get("eval_tokens")))' "$out/outcome.json" 2>/dev/null || echo "no outcome.json")"
}

log "SPOT START model=$MODEL cells=[$CELLS] turn_timeout=${TURNTO}s ceiling=\$$CEIL max_run=${TUXLINK_MAX_RUN_SECS:-1800(default)}s"
i=0
for cell in $CELLS; do
  if [ -z "$ATTEMPTS" ]; then
    i=$((i+1)); run_one "$cell" $((300+i)) & echo $! >> "$PIDS"
  else
    for a in $ATTEMPTS; do
      i=$((i+1)); run_one "$cell" $((300+i)) "$a" & echo $! >> "$PIDS"
    done
  fi
done
wait
log "SPOT COMPLETE"
