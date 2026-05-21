#!/bin/bash
# session-start-briefing.sh — SessionStart hook
#
# Emits a tuxlink session briefing into the model's context when a fresh
# Claude Code session starts. Includes branch state, working-tree status,
# the most recent handoff filename, and the 5 most recent commits.
#
# Input:  JSON on stdin (unused)
# Output: JSON with hookSpecificOutput.additionalContext for context injection.
# Exit:   0 always (failure to gather any field is non-fatal).

set -u

# Resolve repo root from this script's filesystem location (rev-parse-based
# per D1's hook-resolution discipline; supersedes the prior hardcoded path).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO" 2>/dev/null || { echo '{}'; exit 0; }

branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "(unknown)")
ahead_behind=$(git for-each-ref --format='%(upstream:track)' "refs/heads/$branch" 2>/dev/null)
status_count=$(git status --short 2>/dev/null | wc -l | tr -d ' ')

last_handoff_line=""
if [[ -d dev/handoffs ]]; then
    # Use find + sort to handle filenames safely (per codex 2026-05-17 D1
    # SC2012 finding: ls is fragile with non-alphanumeric names).
    # Pattern: handoff files are date-prefixed (YYYY-MM-DD-*.md) so lexical
    # sort = chronological sort. _template.md is excluded because it has no
    # date prefix (sorts last, which is wrong; explicit exclusion via glob).
    last_handoff_file=$(find dev/handoffs -maxdepth 1 -name '20*-*.md' -type f -print 2>/dev/null \
        | sort -r | head -1)
    if [[ -n "$last_handoff_file" ]]; then
        last_handoff_line="$(basename "$last_handoff_file")"
    fi
fi

recent_commits=$(git log --oneline -5 2>/dev/null)

# Branch-protection reminder when on integration branches.
branch_warning=""
case "$branch" in
    main)
        branch_warning=$'\n\n⚠️  You are on \x60main\x60. Direct commits are blocked by the commit-discipline hook. Branch off with \x60git checkout -b task-NN-<slug>\x60 before any work.'
        ;;
    feat/v*)
        branch_warning=$'\n\n⚠️  You are on the integration branch \x60'"$branch"$'\x60. Direct commits are blocked unless prefixed with \x60ALLOW_INTEGRATION_COMMIT=1\x60 (merge-commit carve-out per 2026-05-17 port catalog Decision 1; squash-merge is banned). Normal flow uses \x60gh pr merge --merge --delete-branch\x60. For ordinary task work, branch off with \x60git checkout -b task-NN-<slug>\x60.'
        ;;
esac

briefing=$(cat <<EOF
## Tuxlink session briefing

- **Branch:** \`${branch}\`${ahead_behind:+ ${ahead_behind}}
- **Working tree:** ${status_count} uncommitted file(s)
- **Most recent handoff:** ${last_handoff_line:-none}

### Recent commits
\`\`\`
${recent_commits}
\`\`\`${branch_warning}

### Reminders
- Pick a session moniker via \`python3 .claude/scripts/get_agent_moniker.py\` (3-word hyphenated form, auto-pre-flighted against git history). Legacy single-word monikers in older commits remain valid; the new format applies to forward commits.
- Per-task branches: \`task-NN-<slug>\` (or \`bd-<id>/<slug>\` once Beads is installed).
- Commit-discipline hooks will reject: missing \`Agent:\` trailer, unsubstituted \`<SESSION-MONIKER>\` placeholder, direct commits to integration branches.
EOF
)

jq -n --arg ctx "$briefing" '{
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": $ctx
    }
}'
