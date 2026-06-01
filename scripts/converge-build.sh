#!/usr/bin/env bash
# converge-build.sh — single-source-of-truth consolidated build for tuxlink.
#
# Killed the operator's recurring time sink: ad-hoc rebase + pnpm install +
# tauri dev sequences that lose to one or more of seven known failure modes
# documented in dev/handoffs/2026-06-01-moss-cove-hemlock-convergence-discipline-handoff.md.
#
# v1 scope (this issue: tuxlink-qepd). Codex round 2026-06-01 (3300-line
# transcript at dev/adversarial/2026-06-01-convergence-discipline-codex.md
# on bd-tuxlink-jy6p/convergence-adrev) surfaced 21 findings; v1 addresses:
#   #2  (branch classification, warn-not-refuse — full enforce → tuxlink-21j8)
#   #4  (SHA-compare untracked-vs-tracked collision)
#   #5  (.beads/issues.jsonl explicit stash)
#   #7  (pnpm dev:converged wrapper as default path)
#   #11 (audit log)
#   #13 partial (lockfile-change-detect — full topology verifier → tuxlink-pxmi)
#   #20 (--dry-run mode)
#   #21 (named stash marker)
# Deferred to sub-issues:
#   #1  (post-merge hook semantics)              → tuxlink-21j8
#   #6  (host-level dev-server lease)            → tuxlink-8d7y
#   #8  (build from disposable origin/main wt)   → tuxlink-pxmi
#   #9  (CI scheduled branch audit)              → tuxlink-ui3i
#   #18 (test fixtures for 7 failure modes)      → tuxlink-8zho
#
# Usage:
#   scripts/converge-build.sh [--fresh] [--dry-run] [--skip-launch] [--help]
#   pnpm dev:converged    # package.json wrapper — the documented default path
#
# Exit codes:
#   0   converged + launched (or converged-only if --skip-launch)
#   2   untracked-vs-tracked collision with differing content (operator decision needed)
#   3   working tree dirty in a way the script won't auto-resolve
#   4   rebase failed (probably divergent histories needing operator)
#   5   bd .beads/issues.jsonl unstaged-dirty without a clear stash path
#   6   pnpm install failed
#   7   port 1420 owner refused to release (manual --force-kill-owned needed)
#   10  script-internal error (bug)

set -euo pipefail

# ─── Constants ─────────────────────────────────────────────────────────────

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
readonly AUDIT_LOG="${REPO_ROOT}/dev/scratch/converge-build.log"
readonly STATE_FILE="${REPO_ROOT}/dev/scratch/converge-build-state.json"
readonly STASH_LABEL="converge-build/${RUN_ID}/bd-issues-jsonl"
readonly LOCKFILE="${REPO_ROOT}/pnpm-lock.yaml"

# ─── Flags ─────────────────────────────────────────────────────────────────

FLAG_FRESH=0
FLAG_DRY_RUN=0
FLAG_SKIP_LAUNCH=0
FLAG_FORCE_KILL_OWNED=0

# Filled in by stash_bd_state, consulted by trap_on_exit on abnormal exit.
PERSISTED_STASH_REF=""

# ─── ANSI helpers ──────────────────────────────────────────────────────────

if [[ -t 1 ]]; then
  readonly C_RED=$'\033[31m'
  readonly C_YELLOW=$'\033[33m'
  readonly C_GREEN=$'\033[32m'
  readonly C_BLUE=$'\033[34m'
  readonly C_BOLD=$'\033[1m'
  readonly C_DIM=$'\033[2m'
  readonly C_RESET=$'\033[0m'
else
  readonly C_RED='' C_YELLOW='' C_GREEN='' C_BLUE='' C_BOLD='' C_DIM='' C_RESET=''
fi

step() { printf '%s▶ %s%s\n' "${C_BOLD}${C_BLUE}" "$*" "${C_RESET}" >&2; }
warn() { printf '%s⚠ %s%s\n' "${C_YELLOW}" "$*" "${C_RESET}" >&2; }
ok()   { printf '%s✓ %s%s\n' "${C_GREEN}" "$*" "${C_RESET}" >&2; }
die()  { printf '%s✗ %s%s\n' "${C_RED}${C_BOLD}" "$*" "${C_RESET}" >&2; exit "${2:-10}"; }
dim()  { printf '%s%s%s\n' "${C_DIM}" "$*" "${C_RESET}" >&2; }

# ─── Trap: stash-recovery on abnormal exit ─────────────────────────────────

# Codex P1 #3 — if the script exits non-zero between stash_bd_state and
# restore_bd_state (most likely a rebase failure), the operator's bd state
# is hidden in a stash. Print the recovery command loudly so they don't
# need to grep `git stash list` for the label.
trap_on_exit() {
  local rc=$?
  if [[ "${rc}" -ne 0 && -n "${PERSISTED_STASH_REF}" ]]; then
    # Verify the stash still exists (it might have been popped already).
    if git -C "${REPO_ROOT}" stash list 2>/dev/null | grep -qF "${STASH_LABEL}"; then
      printf '\n%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n' "${C_YELLOW}" "${C_RESET}" >&2
      printf '%s⚠ STASH RECOVERY NEEDED%s\n' "${C_YELLOW}${C_BOLD}" "${C_RESET}" >&2
      printf '%sYour .beads/issues.jsonl is in stash labelled:%s\n' "${C_YELLOW}" "${C_RESET}" >&2
      printf '    %s\n' "${STASH_LABEL}" >&2
      printf '%sFind + restore with:%s\n' "${C_YELLOW}" "${C_RESET}" >&2
      printf '    git stash list | grep %q\n' "${STASH_LABEL}" >&2
      printf '    git stash pop <ref>\n' >&2
      printf '%s━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━%s\n\n' "${C_YELLOW}" "${C_RESET}" >&2
      # Audit even on abnormal exit (best-effort; might fail if disk full etc).
      audit "stash_recovery_needed" "$(printf '{"stash_label":"%s","exit_code":%d}' "${STASH_LABEL}" "${rc}")" || true
    fi
  fi
  return "${rc}"
}
trap trap_on_exit EXIT

# ─── Audit log ────────────────────────────────────────────────────────────

# Append a structured JSON line to dev/scratch/converge-build.log.
# Each line is self-contained; jq-tail-friendly for forensics.
# In --dry-run, emit to stderr instead so the scratch dir stays untouched
# (Codex P3 #7 — dry-run promised no mutations).
audit() {
  local kind="$1"; shift
  local payload="$1"; shift || true
  local ts; ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  local line
  line="$(printf '{"ts":"%s","run_id":"%s","kind":"%s","cwd":"%s","payload":%s}' \
    "${ts}" "${RUN_ID}" "${kind}" "${PWD}" "${payload:-null}")"
  if [[ "${FLAG_DRY_RUN:-0}" -eq 1 ]]; then
    printf '%s[dry-run audit] %s%s\n' "${C_DIM}" "${line}" "${C_RESET}" >&2
    return 0
  fi
  mkdir -p "$(dirname "${AUDIT_LOG}")"
  printf '%s\n' "${line}" >>"${AUDIT_LOG}"
}

# ─── Argument parse ────────────────────────────────────────────────────────

usage() {
  cat <<EOF
converge-build.sh — single-source-of-truth consolidated tuxlink build.

USAGE:
  scripts/converge-build.sh [OPTIONS]
  pnpm dev:converged                 # wrapper, the documented default path

OPTIONS:
  --fresh                  Wipe node_modules + src-tauri/target before install.
                           Default: wipe only when pnpm-lock.yaml changed since
                           the previous run (per recorded state).
  --skip-launch            Converge (rebase + install + build deps) but do not
                           launch \`tauri dev\`.
  --dry-run                Print the plan — branch classification, untracked
                           collision decisions, install mode, processes to
                           kill, launch command — without mutating anything.
  --force-kill-owned       Allow killing tauri/vite/tuxlink processes even if
                           they appear to be owned by another worktree. v1
                           kills any tauri/vite PID; this flag is reserved for
                           the lease enforcement landing in tuxlink-8d7y.
  -h, --help               This message.

KNOWN FAILURE MODES THIS HANDLES:
  1. Operator's branch N commits behind origin/main → rebase forward
  2. pnpm install reports "up to date" but symlinks stale → lockfile diff
  3. .beads/issues.jsonl staged by bd, blocking rebase → named stash
  4. Untracked path same as tracked on origin/main, blocking rebase →
     SHA-compare; auto-remove identical, stop-and-ask on differing
  5. Stale src-tauri/target/debug/tuxlink binary → wipe when HEAD moved
  6. Parallel \`tauri dev\` on port 1420 (strictPort) → kill before launch
  7. Bare-branch warning (orphan-post-merge): warn classification; v2 hooks
     in tuxlink-21j8 will refuse-on-dead

NOT HANDLED IN v1 (see sub-issues):
  - Host-level dev-server lease for safe parallel work       → tuxlink-8d7y
  - Build from disposable worktree at exactly origin/main    → tuxlink-pxmi
  - Refuse commits on merged-dead branches                   → tuxlink-21j8
  - CI scheduled audit catching non-Claude bypasses          → tuxlink-ui3i
  - Test fixtures for all 7 modes                            → tuxlink-8zho

REQUIREMENTS: Linux (bash 4+, ss or lsof, sha256sum, pgrep, pkill, jq, git, pnpm).
              tuxlink targets Linux/Pi development; macOS/Windows porting is
              out of scope for v1.

AUDIT LOG: ${AUDIT_LOG}
STATE FILE: ${STATE_FILE}
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --fresh)             FLAG_FRESH=1 ;;
      --dry-run)           FLAG_DRY_RUN=1 ;;
      --skip-launch)       FLAG_SKIP_LAUNCH=1 ;;
      --force-kill-owned)  FLAG_FORCE_KILL_OWNED=1 ;;
      -h|--help)           usage; exit 0 ;;
      *) die "unknown flag: $1 (try --help)" 10 ;;
    esac
    shift
  done
}

# ─── Phase: classify the current branch ───────────────────────────────────

# Categories (Codex P0 #2 — warn-not-refuse in v1; tuxlink-21j8 lifts this to
# hook-level refuse-on-dead). Output JSON for the audit log.
classify_branch() {
  local branch; branch="$(git -C "${REPO_ROOT}" symbolic-ref --short HEAD 2>/dev/null || echo DETACHED)"
  local sha;    sha="$(git -C "${REPO_ROOT}" rev-parse HEAD)"
  local merged_pr_state="none"
  local open_pr_state="none"

  # gh may be absent or unauthenticated; fail soft (warn, not die).
  if command -v gh >/dev/null 2>&1; then
    # Open PR for this branch?
    if gh pr view "${branch}" --json state,number 2>/dev/null | grep -q '"state":"OPEN"'; then
      open_pr_state="$(gh pr view "${branch}" --json number 2>/dev/null | sed -n 's/.*"number":\([0-9]*\).*/\1/p')"
    fi
    # Most-recent MERGED PR for this branch?
    if gh pr list --head "${branch}" --state merged --json number,mergedAt --limit 1 2>/dev/null | grep -q '"mergedAt"'; then
      merged_pr_state="$(gh pr list --head "${branch}" --state merged --json number --limit 1 2>/dev/null | sed -n 's/.*"number":\([0-9]*\).*/\1/p')"
    fi
  fi

  local category="unknown"
  local note=""
  case "${branch}" in
    main)
      category="protected-main"
      note="building from main directly — convergence is trivially satisfied"
      ;;
    task-amd-main-ui|task-*)
      category="operator-local"
      note="operator's named work branch; rebase forward and proceed"
      ;;
    bd-*)
      if [[ "${open_pr_state}" != "none" ]]; then
        category="bd-issue-pr-open"; note="open PR #${open_pr_state}"
      elif [[ "${merged_pr_state}" != "none" ]]; then
        category="bd-issue-merged-dead"
        note="PR #${merged_pr_state} merged — branch should be retired (v2: pre-commit/pre-push refuses)"
      else
        category="bd-issue-active"; note="bd-issue branch, no PR yet"
      fi
      ;;
    agent-*)
      category="agent-throwaway"
      note="throwaway agent branch"
      ;;
    release-please--*|dependabot/*)
      category="bot"
      note="bot-owned automation branch"
      ;;
    *)
      if [[ "${open_pr_state}" != "none" ]]; then
        category="adhoc-pr-open"; note="open PR #${open_pr_state}"
      elif [[ "${merged_pr_state}" != "none" ]]; then
        category="adhoc-merged-dead"
        note="PR #${merged_pr_state} merged — likely orphan-post-merge"
      else
        category="adhoc-unowned"; note="no bd-id prefix, no PR — verify intent"
      fi
      ;;
  esac

  printf '{"branch":"%s","sha":"%s","category":"%s","note":"%s"}\n' \
    "${branch}" "${sha}" "${category}" "${note}"
}

# ─── Phase: untracked-vs-tracked collision resolution ─────────────────────

# Codex P1 #4. Walk operator's untracked files; for each that exists on
# origin/main, SHA-compare. Identical bytes → auto-remove (the tracked copy
# is the source of truth). Differing bytes → STOP-AND-ASK.
resolve_untracked_collisions() {
  local dry="$1"
  local origin_main_sha; origin_main_sha="$(git -C "${REPO_ROOT}" rev-parse origin/main)"
  local conflict_count=0
  local resolved_identical=()
  local resolved_differing=()

  # git ls-files --others --exclude-standard prints relative paths.
  local untracked
  untracked="$(git -C "${REPO_ROOT}" ls-files --others --exclude-standard)"
  [[ -z "${untracked}" ]] && { echo '{"identical_removed":0,"differing_blocked":0}'; return 0; }

  while IFS= read -r path; do
    [[ -z "${path}" ]] && continue
    # Does origin/main track this path?
    if ! git -C "${REPO_ROOT}" cat-file -e "${origin_main_sha}:${path}" 2>/dev/null; then
      continue  # untracked locally + not tracked upstream → leave it alone
    fi
    # SHA-compare local bytes vs origin/main blob.
    local local_sha; local_sha="$(git -C "${REPO_ROOT}" hash-object "${REPO_ROOT}/${path}")"
    local main_sha; main_sha="$(git -C "${REPO_ROOT}" rev-parse "${origin_main_sha}:${path}")"
    if [[ "${local_sha}" == "${main_sha}" ]]; then
      resolved_identical+=("${path}")
      if [[ "${dry}" -eq 0 ]]; then
        rm -f "${REPO_ROOT}/${path}"
      fi
    else
      resolved_differing+=("${path}")
      conflict_count=$((conflict_count + 1))
    fi
  done <<<"${untracked}"

  if [[ "${#resolved_identical[@]}" -gt 0 ]]; then
    if [[ "${dry}" -eq 1 ]]; then
      dim "  [dry-run] would remove ${#resolved_identical[@]} identical-byte untracked files:"
    else
      ok "removed ${#resolved_identical[@]} identical-byte untracked files (origin/main is SoT):"
    fi
    for f in "${resolved_identical[@]}"; do dim "    ${f}"; done
  fi

  if [[ "${#resolved_differing[@]}" -gt 0 ]]; then
    warn "untracked-vs-tracked collision with differing bytes (${#resolved_differing[@]} files):"
    for f in "${resolved_differing[@]}"; do
      warn "    ${f}"
      warn "      tracked SHA:    ${main_sha}"
      warn "      local SHA:      $(git -C "${REPO_ROOT}" hash-object "${REPO_ROOT}/${f}")"
    done
    warn ""
    warn "operator must choose for each: commit / rename / archive / discard."
    warn "v1 does not auto-resolve differing-bytes collisions; please run:"
    warn "    git diff --no-index origin/main:<path> <path>"
    warn "for each, then commit/rename/archive/discard as appropriate, then re-run."
    audit "untracked_collision_blocked" \
      "$(printf '{"differing":["%s"],"identical":["%s"]}' \
          "$(IFS=',';echo "${resolved_differing[*]}")" \
          "$(IFS=',';echo "${resolved_identical[*]}")")"
    exit 2
  fi

  printf '{"identical_removed":%d,"differing_blocked":%d}\n' \
    "${#resolved_identical[@]}" "${conflict_count}"
}

# ─── Phase: stash bd state ────────────────────────────────────────────────

# Codex P1 #5. bd auto-stages .beads/issues.jsonl on every command; rebase
# refuses while it's dirty. Named stash so concurrent runs do not clobber
# each other (Codex P3 #21).
stash_bd_state() {
  local dry="$1"
  local has_bd_jsonl=0
  local has_other_dirty=0

  # Anything to stash at all?
  if ! git -C "${REPO_ROOT}" diff --quiet --cached -- .beads/issues.jsonl 2>/dev/null; then
    has_bd_jsonl=1
  fi
  if ! git -C "${REPO_ROOT}" diff --quiet -- .beads/issues.jsonl 2>/dev/null; then
    has_bd_jsonl=1
  fi
  # Anything OTHER than .beads/issues.jsonl dirty?
  local other_dirty
  other_dirty="$(git -C "${REPO_ROOT}" status --porcelain --untracked-files=no 2>/dev/null | grep -v ' \.beads/issues\.jsonl$' || true)"
  if [[ -n "${other_dirty}" ]]; then
    has_other_dirty=1
  fi

  if [[ "${has_other_dirty}" -eq 1 ]]; then
    warn "working tree has dirty tracked files besides .beads/issues.jsonl:"
    echo "${other_dirty}" | while IFS= read -r line; do warn "    ${line}"; done
    warn ""
    warn "v1 does not auto-stash arbitrary dirty files (too easy to lose work)."
    warn "Either commit, stash by name, or discard them explicitly, then re-run."
    audit "dirty_blocked" "$(printf '{"summary":%s}' "$(git -C "${REPO_ROOT}" status --porcelain --untracked-files=no | jq -Rsc . 2>/dev/null || echo '"jq missing"')")"
    exit 3
  fi

  if [[ "${has_bd_jsonl}" -eq 0 ]]; then
    dim "no bd state to stash"
    echo "none"
    return 0
  fi

  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: git stash push -m '${STASH_LABEL}' -- .beads/issues.jsonl"
    echo "would-stash"
    return 0
  fi

  if ! git -C "${REPO_ROOT}" stash push -m "${STASH_LABEL}" -- .beads/issues.jsonl >&2; then
    die "failed to stash .beads/issues.jsonl" 5
  fi
  # Capture the freshly-created stash@{0} ref so trap_on_exit can locate it
  # even if the message-match heuristic fails (Codex P1 #3).
  PERSISTED_STASH_REF="stash@{0}"
  ok "stashed .beads/issues.jsonl as '${STASH_LABEL}' (${PERSISTED_STASH_REF})"
  echo "stashed"
}

# Restore the named stash by exact label match (safe across concurrent runs).
restore_bd_state() {
  local stashed="$1"
  local dry="$2"
  [[ "${stashed}" == "stashed" ]] || return 0
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: git stash pop <named stash>"
    return 0
  fi
  # `git stash push -m <msg>` records as "stash@{N}: On <branch>: <msg>".
  # Match by the unique RUN_ID-suffixed label to disambiguate concurrent runs.
  local stash_ref
  stash_ref="$(git -C "${REPO_ROOT}" stash list \
    | awk -v label="${STASH_LABEL}" '$0 ~ label {sub(/:.*/,"",$0); print; exit}' \
    || true)"
  if [[ -z "${stash_ref}" ]]; then
    warn "could not find stash labelled '${STASH_LABEL}' — leaving worktree as-is"
    return 0
  fi
  if ! git -C "${REPO_ROOT}" stash pop "${stash_ref}" >&2; then
    warn "stash pop failed; .beads/issues.jsonl will remain in stash '${stash_ref}'"
    return 0
  fi
  ok "restored .beads/issues.jsonl from stash"
}

# ─── Phase: rebase onto origin/main ───────────────────────────────────────

fetch_prune() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: git fetch origin --prune"
    return 0
  fi
  step "git fetch origin --prune"
  git -C "${REPO_ROOT}" fetch origin --prune >&2
}

rebase_forward() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: git rebase origin/main"
    return 0
  fi
  step "git rebase origin/main"
  if ! git -C "${REPO_ROOT}" rebase origin/main >&2; then
    warn "rebase failed. The script will leave the worktree in 'rebasing' state."
    warn "Resolve conflicts, finish the rebase with 'git rebase --continue',"
    warn "then re-run scripts/converge-build.sh to complete convergence."
    audit "rebase_failed" "null"
    exit 4
  fi
  ok "rebased onto origin/main"
}

# ─── Phase: install + build artifact freshness ────────────────────────────

# Decide what to wipe. Codex P1 #1 + handoff catalog modes 5+6:
#   - node_modules: wipe when --fresh, or pnpm-lock.yaml changed since last run.
#   - src-tauri/target: wipe additionally when HEAD moved since last run (the
#     compiled tuxlink binary is keyed to source SHA; rebase invalidates it).
# Returns one JSON object summarizing both decisions.
maybe_wipe_build_artifacts() {
  local dry="$1"
  local cur_lockfile_sha=""
  [[ -f "${LOCKFILE}" ]] && cur_lockfile_sha="$(sha256sum "${LOCKFILE}" | cut -d' ' -f1)"
  local cur_head_sha; cur_head_sha="$(git -C "${REPO_ROOT}" rev-parse HEAD)"
  local prev_lockfile_sha=""
  local prev_head_sha=""
  if [[ -f "${STATE_FILE}" ]]; then
    prev_lockfile_sha="$(jq -r '.last_lockfile_sha // empty' "${STATE_FILE}" 2>/dev/null || true)"
    prev_head_sha="$(jq -r '.last_head_sha // empty' "${STATE_FILE}" 2>/dev/null || true)"
  fi

  # node_modules decision
  local nm_reason=""
  if [[ "${FLAG_FRESH}" -eq 1 ]]; then
    nm_reason="--fresh flag"
  elif [[ "${cur_lockfile_sha}" != "${prev_lockfile_sha}" ]]; then
    if [[ -z "${prev_lockfile_sha}" ]]; then
      nm_reason="no prior recorded state"
    else
      nm_reason="pnpm-lock.yaml changed (${prev_lockfile_sha:0:8} → ${cur_lockfile_sha:0:8})"
    fi
  fi

  # cargo target decision
  local target_reason=""
  if [[ "${FLAG_FRESH}" -eq 1 ]]; then
    target_reason="--fresh flag"
  elif [[ "${cur_head_sha}" != "${prev_head_sha}" ]]; then
    if [[ -z "${prev_head_sha}" ]]; then
      target_reason="no prior recorded state"
    else
      target_reason="HEAD moved (${prev_head_sha:0:8} → ${cur_head_sha:0:8})"
    fi
  fi

  if [[ -z "${nm_reason}" && -z "${target_reason}" ]]; then
    dim "build artifacts: keep (lockfile + HEAD unchanged since last run)"
    echo '{"node_modules":"keep","target":"keep"}'
    return 0
  fi

  if [[ -n "${nm_reason}" ]]; then
    if [[ "${dry}" -eq 1 ]]; then
      dim "  [dry-run] would wipe node_modules (${nm_reason})"
    else
      step "wiping node_modules (${nm_reason})"
      rm -rf "${REPO_ROOT}/node_modules"
    fi
  else
    dim "node_modules: keep (lockfile unchanged)"
  fi

  if [[ -n "${target_reason}" ]]; then
    if [[ "${dry}" -eq 1 ]]; then
      dim "  [dry-run] would wipe src-tauri/target (${target_reason})"
    else
      step "wiping src-tauri/target (${target_reason})"
      rm -rf "${REPO_ROOT}/src-tauri/target"
    fi
  else
    dim "src-tauri/target: keep (HEAD unchanged)"
  fi

  printf '{"node_modules":"%s","node_modules_reason":"%s","target":"%s","target_reason":"%s"}\n' \
    "$([[ -n "${nm_reason}" ]] && echo wiped || echo keep)" "${nm_reason}" \
    "$([[ -n "${target_reason}" ]] && echo wiped || echo keep)" "${target_reason}"
}

pnpm_install() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: pnpm install --frozen-lockfile"
    return 0
  fi
  step "pnpm install --frozen-lockfile"
  if ! (cd "${REPO_ROOT}" && pnpm install --frozen-lockfile); then
    die "pnpm install failed" 6
  fi
  ok "pnpm install complete"
}

# ─── Phase: kill stale tauri/vite/tuxlink processes ───────────────────────

# Codex P1 #6: full lease lives in tuxlink-8d7y. v1 logs the PIDs we kill so
# multi-worktree forensics is possible after the fact.
kill_stale_dev_processes() {
  local dry="$1"
  local pids_killed=()
  local pid_patterns=(
    "tauri dev"
    "target/debug/tuxlink"
    "node.*vite"
  )
  local matches
  matches="$(pgrep -af "tauri dev|target/debug/tuxlink|node.*vite" 2>/dev/null || true)"
  if [[ -z "${matches}" ]]; then
    dim "no stale tauri/vite/tuxlink processes detected"
    echo '[]'
    return 0
  fi

  echo "${matches}" | while IFS= read -r line; do
    dim "  detected: ${line}"
  done >&2

  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: pkill -f 'tauri dev|target/debug/tuxlink|node.*vite'"
    echo '[]'
    return 0
  fi

  # Capture PIDs into an audit-loggable list, then kill them.
  while IFS= read -r pid; do
    [[ -z "${pid}" ]] && continue
    pids_killed+=("${pid}")
  done < <(pgrep -f "tauri dev|target/debug/tuxlink|node.*vite" 2>/dev/null || true)

  if [[ "${#pids_killed[@]}" -gt 0 ]]; then
    warn "killing ${#pids_killed[@]} stale process(es): ${pids_killed[*]}"
    warn "(tuxlink-8d7y will replace this blanket kill with a proper lease)"
    pkill -f "tauri dev|target/debug/tuxlink|node.*vite" 2>/dev/null || true
    sleep 1
  fi
  printf '['
  local first=1
  for pid in "${pids_killed[@]}"; do
    [[ ${first} -eq 0 ]] && printf ','
    printf '%s' "${pid}"
    first=0
  done
  printf ']'
}

# Codex P1 #4 — after the blanket pkill, verify port 1420 is actually free
# before claiming convergence. If something still holds it (e.g. a process
# that doesn't match our pattern), exit 7 with operator instructions so the
# audit log does not falsely record "converged" for a build that can't launch.
verify_port_free() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: check port 1420 is free; exit 7 if held"
    return 0
  fi
  # ss is the canonical tool on modern Linux; lsof as fallback.
  local owner=""
  if command -v ss >/dev/null 2>&1; then
    owner="$(ss -lntp 2>/dev/null | awk '$4 ~ /:1420$/ {print; exit}' || true)"
  elif command -v lsof >/dev/null 2>&1; then
    owner="$(lsof -i :1420 -sTCP:LISTEN 2>/dev/null | tail -n +2 | head -1 || true)"
  else
    warn "neither ss nor lsof present — cannot verify port 1420 status; continuing"
    return 0
  fi
  if [[ -n "${owner}" ]]; then
    warn "port 1420 still occupied after pkill:"
    warn "    ${owner}"
    warn ""
    warn "v1's blanket pkill matched 'tauri dev|target/debug/tuxlink|node.*vite'"
    warn "but something else is holding the port. Identify + free it manually,"
    warn "then re-run. The proper host-level lease lands in tuxlink-8d7y."
    audit "port_1420_busy" "$(printf '{"owner":%s}' "$(printf '%s' "${owner}" | jq -Rs . 2>/dev/null || echo '"unknown"')")"
    exit 7
  fi
  ok "port 1420 is free"
}

# ─── Phase: launch ────────────────────────────────────────────────────────

launch_tauri_dev() {
  local dry="$1"
  if [[ "${FLAG_SKIP_LAUNCH}" -eq 1 ]]; then
    ok "convergence complete; --skip-launch set, not invoking tauri dev"
    echo "skipped"
    return 0
  fi
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: cd ${REPO_ROOT} && pnpm tauri dev"
    echo "would-launch"
    return 0
  fi
  step "launching: cd ${REPO_ROOT} && pnpm tauri dev"
  ok "converged at HEAD=$(git -C "${REPO_ROOT}" rev-parse --short HEAD) (origin/main=$(git -C "${REPO_ROOT}" rev-parse --short origin/main))"
  ok "audit log: ${AUDIT_LOG}"
  # exec replaces the shell so the script's PID becomes the tauri-dev PID,
  # which is what the operator's terminal will see + Ctrl-C cleanly.
  audit "launching" "$(printf '{"head":"%s","origin_main":"%s"}' \
    "$(git -C "${REPO_ROOT}" rev-parse HEAD)" \
    "$(git -C "${REPO_ROOT}" rev-parse origin/main)")"
  cd "${REPO_ROOT}"
  exec pnpm tauri dev
}

# ─── State-file writer (for the next run's lockfile-change-detect) ────────

write_state() {
  local dry="$1"
  [[ "${dry}" -eq 1 ]] && return 0
  mkdir -p "$(dirname "${STATE_FILE}")"
  local cur_lockfile_sha=""
  [[ -f "${LOCKFILE}" ]] && cur_lockfile_sha="$(sha256sum "${LOCKFILE}" | cut -d' ' -f1)"
  cat >"${STATE_FILE}" <<EOF
{
  "last_run_id": "${RUN_ID}",
  "last_lockfile_sha": "${cur_lockfile_sha}",
  "last_head_sha": "$(git -C "${REPO_ROOT}" rev-parse HEAD)",
  "last_origin_main_sha": "$(git -C "${REPO_ROOT}" rev-parse origin/main)"
}
EOF
}

# ─── Main ─────────────────────────────────────────────────────────────────

main() {
  parse_args "$@"

  step "tuxlink converge-build v1 (run ${RUN_ID})"
  if [[ "${FLAG_DRY_RUN}" -eq 1 ]]; then
    warn "DRY RUN — no mutations will be performed"
  fi

  # 0. Fetch fresh origin refs BEFORE classify + collision detection so we
  # do not act on a stale origin/main (Codex P1 #2).
  step "phase 1/8 — git fetch origin --prune"
  fetch_prune "${FLAG_DRY_RUN}"
  audit "fetched" "$(printf '{"origin_main":"%s"}' \
    "$(git -C "${REPO_ROOT}" rev-parse origin/main 2>/dev/null || echo unknown)")"

  # 1. Classify branch (warn-not-refuse in v1; tuxlink-21j8 lifts to refuse).
  step "phase 2/8 — branch classification"
  local branch_json; branch_json="$(classify_branch)"
  echo "  ${branch_json}" >&2
  audit "branch_classified" "${branch_json}"
  case "${branch_json}" in
    *bd-issue-merged-dead*|*adhoc-merged-dead*)
      warn "this branch has a MERGED PR — committing here is the orphan-post-merge mode."
      warn "v2 (tuxlink-21j8) will refuse via pre-commit hook. v1 only warns."
      ;;
    *adhoc-unowned*)
      warn "unowned branch (no bd-id prefix, no PR). verify intent before continuing."
      ;;
  esac

  # 2. Resolve untracked collisions (auto for identical, stop for differing).
  step "phase 3/8 — untracked-vs-tracked collision check"
  local collision_json; collision_json="$(resolve_untracked_collisions "${FLAG_DRY_RUN}")"
  audit "untracked_resolved" "${collision_json}"

  # 3. Stash bd state.
  step "phase 4/8 — stash .beads/issues.jsonl"
  local stash_state; stash_state="$(stash_bd_state "${FLAG_DRY_RUN}")"
  audit "bd_stash" "$(printf '{"state":"%s","label":"%s"}' "${stash_state}" "${STASH_LABEL}")"

  # 4. Rebase forward.
  step "phase 5/8 — rebase onto origin/main"
  rebase_forward "${FLAG_DRY_RUN}"
  audit "rebased" "$(printf '{"head":"%s","origin_main":"%s"}' \
    "$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)" \
    "$(git -C "${REPO_ROOT}" rev-parse origin/main 2>/dev/null || echo unknown)")"

  # 5. Pop bd stash (best-effort; failure does not block; trap_on_exit
  # would have surfaced a recovery command had rebase failed above).
  restore_bd_state "${stash_state}" "${FLAG_DRY_RUN}"
  PERSISTED_STASH_REF=""  # disarm trap_on_exit's recovery message

  # 6. node_modules + cargo target wipe-or-keep + install.
  step "phase 6/8 — build artifact freshness"
  local wipe_json; wipe_json="$(maybe_wipe_build_artifacts "${FLAG_DRY_RUN}")"
  audit "install_decision" "${wipe_json}"
  pnpm_install "${FLAG_DRY_RUN}"

  # 7. Kill stale dev processes + VERIFY port 1420 is actually free.
  step "phase 7/8 — kill stale tauri/vite/tuxlink processes"
  local killed_json; killed_json="$(kill_stale_dev_processes "${FLAG_DRY_RUN}")"
  audit "processes_killed" "$(printf '{"pids":%s}' "${killed_json}")"
  verify_port_free "${FLAG_DRY_RUN}"

  # 8. Write state + launch. State write happens AFTER port verification so
  # a stalled run does not poison next-run's lockfile/HEAD baselines.
  step "phase 8/8 — launch"
  write_state "${FLAG_DRY_RUN}"
  audit "convergence_complete_pre_launch" "$(printf '{"dry_run":%d}' "${FLAG_DRY_RUN}")"
  launch_tauri_dev "${FLAG_DRY_RUN}"
}

main "$@"
