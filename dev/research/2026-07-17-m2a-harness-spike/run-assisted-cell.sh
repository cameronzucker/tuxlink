#!/usr/bin/env bash
# Assisted re-run dispatcher — the fair-grade round after the M2 extensions.
# Usage: run-assisted-cell.sh <mistralor-r3-v|mistralor-r5-v|mistral119-r5-t> [attempt]
#   *-v cells: OpenRouter arm + pi-contract-validator.js (tests build-list
#     item 3 live against the M5 contract failures).
#   *-t cell: Spark arm + pi-context-trimmer.js + validator (tests item 2
#     against the M1/M4 envelope deaths; gives Mistral-on-Spark its first
#     capability-graded rung-5 attempt).
# Detector always loaded; NO reviver (M2: illegal in Mistral role grammar).
set -u
CELL="$1"; ATTEMPT="${2:-1}"
case "$CELL" in
  *-r3-*) RUNG=3 ;;
  *-r5-*) RUNG=5 ;;
  *) echo "unknown cell: $CELL" >&2; exit 2 ;;
esac
SPIKE_DIR="$(cd "$(dirname "$0")" && pwd)"
BRIEF="${SPIKE_DIR}/../2026-07-16-difficulty-ladder/briefs/rung-${RUNG}.md"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-m2a-pi-${CELL}"
HARN="/home/administrator/.local/share/m2a-harnesses"
NODE22="${HARN}/node22/bin/node"
PI_CLI="${HARN}/pi/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"

SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-${CELL}-transcript${SUFFIX}.txt"

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

TAIL="End your final message with ONLY: Status (DONE | DONE_WITH_CONCERNS |
BLOCKED), a one-line test summary, concerns if any, and the report file path."

PROMPT="${JOB_HEAD}

${TAIL}

## Your brief (requirements of record):

$(cat "$BRIEF")"

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-${CELL} attempt-${ATTEMPT} dispatched" >> "${SDD}/timing.log"

case "$CELL" in
  mistralor-*)
    OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)" || exit 3
    export OPENROUTER_API_KEY
    cd "$WT" && timeout 1800 "$NODE22" "$PI_CLI" \
      -e "${SPIKE_DIR}/pi-toolsyntax-detector.js" \
      -e "${SPIKE_DIR}/pi-contract-validator.js" \
      --provider openrouter --model "mistralai/mistral-small-2603" \
      --thinking medium \
      -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
      "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
  mistral119-*)
    # Trimmer ONLY: on the Spark's strict mistral template, ANY injected
    # user message (validator/detector followUps, like the reviver) 400s
    # with "Unexpected role 'user' after role 'tool'" when it lands after
    # a tool message (measured 13:09Z false start). The trimmer injects
    # nothing — it only shrinks tool results — so it is the sole legal
    # extension on this backend. Contract data comes from the OR arm.
    cd "$WT" && SPARK_API_KEY=dummy timeout 1800 "$NODE22" "$PI_CLI" \
      -e "${SPIKE_DIR}/pi-spark-mistral.js" \
      -e "${SPIKE_DIR}/pi-context-trimmer.js" \
      --provider spark-mistral --model "mistral-small-4-119b" \
      -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
      "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT" ;;
esac
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-${CELL} attempt-${ATTEMPT} finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
