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

# Pattern: git worktree remove
# Disposal must use the 4-step ritual (inventory -> archive -> physical rm -rf -> git worktree prune).
# `git worktree remove` direct-unlinks the worktree, bypassing Recycle Bin / trash on platforms where that
# would otherwise be a safety net; the LFST musing-bhabha incident lost gitignored content this way.
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+worktree[[:space:]]+remove\b'; then
    deny "git worktree remove is banned. Use the disposal ritual: inventory ('git status --short' + 'git ls-files --others --exclude-standard' + 'git ls-files --others --ignored --exclude-standard') -> archive if needed -> rm -rf <worktree-path> -> git worktree prune. See docs/adr (worktree disposal ADR, pending D3) and standing-conventions-cross-project.md §4."
fi

# Pattern: git commit --amend
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+commit\b.*--amend\b'; then
    deny "git commit --amend is banned per CLAUDE.md. Always create a NEW commit to correct earlier work."
fi

# Pattern: git rebase -i / --interactive
# Interactive rebase exposes squash/fixup/drop on the editor screen and detecting the dangerous sub-ops
# in regex is non-trivial. Ban the interactive mode outright; use non-interactive rebases when needed.
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+rebase\b.*(-i\b|--interactive\b)'; then
    deny "git rebase -i / --interactive is banned. The editor screen exposes squash/fixup/drop which are destructive on shared commits, and detecting those sub-operations in this hook is impractical. Use non-interactive rebase ('git rebase <base>') for linear replays, or ask the user before attempting any history manipulation."
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

# --- Blast-radius patterns (tuxlink-18san, operator directive 2026-07-22) ---
# Five sessions hit the cwd-reset landmine where a chained
# "git add -A && git commit && git push" ran in the operator's main checkout
# and swept ~100 untracked WIP files into a mislabeled commit; the fifth
# occurrence PUSHED before the wrong-branch line could stop anything.
# "If it's banned in prose it should be banned with a hook."

# Heredoc-stripped view of the command: commit messages are heredocs
# (<<EOF ... EOF, the Agent-trailer convention) whose PROSE legitimately
# mentions git commands ("run git push at session end"). Counting those
# as invocations would false-deny every well-formed commit, so the chain
# checks below run on a copy with heredoc bodies removed. Conservative:
# only the conventional EOF tag is stripped; other tags stay visible and
# at worst cause a spurious deny (reword the message), never a miss.
cmd_stripped=$(printf '%s' "$cmd" | awk '
    hd { if ($0 == "EOF") hd = 0; next }
    /<<-?['\''"]?EOF['\''"]?/ { hd = 1; print; next }
    { print }
')

# Pattern: git add -A / --all / bare-dot pathspec
# The main checkout perpetually holds ~90 untracked operator files; a
# sweep-stage from a reset cwd is how every occurrence started. Stage
# explicit paths or a scoped subtree and verify with
# 'git diff --cached --name-only'.
if printf '%s' "$cmd_stripped" | grep -qE '\bgit([[:space:]]+-[Cc][[:space:]]+[^[:space:]]+)?[[:space:]]+add[[:space:]]+(-[a-zA-Z]*A|--all\b|\.([[:space:]]|$|["'\''"]))'; then
    deny "git add -A / --all / 'git add .' is banned per CLAUDE.md. The main checkout is full of untracked operator WIP; sweep-staging from a reset cwd has committed ~100 operator files onto the wrong branch five times. Stage explicit paths (e.g. 'git add src-tauri/ docs/x.md') and verify with 'git diff --cached --name-only'."
fi

# Pattern: two or more MUTATING git invocations chained in one call
# One git write per Bash call: the wrong-branch line printed by commit is
# the last tripwire before push, and it only works if push is a SEPARATE
# call. Read-only chains (git log && git status) remain free.
mutating='(add|commit|push|merge|rebase|pull|cherry-pick|revert|reset|stash|switch|checkout|restore|rm|mv|am|apply)'
mut_count=$(printf '%s' "$cmd_stripped" | grep -oE "\bgit([[:space:]]+-[Cc][[:space:]]+[^[:space:]]+)?[[:space:]]+$mutating\b" | wc -l)
if [ "$mut_count" -ge 2 ]; then
    deny "Chaining multiple mutating git operations in one call is banned per CLAUDE.md (found $mut_count). One git write per Bash call: commit in one call, READ the printed branch line, push in the next. This is the deterministic form of the rule that would have stopped all five 'git add -A && git commit && git push' wrong-branch incidents."
fi

# Pattern: cd chained before a mutating git op in the same call
# The main-checkout-race hook judges the PAYLOAD cwd, which an inline cd
# does not update - 'cd <worktree> && git commit' is classified from
# wherever the shell WAS. Standalone cd call first, then the git op bare.
if printf '%s' "$cmd_stripped" | grep -qE '(^|&&|;|\|\|)[[:space:]]*cd[[:space:]][^|&;]*(&&|;)[[:space:]]*git([[:space:]]+-[Cc][[:space:]]+[^[:space:]]+)?[[:space:]]+'"$mutating"'\b'; then
    deny "'cd <dir> && git <write-op>' in one call is banned per CLAUDE.md. The race hook classifies the call by the payload cwd, not the inline cd, so this form runs (or is judged) against the WRONG tree. Run a standalone 'cd <dir>' call first, verify with pwd or 'git branch --show-current', then run the git op bare in the next call."
fi

# All checks passed — allow by default (no output).
exit 0
