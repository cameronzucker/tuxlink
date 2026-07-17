#!/usr/bin/env bash
# M2a spike runner — one worker attempt for one cell.
# Usage: run-cell.sh <cell> [attempt]
#   cell ∈ pi-cn-r3 | mini-cn-r3 | pi-q122-r3 | mini-q122-r3 | pi-e122-r5 | mini-e122-r5
# Wraps the frozen ladder brief in the shared job/report contract (NO Codex R2
# harness block — see README §Treatment decisions), dispatches the harness with
# per-invocation provider config, enforces the 30-min cap, tees the transcript
# into the worker's .superpowers/sdd.
set -u
CELL="$1"; ATTEMPT="${2:-1}"
SPIKE_DIR="$(cd "$(dirname "$0")" && pwd)"
BRIEFS="${SPIKE_DIR}/../2026-07-16-difficulty-ladder/briefs"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-m2a-${CELL}"
HARN="/home/administrator/.local/share/m2a-harnesses"
NODE22="${HARN}/node22/bin/node"
PI_CLI="${HARN}/pi/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"
MINI="/home/administrator/.local/bin/mini"

case "$CELL" in
  *-r3) RUNG=3 ;;
  *-r5) RUNG=5 ;;
  *) echo "unknown cell: $CELL" >&2; exit 2 ;;
esac
HARNESS="${CELL%%-*}"
BRIEF="${BRIEFS}/rung-${RUNG}.md"
SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-${HARNESS}-transcript${SUFFIX}.txt"

JOB_HEAD="You are implementing a task in the tuxlink repository. Your working
directory is the repository root of a dedicated checkout; work only there.

## Your job

1. Do exactly what the brief below specifies; where the brief grants design
   freedom, decide and document.
2. Verify with the exact commands the brief lists (plus any you deem
   necessary).
3. Self-review: every brief requirement met? nothing beyond scope? tests
   verify real behavior; output pristine?
4. Write your full report to .superpowers/sdd/rung-${RUNG}-report.md (relative
   to the repo root), then finish.

Per the brief: do NOT run any git command and do NOT commit — the controller
commits."

if [ "$HARNESS" = "pi" ]; then
  TAIL="End your final message with ONLY: Status (DONE | DONE_WITH_CONCERNS |
BLOCKED), a one-line test summary, concerns if any, and the report file path."
else
  TAIL="End your report FILE with a final line: Status: (DONE |
DONE_WITH_CONCERNS | BLOCKED) — one-line test summary; concerns if any. Then
submit using the completion command from your instructions."
fi

PROMPT="${JOB_HEAD}

${TAIL}

## Your brief (requirements of record):

$(cat "$BRIEF")"

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-${CELL} attempt-${ATTEMPT} dispatched" >> "${SDD}/timing.log"

case "$CELL" in
  pi-cn-r3)
    cd "$WT" && SPARK_API_KEY=dummy timeout 1800 "$NODE22" "$PI_CLI" \
      -e "${SPIKE_DIR}/pi-spark.js" --provider spark --model qwen3-coder-next \
      -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
      "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
  pi-q122-r3)
    cd "$WT" && SPARK_API_KEY=dummy timeout 1800 "$NODE22" "$PI_CLI" \
      -e "${SPIKE_DIR}/pi-spark.js" --provider spark --model qwen35-122b-nvfp4 \
      -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
      "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
  pi-e122-r5)
    OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)" || exit 3
    export OPENROUTER_API_KEY
    cd "$WT" && timeout 1800 "$NODE22" "$PI_CLI" \
      --provider openrouter --model "qwen/qwen3.5-122b-a10b" --thinking medium \
      -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
      "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
  mini-cn-r3|mini-q122-r3)
    cd "$WT" && timeout 1800 "$MINI" \
      -c mini_textbased.yaml -c "${SPIKE_DIR}/mini-${CELL#mini-}.yaml" \
      -y --exit-immediately \
      -o "$SDD/rung-${RUNG}-mini-trajectory${SUFFIX}.json" \
      -t "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
  mini-e122-r5)
    OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)" || exit 3
    export OPENROUTER_API_KEY
    cd "$WT" && timeout 1800 "$MINI" \
      -c mini_textbased.yaml -c "${SPIKE_DIR}/mini-e122-r5.yaml" \
      -y --exit-immediately \
      -o "$SDD/rung-${RUNG}-mini-trajectory${SUFFIX}.json" \
      -t "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
esac
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-${CELL} attempt-${ATTEMPT} finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
