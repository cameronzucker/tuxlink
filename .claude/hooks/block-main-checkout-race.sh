#!/bin/bash
# block-main-checkout-race.sh — PreToolUse Bash hook (ported from
# support-tools/.claude/hooks/block-main-checkout-race.ps1; refined per
# codex 2026-05-17 D1 review findings)
#
# When more than one live tuxlink Claude Code session is active, branch / HEAD /
# history mutations in the main checkout are denied unless the current session
# owns the main-checkout lease. Worktrees remain the normal path for concurrent
# feature work per ADR 0008.
#
# Leases live at <git-common-dir>/session-leases/ — i.e., shared by the main
# checkout AND every worktree of this repo. (Per codex D1-P1: a worktree's
# .claude/session-leases/ would be a different filesystem location from main's,
# so leases written from worktree never propagate to main; storing them under
# the shared git common-dir fixes the cross-checkout visibility gap.)
#
# Side effects:
#   - Writes/refreshes this session's lease at <git-common-dir>/session-leases/<session-id>.json
#     on every invocation (heartbeat). TTL is 30 minutes of wall clock.
#   - On deny: appends a JSONL record to <git-common-dir>/session-leases/denied-attempts.jsonl
#
# Input:  Claude Code hook payload JSON on stdin
# Output: JSON deny on stdout if a banned pattern matches; nothing if clean.
# Exit:   0 always (decision is in the JSON output).

set -u

input=$(cat)
cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
[[ -z "$cmd" ]] && exit 0

# --- Resolve repo root from this script's filesystem location ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"
[[ -d "$REPO/.git" || -f "$REPO/.git" ]] || exit 0  # not a git repo; bail to allow

# --- Resolve LEASE_DIR to the shared git common-dir ---
# Per codex D1-P1: leases MUST live in a location shared by main + all worktrees,
# not per-checkout under .claude/. `git rev-parse --git-common-dir` returns the
# shared .git/ regardless of which checkout we're invoked from.
git_common_dir=$(cd "$REPO" 2>/dev/null && git rev-parse --git-common-dir 2>/dev/null)
[[ -z "$git_common_dir" ]] && exit 0  # not a git repo per git itself; bail
if [[ "$git_common_dir" != /* ]]; then
    git_common_dir=$(cd "$REPO" && cd "$git_common_dir" 2>/dev/null && pwd)
    [[ -z "$git_common_dir" ]] && exit 0
fi
LEASE_DIR="$git_common_dir/session-leases"
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
    if [[ "$resolved_cwd" == "$REPO" || "$resolved_cwd" == "$REPO"/* ]]; then
        cwd_for_git="$resolved_cwd"
    fi
fi

# --- Determine main-checkout vs worktree (conservative: assume main unless proven otherwise) ---
is_main_checkout=true
git_dir=$(git -C "$cwd_for_git" rev-parse --absolute-git-dir 2>/dev/null || echo "")
if [[ -n "$git_dir" ]]; then
    if [[ "$git_dir" == *"/.git/worktrees/"* ]]; then
        is_main_checkout=false
    fi
fi

# --- Detect git-target-override forms (per codex D1-P2) ---
# `git -C <path>` and `git --git-dir=<path>` redirect git to operate on a
# different repo/checkout than the agent's cwd. If present, do not fast-path
# on cwd-is-worktree, because the command may target the main checkout from
# within a worktree.
has_git_target_override=false
if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+(-C[[:space:]]|--git-dir(=|[[:space:]]))'; then
    has_git_target_override=true
fi

# --- Determine current branch ---
current_branch=$(git -C "$cwd_for_git" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
if [[ -z "$current_branch" || "$current_branch" == "HEAD" ]]; then
    current_branch="(detached)"
fi

moniker="${TUXLINK_AGENT_MONIKER:-(unknown)}"

# --- Refresh THIS session's lease (heartbeat) ---
now_utc=$(date -u +%Y-%m-%dT%H:%M:%S.%6NZ)
lease_path="$LEASE_DIR/$session_id.json"
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
cutoff_epoch=$(date -u -d '30 minutes ago' +%s 2>/dev/null || echo "0")
other_live_count=0
other_summary=""
if [[ -d "$LEASE_DIR" ]]; then
    for lease_file in "$LEASE_DIR"/*.json; do
        [[ -e "$lease_file" ]] || continue
        bn=$(basename "$lease_file")
        [[ "$bn" == "$session_id.json" || "$bn" == "main-checkout.json" ]] && continue
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

# --- Fast paths ---
# 1) No other live session: allow.
if (( other_live_count == 0 )); then
    exit 0
fi
# 2) I'm in a worktree AND the command does NOT have a git-target override:
#    the command targets only my worktree; let it through. Per codex D1-P2, this
#    fast-path is unsafe when `git -C` / `--git-dir` redirects target — fall
#    through to the lease check in that case.
if [[ "$is_main_checkout" != "true" ]] && [[ "$has_git_target_override" != "true" ]]; then
    exit 0
fi

# --- Risky-op detection ---
# Narrow set per LFST source. Read-only ops (status, diff, log, fetch, show,
# push) are NOT governed by this hook.
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

# `git checkout` is risky EXCEPT for the read-only forms.
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+checkout\b'; then
    if ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+checkout[[:space:]]+(--show-current|-l|--detach)\b'; then
        matched=true
    fi
fi

# `git branch` is risky for mutating subcommands; allow bare `git branch` (lists
# all local branches) and the explicit listing forms (per codex D1-P3).
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch\b'; then
    # Mutating cases: -d / -D / --delete / -m / -M / --move / -c / -C / --copy /
    # --set-upstream-to / --unset-upstream, OR a branch-name argument (any word
    # not starting with `-`).
    if printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch[[:space:]]+(-d|-D|--delete|-m|-M|--move|-c|-C|--copy|--set-upstream-to|--unset-upstream)\b'; then
        matched=true
    elif printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+branch[[:space:]]+[^-[:space:]]'; then
        # `git branch <name>` — creates a branch; mutating
        matched=true
    fi
    # bare `git branch` (no args) → listing, allow
    # `git branch --show-current` / `-a` / `--all` / `-r` / `--remote` / `--list` / `-l` → listing, allow
fi

# `git stash` is mutating unless followed by a read-only subcommand
# (per codex D1-P2: bare `git stash` defaults to `push` and was being missed).
if ! $matched && printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+stash(\b|$)'; then
    if ! printf '%s' "$cmd" | grep -qE '\bgit[[:space:]]+stash[[:space:]]+(list|show)\b'; then
        matched=true
    fi
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

Per ADR 0008 (worktrees mandatory under bd-issue ownership): worktrees are
MANDATORY for write work when another live Claude Code session is active.
The main checkout is a shared coordination surface, not a concurrent task
workspace.

QUICK FIX:
  python3 .claude/scripts/new_tuxlink_worktree.py --slug <slug> --issue <bd-id>
This creates worktrees/<name>/, claims the bd issue, records the path, and
prints the cd target. Switch your work there, then re-run your git op.

To see active sessions:
  python3 .claude/scripts/get_tuxlink_sessions.py

To take the main-checkout lease (e.g., for integration coordination work that
genuinely belongs in main): coordinate with other active sessions first, then
write \$LEASE_DIR/main-checkout.json with your sessionId. Hook re-checks lease
ownership on every invocation.

This denial is recorded at <git-common-dir>/session-leases/denied-attempts.jsonl
for forensics."

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
if [[ -n "$denied_entry" ]]; then
    printf '%s\n' "$denied_entry" >> "$denied_log" 2>/dev/null || true
fi

# --- Emit deny per Claude Code hook protocol ---
jq -n --arg reason "$deny_reason" '{
    "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": $reason
    }
}'

exit 0
