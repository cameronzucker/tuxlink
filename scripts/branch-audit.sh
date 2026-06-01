#!/usr/bin/env bash
# branch-audit.sh — survey all remote branches + flag orphan-post-merge.
#
# Companion to .githooks/pre-commit + .githooks/pre-push (ADR 0017's
# branch lifecycle state machine). The local hooks only enforce when a
# Claude/Codex/shell-driven commit or push fires from a clone with
# .githooks installed; this script is the INDEPENDENT auditor that
# catches the cases the local hooks miss:
#
#   - Web-UI merges (the operator merged a PR via github.com without
#     pulling locally afterward) leaving the local branch state
#     "looks-active" until the next pull.
#   - Force-pushes from a session without the hooks installed.
#   - Agents bypassing the hook via the documented override sentinel
#     (the override is loud + audited at dev/scratch/, but the audit
#     log is local-only; this CI step is the global view).
#   - Direct API merges via `gh pr merge` from a script.
#
# Runs from .github/workflows/branch-audit.yml on nightly + manual
# dispatch. Implements Codex 2026-06-01 P1 #9:
#
#   "Claude hooks are not universal enforcement. .claude/hooks only
#    protects Claude Code tool calls. Codex CLI, human shell, GitHub
#    web merges, scripts that call gh pr merge, and API merges bypass
#    the proposed hook. Add an independent auditor: CI/scheduled branch
#    audit, local pre-push protection where feasible."
#
# Output:
#   - stdout: human-readable summary of orphan-post-merge branches.
#   - $GITHUB_OUTPUT: structured fields the workflow uses to update a
#     tracking issue.
#   - Exit 0 if the audit completed successfully (regardless of
#     whether orphans were found — the workflow decides whether to
#     gate downstream steps on findings).
#   - Exit 1 only if the audit COULD NOT RUN (gh unavailable, no
#     branches enumerable, internal error).

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
# shellcheck source=.githooks/lib/branch-state.sh
. "${REPO_ROOT}/.githooks/lib/branch-state.sh"

# Branches the audit ignores entirely: the protected refs + bot refs +
# the well-known integration branches we know are still alive.
_audit_ignore_branch() {
  case "$1" in
    main|master|release|production) return 0 ;;
    release-please--*) return 0 ;;
    dependabot/*) return 0 ;;
    github-actions/*) return 0 ;;
    renovate/*) return 0 ;;
    *) return 1 ;;
  esac
}

# Get the merged-PR's merge-commit SHA on main for a given branch.
# Returns empty string if no merged PR exists.
_audit_merged_tip() {
  local branch="$1"
  gh pr list --head "${branch}" --state merged --limit 1 \
    --json mergeCommit \
    --jq '.[0].mergeCommit.oid // empty' 2>/dev/null || true
}

# Get the head of origin/<branch>, or empty string if branch doesn't
# exist on origin anymore.
_audit_remote_head() {
  local branch="$1"
  git rev-parse "refs/remotes/origin/${branch}" 2>/dev/null || true
}

# Count commits on origin/<branch> that are NOT reachable from main.
_audit_commits_ahead_of_main() {
  local branch="$1"
  git rev-list --count "origin/${branch}" --not "origin/main" 2>/dev/null || echo 0
}

main() {
  if ! command -v gh >/dev/null 2>&1; then
    printf '✗ gh CLI not available — cannot run branch audit\n' >&2
    exit 1
  fi

  printf '▶ Fetching all origin refs (--prune)…\n' >&2
  git fetch origin --prune >&2

  # Enumerate all remote branch names.
  local branches
  branches="$(git for-each-ref --format='%(refname:short)' refs/remotes/origin/ \
              | sed 's|^origin/||' \
              | grep -v '^HEAD$' \
              || true)"

  if [[ -z "${branches}" ]]; then
    printf '✗ no remote branches found — cannot enumerate\n' >&2
    exit 1
  fi

  local orphan_branches=()
  local closed_dead_with_ahead=()
  local unknown_branches=()
  local total=0
  local audited=0

  while IFS= read -r branch; do
    total=$((total + 1))
    [[ -z "${branch}" ]] && continue
    if _audit_ignore_branch "${branch}"; then
      continue
    fi
    audited=$((audited + 1))

    local state
    state="$(classify_branch_state "${branch}")"

    case "${state}" in
      merged-dead)
        # Did the branch get post-merge commits? Compare remote HEAD with
        # the merge-commit SHA on main.
        local merged_tip remote_head ahead
        merged_tip="$(_audit_merged_tip "${branch}")"
        remote_head="$(_audit_remote_head "${branch}")"
        ahead="$(_audit_commits_ahead_of_main "${branch}")"
        if [[ -n "${remote_head}" && "${ahead}" -gt 0 ]]; then
          # Branch is merged BUT still has commits not on main → orphan-post-merge.
          orphan_branches+=("${branch}|${remote_head}|${ahead}|${merged_tip:-?}")
        fi
        ;;
      closed-dead)
        local ahead
        ahead="$(_audit_commits_ahead_of_main "${branch}")"
        if [[ "${ahead}" -gt 0 ]]; then
          closed_dead_with_ahead+=("${branch}|${ahead}")
        fi
        ;;
      unknown)
        unknown_branches+=("${branch}")
        ;;
      *)
        : ;;
    esac
  done <<<"${branches}"

  # ─── Summary ─────────────────────────────────────────────────────────

  printf '\n=== tuxlink branch lifecycle audit ===\n' >&2
  printf 'date: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >&2
  printf 'branches enumerated: %d\n' "${total}" >&2
  printf 'branches audited (after ignoring main/bot/protected): %d\n' "${audited}" >&2
  printf 'orphan-post-merge branches found: %d\n' "${#orphan_branches[@]}" >&2
  printf 'closed-without-merge branches with extra commits: %d\n' "${#closed_dead_with_ahead[@]}" >&2
  printf 'unknown-classification branches: %d\n' "${#unknown_branches[@]}" >&2
  printf '\n' >&2

  if [[ "${#orphan_branches[@]}" -eq 0 \
        && "${#closed_dead_with_ahead[@]}" -eq 0 ]]; then
    printf '✓ No orphan-post-merge commits detected. Branch lifecycle is clean.\n' >&2
    # Emit a "clean" finding to GITHUB_OUTPUT so the workflow can close
    # any existing tracking issue.
    if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
      {
        echo "orphan_count=0"
        echo "closed_dead_with_ahead_count=0"
        echo "unknown_count=${#unknown_branches[@]}"
        echo 'clean=true'
      } >>"${GITHUB_OUTPUT}"
    fi
    exit 0
  fi

  printf '⚠ FINDINGS\n' >&2
  printf '\n' >&2
  if [[ "${#orphan_branches[@]}" -gt 0 ]]; then
    printf '== Orphan-post-merge branches ==\n' >&2
    printf '(merged PR + commits ahead of main on the same branch — the v1p failure mode)\n\n' >&2
    for entry in "${orphan_branches[@]}"; do
      IFS='|' read -r b remote_head ahead merged_tip <<<"${entry}"
      printf '  %s\n' "${b}" >&2
      printf '    remote HEAD:  %s\n' "${remote_head}" >&2
      printf '    commits ahead of main: %s\n' "${ahead}" >&2
      printf '    merged-tip:   %s\n' "${merged_tip}" >&2
    done
    printf '\n' >&2
  fi
  if [[ "${#closed_dead_with_ahead[@]}" -gt 0 ]]; then
    printf '== Closed-without-merge branches with extra commits ==\n' >&2
    for entry in "${closed_dead_with_ahead[@]}"; do
      IFS='|' read -r b ahead <<<"${entry}"
      printf '  %s (%s commits ahead of main)\n' "${b}" "${ahead}" >&2
    done
    printf '\n' >&2
  fi

  # ─── Build issue body for the workflow to post / update ──────────────

  local body_file="${RUNNER_TEMP:-/tmp}/branch-audit-body.md"
  {
    echo "# Branch lifecycle audit — $(date -u +%Y-%m-%d)"
    echo ""
    echo "_Generated by \`.github/workflows/branch-audit.yml\` on $(date -u +%Y-%m-%dT%H:%M:%SZ)._"
    echo ""
    echo "This is the independent auditor described in [ADR 0017](docs/adr/0017-branch-state-machine.md): the local \`.githooks/pre-commit\` + \`.githooks/pre-push\` catch commits/pushes from agents and shells with the hooks installed; this CI step is the global view that catches web-UI merges, hookless sessions, and \`--no-verify\` bypasses retroactively."
    echo ""
    if [[ "${#orphan_branches[@]}" -gt 0 ]]; then
      echo "## Orphan-post-merge branches (${#orphan_branches[@]})"
      echo ""
      echo "These branches have a merged PR but commits AHEAD of \`main\`. The commits are unreachable from any open PR — the v1p failure mode catalogued on 2026-06-01."
      echo ""
      echo "| Branch | Commits ahead of main | Merge-commit | Remote HEAD |"
      echo "|---|---:|---|---|"
      for entry in "${orphan_branches[@]}"; do
        IFS='|' read -r b remote_head ahead merged_tip <<<"${entry}"
        echo "| \`${b}\` | ${ahead} | \`${merged_tip:0:12}\` | \`${remote_head:0:12}\` |"
      done
      echo ""
      echo "**Remediation:**"
      echo ""
      echo "1. Open a fresh \`bd-<id>/<slug>\` branch off \`main\` for the orphaned work."
      echo "2. Cherry-pick or rebuild the orphan commits there."
      echo "3. Open a PR + ship through normal review."
      echo "4. Delete the dead branch on origin: \`git push origin --delete <branch>\`."
      echo ""
    fi
    if [[ "${#closed_dead_with_ahead[@]}" -gt 0 ]]; then
      echo "## Closed-without-merge branches with extra commits (${#closed_dead_with_ahead[@]})"
      echo ""
      echo "These branches' PRs were closed without merging, but commits exist ahead of \`main\`. Often this is intentional (abandoned exploration); if the work is still relevant, it should be revived in a new \`bd-<id>/<slug>\` branch."
      echo ""
      echo "| Branch | Commits ahead of main |"
      echo "|---|---:|"
      for entry in "${closed_dead_with_ahead[@]}"; do
        IFS='|' read -r b ahead <<<"${entry}"
        echo "| \`${b}\` | ${ahead} |"
      done
      echo ""
    fi
    if [[ "${#unknown_branches[@]}" -gt 0 ]]; then
      echo "## Unknown-classification branches (${#unknown_branches[@]})"
      echo ""
      echo "These branches could not be classified — typically a transient \`gh\` failure during the audit. They will be re-checked tomorrow."
      echo ""
      echo "<details><summary>List</summary>"
      echo ""
      for b in "${unknown_branches[@]}"; do
        echo "- \`${b}\`"
      done
      echo ""
      echo "</details>"
      echo ""
    fi
    echo "---"
    echo "_This issue is auto-updated nightly. The workflow file is \`.github/workflows/branch-audit.yml\` and the audit logic is \`scripts/branch-audit.sh\`._"
  } >"${body_file}"

  # Emit structured fields to GITHUB_OUTPUT (for workflow consumption).
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    {
      echo "orphan_count=${#orphan_branches[@]}"
      echo "closed_dead_with_ahead_count=${#closed_dead_with_ahead[@]}"
      echo "unknown_count=${#unknown_branches[@]}"
      echo "clean=false"
      echo "body_file=${body_file}"
    } >>"${GITHUB_OUTPUT}"
  fi
  printf 'audit body written to %s\n' "${body_file}" >&2
  exit 0
}

main "$@"
