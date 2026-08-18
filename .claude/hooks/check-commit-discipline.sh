#!/bin/bash
# check-commit-discipline.sh — PreToolUse Bash hook
#
# Enforces commit discipline:
#   1. No <SESSION-MONIKER> placeholder leaks (catches unsubstituted plan templates).
#   2. Every commit has an Agent: <moniker> trailer (CLAUDE.md §Agent identity).
#   3. No direct commits to main (always blocked).
#   4. No direct commits to feat/v0.0.1 unless ALLOW_INTEGRATION_COMMIT=1 prefix is present
#      (carve-out for the legitimate merge-commit (no-ff) step in the per-task-branch model
#      per Decision 1 of the 2026-05-17 LFST→tuxlink port catalog: squash-merge is banned;
#      all PRs into integration branches merge as merge-commits with no fast-forward).
#
# Input:  JSON on stdin with .tool_input.command
# Output: JSON deny on stdout if a check fails; nothing if clean.
# Exit:   0 always.

set -u

# Resolve repo root from this script's filesystem location (rev-parse-based
# per D1's hook-resolution discipline; supersedes the prior hardcoded path).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')

# Only act on `git commit` invocations. `git commit --amend` is blocked by
# block-destructive-git.sh; we don't double-up here.
if ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+commit\b'; then
    exit 0
fi

deny() {
    local reason="$1"
    jq -n --arg reason "$reason" '{
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": $reason
        }
    }'
    exit 0
}

# Check 1: <SESSION-MONIKER> placeholder must be substituted before commit
if printf '%s' "$cmd" | grep -q '<SESSION-MONIKER>'; then
    deny "Commit message still contains the literal '<SESSION-MONIKER>' placeholder from the plan template. Substitute with your actual session moniker (e.g., 'Agent: alder') in the heredoc before committing."
fi

# Check 2: require Agent: <moniker> trailer
if ! printf '%s' "$cmd" | grep -qE 'Agent:[[:space:]]+[a-z0-9_-]+'; then
    deny "Commit message lacks the required 'Agent: <moniker>' trailer per CLAUDE.md. Add 'Agent: <your-session-moniker>' on its own line above 'Co-Authored-By:'."
fi

# Check 3+4: branch protection
# Resolve the branch from the PAYLOAD cwd — the checkout the commit actually
# runs in — constrained under $REPO. The script's own location is always the
# main checkout, which misjudged worktree commits once the main checkout
# parked on 'main' (tuxlink-kw1fd: every worktree commit was denied as a
# direct-to-main commit, blocking the exact flow ADR 0008 mandates). Same
# payload-cwd pattern as block-main-checkout-race.sh (codex D1 review).
# If the command redirects git elsewhere — the -C / --git-dir / --work-tree
# flag forms, or GIT_DIR= / GIT_WORK_TREE= environment assignments anywhere
# in the call (tuxlink-v7dju, Codex retro-review HIGH: the env forms redirect
# the real operation while the payload cwd still looks like a worktree) —
# fall back to $REPO, conservative: identical verdict to the pre-fix
# behavior. The env-token match is deliberately boundary-free (Codex round
# 2: quoted `env "..."` and append-assign spellings evade any boundary
# class; the bare token is the trigger). GIT_INDEX_FILE / GIT_COMMON_DIR /
# core.worktree were empirically shown NOT to relocate the commit's branch
# and are excluded. Prose mentioning these tokens in a commit message trips
# the fallback in the deny direction only; reword the message (same
# convention as block-destructive-git.sh's heredoc guidance).
payload_cwd=$(printf '%s' "$input" | jq -r '.cwd // ""')
cwd_for_git="$REPO"
if [[ -n "$payload_cwd" && -d "$payload_cwd" ]] \
   && ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+(-C[[:space:]]|--git-dir(=|[[:space:]])|--work-tree(=|[[:space:]]))' \
   && ! printf '%s' "$cmd" | grep -qE '(GIT_DIR|GIT_WORK_TREE)\+?='; then
    resolved_cwd=$(cd "$payload_cwd" 2>/dev/null && pwd)
    if [[ "$resolved_cwd" == "$REPO" || "$resolved_cwd" == "$REPO"/* ]]; then
        cwd_for_git="$resolved_cwd"
    fi
fi
branch=$(git -C "$cwd_for_git" rev-parse --abbrev-ref HEAD 2>/dev/null) || branch=""

case "$branch" in
    main)
        deny "Direct commits to 'main' are blocked per the per-task-branch model (docs/adr/0004-per-task-branch-model.md). Branch off with 'git checkout -b task-NN-<slug>' first; 'main' is updated only via ff-merge from the integration branch at release time."
        ;;
    feat/v0.0.1|feat/v*)
        # Allow only if command has ALLOW_INTEGRATION_COMMIT=1 set as an env-var prefix.
        # Match at command start OR after a shell separator (&&, ;, |).
        if ! printf '%s' "$cmd" | grep -qE '(^|[[:space:]&;|])[[:space:]]*ALLOW_INTEGRATION_COMMIT=1[[:space:]]+git'; then
            deny "Direct commits to '$branch' are blocked per the per-task-branch model (docs/adr/0004). For the legitimate merge-commit step (no-ff per Decision 1 of the 2026-05-17 port catalog), prefix the command with 'ALLOW_INTEGRATION_COMMIT=1 git commit ...'. Normal flow uses 'gh pr merge --merge --delete-branch' server-side, which bypasses this hook entirely. For ordinary task work, branch off with 'git checkout -b task-NN-<slug>' first."
        fi
        ;;
esac

exit 0
