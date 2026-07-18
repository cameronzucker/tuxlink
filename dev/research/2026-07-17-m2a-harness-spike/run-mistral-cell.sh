#!/usr/bin/env bash
# Mistral round dispatcher — one worker attempt for one cell.
# Usage: run-mistral-cell.sh <r3|r5> [attempt]
# M2 harness minus the reviver: Mistral's chat template REJECTS a user
# message directly after a tool message (400 "Unexpected role 'user' after
# role 'tool'", measured 2026-07-18 a1 false start) — the F6 nudge that
# Qwen requires is illegal in Mistral's role grammar. Reviver must become
# model-conditional in M2; detector stays loaded.
set -u
RUNGARG="$1"; ATTEMPT="${2:-1}"
case "$RUNGARG" in
  r3) RUNG=3 ;;
  r5) RUNG=5 ;;
  *) echo "usage: run-mistral-cell.sh <r3|r5> [attempt]" >&2; exit 2 ;;
esac
SPIKE_DIR="$(cd "$(dirname "$0")" && pwd)"
BRIEF="${SPIKE_DIR}/../2026-07-16-difficulty-ladder/briefs/rung-${RUNG}.md"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-m2a-pi-mistral119-${RUNGARG}"
HARN="/home/administrator/.local/share/m2a-harnesses"
NODE22="${HARN}/node22/bin/node"
PI_CLI="${HARN}/pi/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"

SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-pi-mistral-transcript${SUFFIX}.txt"

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

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-mistral119-${RUNGARG} attempt-${ATTEMPT} dispatched" >> "${SDD}/timing.log"

cd "$WT" && SPARK_API_KEY=dummy timeout 1800 "$NODE22" "$PI_CLI" \
  -e "${SPIKE_DIR}/pi-spark-mistral.js" \
  -e "${SPIKE_DIR}/pi-toolsyntax-detector.js" \
  --provider spark-mistral --model "mistral-small-4-119b" \
  -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
  "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT"
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-mistral119-${RUNGARG} attempt-${ATTEMPT} finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
