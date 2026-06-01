# branch-state.sh — shared classifier for tuxlink's branch lifecycle hooks.
#
# Implements the state machine from docs/adr/0017-branch-state-machine.md.
# Sourced by .githooks/pre-commit and .githooks/pre-push.
#
# Exports one function:
#
#   classify_branch_state <branch-name>
#     Echoes one of:
#       protected   — main / master / release / any ref the hooks should skip
#       bot         — release-please--*, dependabot/*, github-actions/*
#       merged-dead — branch has a merged PR; commit/push REFUSE
#       closed-dead — branch has a closed-without-merge PR; commit/push REFUSE
#       pr-open     — branch has an open PR; allow
#       active      — local branch, no PR yet; allow
#       unknown     — gh unavailable or auth failed; warn + allow (CI catches)
#
# Exit codes from this function are ALWAYS 0; the classification is the
# echoed value. Callers compare strings.

# Branch names matching these patterns are treated as bot-owned and always
# bypass the state-machine refuse rules.
_branch_state_is_bot() {
  case "$1" in
    release-please--*) return 0 ;;
    dependabot/*)      return 0 ;;
    github-actions/*)  return 0 ;;
    renovate/*)        return 0 ;;
    *) return 1 ;;
  esac
}

# Branch names matching these patterns are treated as protected (managed by
# GitHub branch protection + .claude/hooks/block-destructive-git.sh, not by
# the lifecycle hooks).
_branch_state_is_protected() {
  case "$1" in
    main|master|release|production) return 0 ;;
    release/*) return 0 ;;
    *) return 1 ;;
  esac
}

classify_branch_state() {
  local branch="$1"

  if [ -z "${branch}" ] || [ "${branch}" = "HEAD" ]; then
    echo "unknown"
    return 0
  fi

  if _branch_state_is_protected "${branch}"; then
    echo "protected"
    return 0
  fi

  if _branch_state_is_bot "${branch}"; then
    echo "bot"
    return 0
  fi

  if ! command -v gh >/dev/null 2>&1; then
    echo "unknown"
    return 0
  fi

  # 5-second timeout per gh call so a slow network does not stall every
  # `git commit` for minutes.
  local merged_check closed_check open_check

  merged_check="$(timeout 5 gh pr list --head "${branch}" --state merged --limit 1 --json number,mergedAt 2>/dev/null || true)"
  if printf '%s' "${merged_check}" | grep -q '"mergedAt"'; then
    echo "merged-dead"
    return 0
  fi

  closed_check="$(timeout 5 gh pr list --head "${branch}" --state closed --limit 1 --json number,state 2>/dev/null || true)"
  if printf '%s' "${closed_check}" | grep -q '"state":"CLOSED"'; then
    # state=CLOSED with no mergedAt means closed-without-merge.
    if ! printf '%s' "${closed_check}" | grep -q '"mergedAt"'; then
      echo "closed-dead"
      return 0
    fi
  fi

  open_check="$(timeout 5 gh pr list --head "${branch}" --state open --limit 1 --json number 2>/dev/null || true)"
  if printf '%s' "${open_check}" | grep -q '"number"'; then
    echo "pr-open"
    return 0
  fi

  echo "active"
  return 0
}

# Helper: print a multi-line refuse message for the operator. Args:
#   $1 = state (merged-dead / closed-dead)
#   $2 = branch name
#   $3 = operation (commit / push)
#   $4 = override env var name (e.g. TUXLINK_BRANCH_LIFECYCLE_OVERRIDE)
branch_state_refuse_message() {
  local state="$1" branch="$2" op="$3" override_var="$4"
  local pr_state_label="merged"
  [ "${state}" = "closed-dead" ] && pr_state_label="closed without merge"

  cat >&2 <<EOF

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✗ ${op} REFUSED — branch '${branch}' is ${state}.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

The PR for this branch has been ${pr_state_label}. Continuing to
${op} on a ${state} branch creates orphan commits that land
nowhere — the failure mode catalogued as "v1p" in the 2026-06-01
forensic session.

Per docs/adr/0017-branch-state-machine.md, the correct path is:

  1. Open a new bd issue for this follow-up work:
       bd create --title="follow-up: <what>" --type=task --priority=2

  2. Branch off main (NOT the merged-dead branch):
       git checkout main && git pull --ff-only
       python3 .claude/scripts/new_tuxlink_worktree.py \\
           --slug <slug> --issue tuxlink-<new-id> \\
           --base main --moniker <your-moniker>

  3. Land your work + open its own PR.

OVERRIDE (documented escape hatch — loud + audited):
  ${override_var}=I-know-what-Im-doing git ${op,,} ...

The override logs to dev/scratch/branch-lifecycle-overrides.log for
audit. Use sparingly; the CI nightly audit (tuxlink-ui3i) also
detects orphan-post-merge commits retroactively.
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

EOF
}

# Helper: log an override to the audit file.
branch_state_log_override() {
  local op="$1" branch="$2" state="$3"
  local log_dir log_file
  log_dir="$(git rev-parse --show-toplevel)/dev/scratch"
  log_file="${log_dir}/branch-lifecycle-overrides.log"
  mkdir -p "${log_dir}"
  printf '%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "${USER:-unknown}" \
    "${op}" \
    "${branch}" \
    "${state}" \
    >>"${log_file}"
}
