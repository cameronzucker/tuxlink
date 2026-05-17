#!/bin/bash
# block-main-checkout-race.sh — PreToolUse Bash hook (ported from
# support-tools/.claude/hooks/block-main-checkout-race.ps1)
#
# When more than one live tuxlink Claude Code session is active, branch / HEAD /
# history mutations in the main checkout are denied unless the current session
# owns the main-checkout lease. Worktrees remain the normal path for concurrent
# feature work (per D2's worktree-issue-ownership rule, once it lands).
#
# Side effects:
#   - Writes/refreshes this session's lease at .claude/session-leases/<session-id>.json
#     on every invocation (heartbeat). TTL is 30 minutes of wall clock; stale
#     leases are ignored.
#   - On deny: appends a JSONL record to .claude/session-leases/denied-attempts.jsonl
#
# Input:  Claude Code hook payload JSON on stdin
# Output: JSON deny on stdout if a banned pattern matches; nothing if clean.
# Exit:   0 always (decision is in the JSON output).
#
# Rev-parse-based path resolution: REPO is derived from this script's location
# (.claude/hooks/<this>.sh -> ../../) per ADR D1 hook-resolution discipline.

set -u

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
[[ -z "$cmd" ]] && exit 0

# --- Resolve repo root from this script's filesystem location ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"
[[ -d "$REPO/.git" || -f "$REPO/.git" ]] || exit 0  # not a git repo; bail to allow

LEASE_DIR="$REPO/.claude/session-leases"
mkdir -p "$LEASE_DIR" 2>/dev/null

# --- Derive session id from hook payload ---
session_id=$(printf '%s' "$input" | jq -r '.session_id // ""')
if [[ -z "$session_id" ]]; then
    transcript_path=$(printf '%s' "$input" | jq -r '.transcript_path // ""')
    if [[ -n "$transcript_path" ]]; then
        session_id=$(printf '%s' "$transcript_path" | md5sum | awk '{print $1}')
    else
        # No reliable id; allow rather than misclassify.
        exit 0
    fi
fi

# --- Resolve cwd for git queries (constrained under $REPO) ---
payload_cwd=$(printf '%s' "$input" | jq -r '.cwd // ""')
cwd_for_git="$REPO"
if [[ -n "$payload_cwd" && -d "$payload_cwd" ]]; then
    resolved_cwd=$(cd "$payload_cwd" 2>/dev/null && pwd)
    # Constrain: only honor if it's under $REPO (prevents a stray cd outside the
    # repo from masking what's really a main-checkout operation).
    if [[ "$resolved_cwd" == "$REPO" || "$resolved_cwd" == "$REPO"/* ]]; then
        cwd_for_git="$resolved_cwd"
    fi
fi

# --- Determine main-checkout vs worktree (conservative: assume main unless proven otherwise) ---
is_main_checkout=true
git_dir=$(git -C "$cwd_for_git" rev-parse --absolute-git-dir 2>/dev/null || echo "")
if [[ -n "$git_dir" ]]; then
    # Worktrees have a gitdir of the form `.../.git/worktrees/<name>`.
    if [[ "$git_dir" == *"/.git/worktrees/"* ]]; then
        is_main_checkout=false
    fi
fi

# --- Determine current branch (normalize empty + literal `HEAD` to `(detached)`) ---
current_branch=$(git -C "$cwd_for_git" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
if [[ -z "$current_branch" || "$current_branch" == "HEAD" ]]; then
    current_branch="(detached)"
fi

# --- Optional moniker from env (not always populated) ---
moniker="${TUXLINK_AGENT_MONIKER:-(unknown)}"

# --- Refresh THIS session's lease (heartbeat) ---
now_utc=$(date -u +%Y-%m-%dT%H:%M:%S.%6NZ)
lease_path="$LEASE_DIR/$session_id.json"
# Best-effort lease write; never fail the hook on infrastructure write failure.
jq -n \
    --arg sid "$session_id" \
    --arg mon "$moniker" \
    --arg repo "$REPO" \
    --arg cwd "$payload_cwd" \
    --arg br "$current_branch" \
    --argjson main "$is_main_checkout" \
    --arg ts "$now_utc" \
    '{sessionId:$sid, moniker:$mon, repo:$repo, cwd:$cwd, branch:$br, isMainCheckout:$main, lastSeenUtc:$ts}' \
    > "$lease_path" 2>/dev/null || true

# --- Find OTHER live sessions in this repo (TTL = 30 min) ---
# Use date math via epoch seconds.
cutoff_epoch=$(date -u -d '30 minutes ago' +%s 2>/dev/null || echo "0")
other_live_count=0
other_summary=""
if [[ -d "$LEASE_DIR" ]]; then
    for lease_file in "$LEASE_DIR"/*.json; do
        [[ -e "$lease_file" ]] || continue
        bn=$(basename "$lease_file")
        # Skip own lease + the main-checkout lease + the denied-attempts.jsonl
        [[ "$bn" == "$session_id.json" || "$bn" == "main-checkout.json" ]] && continue
        other_sid=$(jq -r '.sessionId // ""' "$lease_file" 2>/dev/null)
        other_repo=$(jq -r '.repo // ""' "$lease_file" 2>/dev/null)
        other_ts=$(jq -r '.lastSeenUtc // ""' "$lease_file" 2>/dev/null)
        other_branch=$(jq -r '.branch // ""' "$lease_file" 2>/dev/null)
        other_main=$(jq -r '.isMainCheckout // false' "$lease_file" 2>/dev/null)
        other_moniker=$(jq -r '.moniker // "(unknown)"' "$lease_file" 2>/dev/null)
        [[ -z "$other_ts" || "$other_repo" != "$REPO" ]] && continue
        other_epoch=$(date -u -d "$other_ts" +%s 2>/dev/null || echo "0")
        if (( other_epoch > cutoff_epoch )); then
            age_min=$(( ($(date -u +%s) - other_epoch) / 60 ))
            loc=$([[ "$other_main" == "true" ]] && echo "main checkout" || echo "worktree")
            other_summary="${other_summary:+$other_summary; }${other_moniker} on '${other_branch}' [${loc}, last seen ${age_min}m ago]"
            other_live_count=$(( other_live_count + 1 ))
        fi
    done
fi

# --- Fast paths: no concurrent session, OR I'm in a worktree ---
if (( other_live_count == 0 )) || [[ "$is_main_checkout" != "true" ]]; then
    exit 0
fi

# --- Risky-op patterns (per LFST source; narrow, NOT a full git ban) ---
# Read-only / non-mutating ops (status, diff, log, fetch, show, push) are NOT
# governed by this hook.
risky_re='(^|[^a-zA-Z0-9_-])(git[[:space:]]+(checkout([[:space:]]+(--show-current|-l|--detach)|[[:space:]]+[^-])|switch|commit|merge|rebase|pull|reset|branch([[:space:]]+--show-current|[[:space:]]+-a|[[:space:]]+--all|[[:space:]]+-r|[[:space:]]+--remote|[[:space:]]+--list|[[:space:]]+-l)?|add|cherry-pick|revert|stash[[:space:]]+(push|pop|apply|drop|clear)))'
# Simpler approach: a small set of separate checks is more readable than one
# mega-regex. Reset to per-pattern checks:
matched=false
for pattern in \
    '\bgit[[:space:]]+switch\b' \
    '\bgit[[:space:]]+commit\b' \
    '\bgit[[:space:]]+merge\b' \
    '\bgit[[:space:]]+rebase\b' \
    '\bgit[[:space:]]+pull\b' \
    '\bgit[[:space:]]+reset\b' \
    '\bgit[[:space:]]+add\b' \
    '\bgit[[:space:]]+cherry-pick\b' \
    '\bgit[[:space:]]+revert\b'; do
    if printf '%s' "$cmd" | grep -qE "$pattern"; then
        matched=true
        break
    fi
done

# `git checkout` is risky EXCEPT for the read-only forms (--show-current, -l, --detach).
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+checkout\b'; then
    if ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+checkout[[:space:]]+(--show-current|-l|--detach)\b'; then
        matched=true
    fi
fi

# `git branch` is risky EXCEPT for the listing forms.
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch\b'; then
    if ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch[[:space:]]+(--show-current|-a|--all|-r|--remote|--list|-l)\b'; then
        matched=true
    fi
fi

# `git stash` is risky for push/pop/apply/drop/clear.
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+stash[[:space:]]+(push|pop|apply|drop|clear)\b'; then
    matched=true
fi

if ! $matched; then
    exit 0
fi

# --- Check if current session owns the main-checkout lease ---
main_lease="$LEASE_DIR/main-checkout.json"
if [[ -f "$main_lease" ]]; then
    main_sid=$(jq -r '.sessionId // ""' "$main_lease" 2>/dev/null)
    main_ts=$(jq -r '.lastSeenUtc // ""' "$main_lease" 2>/dev/null)
    if [[ -n "$main_ts" && "$main_sid" == "$session_id" ]]; then
        main_epoch=$(date -u -d "$main_ts" +%s 2>/dev/null || echo "0")
        if (( main_epoch > cutoff_epoch )); then
            exit 0  # I hold the lease.
        fi
    fi
fi

# --- Build deny reason ---
deny_reason="Main-checkout HEAD/branch/history operation BLOCKED.

Another live tuxlink session is active: $other_summary.
This session does NOT own the main-checkout lease.

Per the worktree-mandatory rule (D2 ADR, once it lands): worktrees are MANDATORY
for write work when another live Claude Code session is active. The main
checkout is a shared coordination surface, not a concurrent task workspace.

QUICK FIX (once D4's worktree-creator script lands):
  .claude/scripts/new_tuxlink_worktree.sh <slug> [<bd-issue-id>] [<moniker>]
This creates worktrees/<bd-id-or-slug>/, claims the bd issue if provided, and
prints the cd path. Switch your work there, then re-run your git op.

To see active sessions (once D4's script lands):
  .claude/scripts/get_tuxlink_sessions.sh

To take the main-checkout lease (e.g., for integration coordination work that
genuinely belongs in main): coordinate with other active sessions first, then
write \$LEASE_DIR/main-checkout.json with your sessionId. Hook re-checks lease
ownership on every invocation.

This denial is recorded at .claude/session-leases/denied-attempts.jsonl for
forensics."

# --- Log denied attempt (best-effort; never fail the deny on log failure) ---
denied_log="$LEASE_DIR/denied-attempts.jsonl"
denied_entry=$(jq -n \
    --arg ts "$now_utc" \
    --arg sid "$session_id" \
    --arg mon "$moniker" \
    --arg br "$current_branch" \
    --arg cmdstr "$cmd" \
    --arg other "$other_summary" \
    '{timestamp:$ts, sessionId:$sid, moniker:$mon, branch:$br, command:$cmdstr, otherLiveSessions:$other}' 2>/dev/null)
[[ -n "$denied_entry" ]] && printf '%s\n' "$denied_entry" >> "$denied_log" 2>/dev/null || true

# --- Emit deny per Claude Code hook protocol ---
jq -n --arg reason "$deny_reason" '{
    "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": $reason
    }
}'

exit 0
