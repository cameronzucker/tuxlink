#!/bin/bash
# block-destructive-git.sh — PreToolUse Bash hook
#
# Refuses Bash tool calls that contain banned destructive git operations
# per CLAUDE.md §"Git workflow — destructive commands are BANNED".
#
# Input:  JSON on stdin with .tool_input.command
# Output: JSON deny on stdout if a banned pattern is matched; nothing if clean.
# Exit:   0 always (decision is in the JSON output).

set -u

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')

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

# Pattern: git reset --hard
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+reset[[:space:]]+--hard\b'; then
    deny "git reset --hard is banned per CLAUDE.md. Use 'git revert <commit>' for an additive undo, or ask the user which specific file to restore."
fi

# Pattern: git push --force / -f / --force-with-lease
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+push\b.*(--force(-with-lease)?|[[:space:]]-f([[:space:]]|$))'; then
    deny "git push --force / -f / --force-with-lease is banned per CLAUDE.md. Force-push rewrites remote history; if you need to replace a pushed commit, open a new PR or ask the user."
fi

# Pattern: git checkout -- .  /  git restore .
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+(checkout[[:space:]]+--[[:space:]]+\.|restore[[:space:]]+\.)'; then
    deny "git checkout -- . / git restore . wipes the entire working tree per CLAUDE.md. Name specific files explicitly."
fi

# Pattern: git clean -f / -fd
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+clean\b.*-f'; then
    deny "git clean -f / -fd is banned per CLAUDE.md. Investigate untracked files manually before deleting."
fi

# Pattern: git branch -D / --delete --force
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch[[:space:]]+(-D|--delete[[:space:]]+--force)'; then
    deny "git branch -D / --delete --force is banned per CLAUDE.md. Use git branch -d (which refuses to delete unmerged branches)."
fi

# Pattern: git commit --amend
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+commit\b.*--amend\b'; then
    deny "git commit --amend is banned per CLAUDE.md. Always create a NEW commit to correct earlier work."
fi

# Pattern: git reflog expire ... --expire=now
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+reflog[[:space:]]+expire\b.*--expire=now'; then
    deny "git reflog expire --expire=now strips the reflog safety net per CLAUDE.md."
fi

# Pattern: git gc --prune=now
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+gc\b.*--prune=now'; then
    deny "git gc --prune=now strips the reflog safety net per CLAUDE.md."
fi

# Pattern: git filter-branch / git filter-repo
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+filter-(branch|repo)\b'; then
    deny "git filter-branch / filter-repo (mass history rewrite) is banned per CLAUDE.md."
fi

# Pattern: --no-verify on any git command
if printf '%s' "$cmd" | grep -qE '\bgit\b.*--no-verify\b'; then
    deny "git --no-verify (skips hooks) is banned per CLAUDE.md. The hooks exist for a reason; if a hook fails, fix the root cause."
fi

# Pattern: --no-gpg-sign / -c commit.gpgsign=false
if printf '%s' "$cmd" | grep -qE '\bgit\b.*(--no-gpg-sign|-c[[:space:]]+commit\.gpgsign=false)\b'; then
    deny "git --no-gpg-sign / -c commit.gpgsign=false is banned per CLAUDE.md."
fi

# All checks passed — allow by default (no output).
exit 0
