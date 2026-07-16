#!/usr/bin/env bash
# Ladder worker runner — one codex worker attempt for one rung of one arm.
# Usage: run-rung.sh <arm: cn|o397|q122|e122> <rung: 1..6> [attempt: 1|2]
# Reads the frozen brief, wraps it in the R2 guidance frame, dispatches codex
# with per-invocation provider overrides (never touches ~/.codex/config.toml),
# enforces the 30-min cap, tees the transcript into the arm's .superpowers/sdd.
set -u
ARM="$1"; RUNG="$2"; ATTEMPT="${3:-1}"
LADDER_DIR="$(cd "$(dirname "$0")" && pwd)"
WT="/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-7raoe-ladder-arm-${ARM}"
BRIEF="${LADDER_DIR}/briefs/rung-${RUNG}.md"
SDD="${WT}/.superpowers/sdd"
mkdir -p "$SDD"
SUFFIX=""; [ "$ATTEMPT" != "1" ] && SUFFIX=".attempt${ATTEMPT}"
TRANSCRIPT="${SDD}/rung-${RUNG}-codex-transcript${SUFFIX}.txt"

case "$ARM" in
  cn)
    PROVIDER=spark; MODEL=qwen3-coder-next
    BASE_URL=https://inference.twin-bramble.ts.net/v1
    KEY_ENV=SPARK_API_KEY; export SPARK_API_KEY=dummy ;;
  q122)
    PROVIDER=spark; MODEL=qwen35-122b-nvfp4
    BASE_URL=https://inference.twin-bramble.ts.net/v1
    KEY_ENV=SPARK_API_KEY; export SPARK_API_KEY=dummy ;;
  o397)
    PROVIDER=openrouter; MODEL=qwen/qwen3.5-397b-a17b
    BASE_URL=https://openrouter.ai/api/v1
    KEY_ENV=OPENROUTER_API_KEY
    OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)"; export OPENROUTER_API_KEY ;;
  e122)
    PROVIDER=openrouter; MODEL=qwen/qwen3.5-122b-a10b
    BASE_URL=https://openrouter.ai/api/v1
    KEY_ENV=OPENROUTER_API_KEY
    OPENROUTER_API_KEY="$(secret-tool lookup service elmer-openrouter account teacher)"; export OPENROUTER_API_KEY ;;
  *) echo "unknown arm: $ARM" >&2; exit 2 ;;
esac

PROMPT="$(cat <<WRAP
You are implementing a task in the tuxlink repository.

## Harness usage (important)

Read files with shell commands (cat, sed -n, rg, grep). Make EDITS by running
python3 heredoc scripts that read the file, perform exact string replacement,
and write it back — then VERIFY each edit landed with grep before moving on.
Example:

    python3 - <<'PYEOF'
    p = 'path/to/file.ts'
    s = open(p).read()
    old = "exact existing text"
    new = "replacement text"
    assert old in s
    open(p, 'w').write(s.replace(old, new, 1))
    PYEOF

Do NOT use apply_patch (not available), interactive editors (ed/vi), or MCP
resource reads (no servers exist). If your edits repeatedly fail to land,
STOP and report status BLOCKED with what you tried — NEVER report work as
implemented or tests as passing unless you ran the command and saw it.

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
commits.

End your final message with ONLY: Status (DONE | DONE_WITH_CONCERNS |
BLOCKED), a one-line test summary, concerns if any, and the report file path.

## Your brief (requirements of record):

$(cat "$BRIEF")
WRAP
)"

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) arm-${ARM} rung-${RUNG} attempt-${ATTEMPT} codex worker dispatched (model ${MODEL})" >> "${SDD}/timing.log"
timeout 1800 codex exec --skip-git-repo-check \
  --cd "$WT" \
  -c model_provider="$PROVIDER" \
  -c "model_providers.${PROVIDER}.name=${PROVIDER}" \
  -c "model_providers.${PROVIDER}.base_url=${BASE_URL}" \
  -c "model_providers.${PROVIDER}.wire_api=responses" \
  -c "model_providers.${PROVIDER}.env_key=${KEY_ENV}" \
  -m "$MODEL" "$PROMPT" </dev/null 2>&1 | tee "$TRANSCRIPT"
RC=${PIPESTATUS[0]}
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) arm-${ARM} rung-${RUNG} attempt-${ATTEMPT} codex worker finished (exit ${RC})" >> "${SDD}/timing.log"
exit "$RC"
