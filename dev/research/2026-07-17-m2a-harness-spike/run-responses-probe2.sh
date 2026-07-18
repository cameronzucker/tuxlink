#!/usr/bin/env bash
# M2a post-hoc probe #3 — pi-e122-r5 over the Responses route WITH the
# harness fixes the verdict's work items mandated:
#   -e pi-openrouter-responses.js   (Responses route, probe #2 machinery)
#   -e pi-think-reviver.js          (restores per-turn thinking; see file header)
#   -e pi-toolsyntax-detector.js    (mandatory work item 2: pseudo-tool-call retry)
# Everything else held constant with probe #2 / the registered pi-e122-r5
# cell: frozen rung-5 brief text, contract, --thinking medium, 30-min cap,
# -ne -ns --offline, worker base b82b404d.
set -u
ATTEMPT="${1:-1}"
SPIKE_DIR="$(cd "$(dirname "$0")" && pwd)"
BRIEF="${SPIKE_DIR}/../2026-07-16-difficulty-ladder/briefs/rung-5.md"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-m2a-pi-e122-r5-responses2"
HARN="/home/administrator/.local/share/m2a-harnesses"
NODE22="${HARN}/node22/bin/node"
PI_CLI="${HARN}/pi/node_modules/@earendil-works/pi-coding-agent/dist/cli.js"

RUNG=5
SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-pi-responses2-transcript${SUFFIX}.txt"

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

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-e122-r5-responses2 attempt-${ATTEMPT} dispatched" >> "${SDD}/timing.log"

OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)" || exit 3
export OPENROUTER_API_KEY
cd "$WT" && timeout 1800 "$NODE22" "$PI_CLI" \
  -e "${SPIKE_DIR}/pi-openrouter-responses.js" \
  -e "${SPIKE_DIR}/pi-think-reviver.js" \
  -e "${SPIKE_DIR}/pi-toolsyntax-detector.js" \
  --provider openrouter-responses --model "qwen/qwen3.5-122b-a10b" \
  --thinking medium \
  -p --mode text --session-dir "$SDD/pi-sessions" -ne -ns --offline \
  "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT"
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) cell-pi-e122-r5-responses2 attempt-${ATTEMPT} finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
