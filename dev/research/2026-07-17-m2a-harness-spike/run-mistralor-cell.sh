#!/usr/bin/env bash
# Mistral-over-OpenRouter comparison arm (operator-directed): the SAME model
# vintage as the Spark round (mistral-small-2603 = Mistral Small 4 119B) at
# full precision and 262k context — decouples the model from the Spark's
# 32k MLA-disable envelope (findings M1/M4). Pi builtin openrouter catalog
# entry (reasoning:true, chat-completions); --thinking medium matches the
# pi-e122-r5 OpenRouter treatment. Detector loaded; NO reviver (M2: illegal
# in Mistral's role grammar).
# Usage: run-mistralor-cell.sh <r3|r5> [attempt]
set -u
RUNGARG="$1"; ATTEMPT="${2:-1}"
case "$RUNGARG" in
  r3) RUNG=3 ;;
  r5) RUNG=5 ;;
  *) echo "usage: run-mistralor-cell.sh <r3|r5> [attempt]" >&2; exit 2 ;;
esac
SPIKE_DIR="$(cd "$(dirname "$0")" && pwd)"
BRIEF="${SPIKE_DIR}/../2026-07-16-difficulty-ladder/briefs/rung-${RUNG}.md"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-m2a-pi-mistralor-${RUNGARG}"
HARN="/home/administrator/.local/share/m2a-harnesses"
NODE22="${HARN}/node22/bin/node"
PI_CLI="${HARN}/pi/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"

SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-pi-mistralor-transcript${SUFFIX}.txt"

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

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-mistralor-${RUNGARG} attempt-${ATTEMPT} dispatched" >> "${SDD}/timing.log"

OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)" || exit 3
export OPENROUTER_API_KEY
cd "$WT" && timeout 1800 "$NODE22" "$PI_CLI" \
  -e "${SPIKE_DIR}/pi-toolsyntax-detector.js" \
  --provider openrouter --model "mistralai/mistral-small-2603" \
  --thinking medium \
  -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
  "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT"
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-mistralor-${RUNGARG} attempt-${ATTEMPT} finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
