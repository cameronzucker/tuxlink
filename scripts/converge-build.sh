#!/usr/bin/env bash
# converge-build.sh — single-source-of-truth consolidated build for tuxlink.
#
# Killed the operator's recurring time sink: ad-hoc rebase + pnpm install +
# tauri dev sequences that lose to one or more of seven known failure modes
# documented in dev/handoffs/2026-06-01-moss-cove-hemlock-convergence-discipline-handoff.md.
#
# v2 (this issue: tuxlink-pxmi). Codex 2026-06-01 P1 #8: "Source of truth
# remains ambiguous — rebasing task-amd-main-ui and restoring a stash can
# produce a build from `origin/main + local handoff/docs + dirty overlay`,
# while the banner prints only one HEAD SHA. That is not clearly 'actual
# project state.' Best option: build from a disposable/managed worktree
# checked out at `origin/main`, leaving the operator handoff branch out
# of runtime builds."
#
# Build target is .local/converge-build-worktree/ at the repo root —
# a persistent throwaway worktree with detached HEAD at exactly
# origin/main. Operator's working checkout (task-amd-main-ui or any
# bd-NN/* branch) is NEVER mutated by this script. node_modules + cargo
# target/ live INSIDE the disposable worktree, cached across runs,
# wiped on lockfile or HEAD change.
#
# v1 → v2 deltas:
#   - REMOVED: rebase phase (operator checkout is untouched)
#   - REMOVED: .beads/issues.jsonl stash phase (no rebase = no jsonl conflict)
#   - REMOVED: untracked-vs-tracked collision check (operator checkout untouched)
#   - REMOVED: EXIT trap for stash recovery (no stash anymore)
#   - ADDED:   ensure_disposable_worktree (create or fast-forward)
#   - ADDED:   sync_disposable_worktree (update its detached HEAD to origin/main)
#
# Inherited from v1 (still correct):
#   - Branch classification (warn on merged-dead/orphan; v2 hooks in
#     tuxlink-21j8 lift to refuse at commit/push time)
#   - Audit log at dev/scratch/converge-build.log (json-lines, jq-friendly)
#   - lockfile-change-detect for node_modules wipe
#   - HEAD-change-detect for Cargo workspace target/ wipe
#   - --fresh / --dry-run / --skip-launch flags
#   - Process kill + port 1420 verification
#
# Usage:
#   scripts/converge-build.sh [--fresh] [--dry-run] [--skip-launch] [--help]
#   pnpm dev:converged    # package.json wrapper — the documented default path
#
# Exit codes:
#   0   converged + launched (or converged-only if --skip-launch)
#   4   git worktree create/fetch/sync failed, OR disposable worktree
#       is in a state the script won't auto-fix (dirty content, or
#       orphan directory at the disposable path that isn't a worktree)
#   6   pnpm install failed
#   7   port 1420 owner refused to release (manual --force-kill-owned needed)
#   9   another converge-build is in flight in this operator checkout
#       (per-checkout flock contention)
#   10  script-internal error (bug)

set -euo pipefail

# ─── Constants ─────────────────────────────────────────────────────────────

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
readonly AUDIT_LOG="${REPO_ROOT}/dev/scratch/converge-build.log"
readonly STATE_FILE="${REPO_ROOT}/dev/scratch/converge-build-state.json"
# The disposable worktree lives INSIDE the operator's main checkout but
# under .local/ which is gitignored. Persistent across runs.
readonly DISPOSABLE_WT_DIR="${REPO_ROOT}/.local/converge-build-worktree"

# ─── Flags ─────────────────────────────────────────────────────────────────

FLAG_FRESH=0
FLAG_DRY_RUN=0
FLAG_SKIP_LAUNCH=0
FLAG_FORCE_KILL_OWNED=0

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

# ─── Audit log ────────────────────────────────────────────────────────────

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
converge-build.sh v2 — disposable-worktree-at-origin/main consolidated build.

USAGE:
  scripts/converge-build.sh [OPTIONS]
  pnpm dev:converged                 # wrapper, the documented default path

WHAT IT DOES:
  Maintains a managed throwaway worktree at .local/converge-build-worktree/
  pinned at exactly origin/main (detached HEAD). Builds + launches tauri
  dev from THAT worktree, so the runtime build is unambiguous:
  no overlay from your handoff branch, no risk of building stale code.

OPTIONS:
  --fresh                  Wipe disposable worktree's node_modules +
                           target/ before install. Default: wipe
                           on lockfile change OR HEAD change since the
                           previous run.
  --skip-launch            Converge (sync + install + build deps) but do
                           not launch \`tauri dev\`.
  --dry-run                Print the plan without mutating anything.
  --force-kill-owned       Allow killing tauri/vite/tuxlink processes
                           that may be owned by another worktree.
                           Reserved for tuxlink-8d7y lease integration.
  -h, --help               This message.

KNOWN FAILURE MODES THIS HANDLES:
  1. Operator's handoff branch is N commits behind origin/main
     → BUILD IS NOT FROM OPERATOR'S BRANCH; always origin/main.
     Operator branch is read-only to this script.
  2. pnpm install reports "up to date" but symlinks stale
     → lockfile diff inside the disposable worktree drives the wipe.
  3. .beads/issues.jsonl staged by bd
     → no rebase of operator branch happens; staged file is irrelevant.
  4. Untracked vs tracked path collision in operator's checkout
     → disposable worktree is freshly-checked-out from origin/main;
     no untracked overlay possible.
  5. Stale target/debug/tuxlink binary
     → wipe when disposable worktree's HEAD changes (effectively
     whenever origin/main moves forward).
  6. Parallel \`tauri dev\` on port 1420 (strictPort)
     → kill before launch + verify port is free via ss / lsof.
  7. Bare-branch warning (orphan-post-merge)
     → still warns on operator's current branch; v2 hooks in
     tuxlink-21j8 lift to refuse at commit/push time.

NOT HANDLED IN v2 (see sub-issues / follow-up PRs):
  - Host-level dev-server lease for safe parallel work     → tuxlink-8d7y
  - CI scheduled audit catching non-Claude bypasses        → tuxlink-ui3i
  - Test fixtures for all 7 modes                          → tuxlink-8zho
  - Refuse commits on merged-dead branches                 → tuxlink-21j8

REQUIREMENTS: Linux (bash 4+, ss or lsof, sha256sum, pgrep, pkill, jq, git, pnpm).

AUDIT LOG:        ${AUDIT_LOG}
STATE FILE:       ${STATE_FILE}
DISPOSABLE WT:    ${DISPOSABLE_WT_DIR}
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

# ─── Phase: branch classification (informational only in v2) ──────────────

# v2 still classifies the operator's branch + warns on merged-dead/orphan,
# but the warning is purely informational: the disposable worktree is the
# build target, so the operator's branch state cannot affect the runtime
# build. Branch state machine (tuxlink-21j8) lifts the warn to a refuse
# at commit/push time via .githooks.
classify_operator_branch() {
  local branch; branch="$(git -C "${REPO_ROOT}" symbolic-ref --short HEAD 2>/dev/null || echo DETACHED)"
  local sha;    sha="$(git -C "${REPO_ROOT}" rev-parse HEAD)"
  local merged_pr_state="none"
  local open_pr_state="none"

  if command -v gh >/dev/null 2>&1; then
    if gh pr view "${branch}" --json state,number 2>/dev/null | grep -q '"state":"OPEN"'; then
      open_pr_state="$(gh pr view "${branch}" --json number 2>/dev/null | sed -n 's/.*"number":\([0-9]*\).*/\1/p')"
    fi
    if gh pr list --head "${branch}" --state merged --json number,mergedAt --limit 1 2>/dev/null | grep -q '"mergedAt"'; then
      merged_pr_state="$(gh pr list --head "${branch}" --state merged --json number --limit 1 2>/dev/null | sed -n 's/.*"number":\([0-9]*\).*/\1/p')"
    fi
  fi

  local category="unknown"
  case "${branch}" in
    main)                                     category="protected-main" ;;
    task-amd-main-ui|task-*)                  category="operator-local" ;;
    bd-*)
      if [[ "${open_pr_state}" != "none" ]]; then
        category="bd-issue-pr-open"
      elif [[ "${merged_pr_state}" != "none" ]]; then
        category="bd-issue-merged-dead"
      else
        category="bd-issue-active"
      fi
      ;;
    agent-*)                                  category="agent-throwaway" ;;
    release-please--*|dependabot/*)           category="bot" ;;
    *)
      if [[ "${open_pr_state}" != "none" ]]; then
        category="adhoc-pr-open"
      elif [[ "${merged_pr_state}" != "none" ]]; then
        category="adhoc-merged-dead"
      else
        category="adhoc-unowned"
      fi
      ;;
  esac

  printf '{"branch":"%s","sha":"%s","category":"%s"}\n' \
    "${branch}" "${sha}" "${category}"
}

# ─── Phase: fetch + sync the disposable worktree ──────────────────────────

fetch_prune() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: git fetch origin --prune"
    return 0
  fi
  step "git fetch origin --prune (in operator checkout)"
  git -C "${REPO_ROOT}" fetch origin --prune >&2
}

# Ensure .local/converge-build-worktree exists at HEAD=origin/main (detached).
# First run: create via `git worktree add --detach`.
# Subsequent runs: `git checkout --detach origin/main` from inside it (this
# is safe — detached HEAD updates do not affect any branch).
# Helper: is the disposable worktree dirty? Tracks staged + tracked changes
# plus non-cache untracked files. Ignores known generated build caches even if
# the disposable checkout predates the .gitignore entry for the root Cargo
# workspace target/. Returns 0 if clean, 1 if dirty.
_disposable_untracked_source_paths() {
  git -C "${DISPOSABLE_WT_DIR}" ls-files --others --exclude-standard 2>/dev/null \
    | while IFS= read -r path; do
        case "${path}" in
          target|target/*|node_modules|node_modules/*|src-tauri/target|src-tauri/target/*|src-tauri/gen|src-tauri/gen/*)
            ;;
          *)
            printf '%s\n' "${path}"
            ;;
        esac
      done
}

_disposable_is_clean() {
  [[ -d "${DISPOSABLE_WT_DIR}" ]] || return 0
  if ! git -C "${DISPOSABLE_WT_DIR}" diff --quiet 2>/dev/null; then return 1; fi
  if ! git -C "${DISPOSABLE_WT_DIR}" diff --quiet --cached 2>/dev/null; then return 1; fi
  # Untracked NON-cache files. Keep this path-level allowlist narrow: source
  # dirt remains a hard refusal, but root target/ from the workspace build no
  # longer blocks the next sync.
  local untracked
  untracked="$(_disposable_untracked_source_paths || true)"
  [[ -z "${untracked}" ]]
}

ensure_disposable_worktree() {
  local dry="$1"
  local origin_main_sha; origin_main_sha="$(git -C "${REPO_ROOT}" rev-parse origin/main)"

  # Codex P2 #2 (2026-06-01): defer .local/ creation to the non-dry-run
  # path so --dry-run honors its no-mutation contract.
  if [[ "${dry}" -eq 0 ]]; then
    mkdir -p "$(dirname "${DISPOSABLE_WT_DIR}")"
  fi

  # Codex P1 #2 (2026-06-01): proactively prune git's worktree registry
  # before the registration check. This handles the case where the
  # worktree was registered but the directory was deleted out-of-band
  # (manual rm -rf, disk failure, OS clean-up). Without prune, the
  # registration check sees the path as 'registered', then the subsequent
  # `git -C $DIR` calls fail because the directory is missing.
  if [[ "${dry}" -eq 0 ]]; then
    git -C "${REPO_ROOT}" worktree prune >&2 2>/dev/null || true
  fi

  # Is the worktree currently registered (post-prune) and directory present?
  local registered=0
  local dir_exists=0
  [[ -d "${DISPOSABLE_WT_DIR}" ]] && dir_exists=1
  if git -C "${REPO_ROOT}" worktree list --porcelain 2>/dev/null \
      | awk -v p="${DISPOSABLE_WT_DIR}" '$1=="worktree" && $2==p {found=1} END {exit !found}'; then
    registered=1
  fi

  # Codex P1 #2 continued: handle the un-registered-but-present-on-disk
  # case — refuse with a clear remediation rather than letting
  # `git worktree add` fail with an opaque message.
  if [[ "${registered}" -eq 0 && "${dir_exists}" -eq 1 ]]; then
    warn "directory ${DISPOSABLE_WT_DIR} exists but is NOT a registered worktree."
    warn "this likely means it was a worktree that was deregistered (e.g. via"
    warn "git worktree prune after manual delete) but the dir was not removed."
    warn "If safe, remove it: rm -rf ${DISPOSABLE_WT_DIR}"
    warn "then re-run scripts/converge-build.sh."
    audit "disposable_orphan_dir" "$(printf '{"path":"%s"}' "${DISPOSABLE_WT_DIR}")"
    exit 4
  fi

  if [[ "${registered}" -eq 0 ]]; then
    if [[ "${dry}" -eq 1 ]]; then
      dim "  [dry-run] would: git worktree add --detach ${DISPOSABLE_WT_DIR} ${origin_main_sha:0:12}"
    else
      step "creating disposable worktree at ${DISPOSABLE_WT_DIR}"
      if ! git -C "${REPO_ROOT}" worktree add --detach "${DISPOSABLE_WT_DIR}" "${origin_main_sha}" >&2; then
        die "git worktree add failed — check git error above" 4
      fi
      ok "disposable worktree created at HEAD=${origin_main_sha:0:12}"
    fi
    return 0
  fi

  # Worktree exists. Codex P1 #1 (2026-06-01): check dirtiness BEFORE the
  # HEAD-matches early return. A dirty worktree that happens to be at the
  # right HEAD would otherwise silently contaminate the build.
  if ! _disposable_is_clean; then
    warn "disposable worktree has uncommitted/untracked source changes — refusing to use"
    warn "inspect with: git -C ${DISPOSABLE_WT_DIR} status"
    warn "agents should never write here; this likely indicates a misbehaving process"
    warn "or a manual edit. If intentional, commit + push elsewhere; if accidental,"
    warn "manually clean the specific source paths. Build caches (node_modules/,"
    warn "target/, src-tauri/target/, src-tauri/gen/) are ignored by this guard."
    audit "disposable_dirty" "$(printf '{"path":"%s"}' "${DISPOSABLE_WT_DIR}")"
    exit 4
  fi

  local wt_head=""
  wt_head="$(git -C "${DISPOSABLE_WT_DIR}" rev-parse HEAD 2>/dev/null || echo unknown)"

  if [[ "${wt_head}" = "${origin_main_sha}" ]]; then
    dim "disposable worktree already at origin/main (${origin_main_sha:0:12})"
    return 0
  fi

  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: cd ${DISPOSABLE_WT_DIR} && git checkout --detach ${origin_main_sha:0:12}"
    return 0
  fi

  step "syncing disposable worktree to origin/main (${wt_head:0:12} → ${origin_main_sha:0:12})"
  if ! git -C "${DISPOSABLE_WT_DIR}" checkout --detach "${origin_main_sha}" >&2; then
    die "git checkout --detach failed inside disposable worktree" 4
  fi
  ok "disposable worktree synced to ${origin_main_sha:0:12}"
}

# ─── Phase: build artifact freshness inside the disposable worktree ───────

maybe_wipe_build_artifacts() {
  local dry="$1"
  local lockfile="${DISPOSABLE_WT_DIR}/pnpm-lock.yaml"
  local cur_lockfile_sha=""
  [[ -f "${lockfile}" ]] && cur_lockfile_sha="$(sha256sum "${lockfile}" | cut -d' ' -f1)"
  local cur_head_sha=""
  if [[ -d "${DISPOSABLE_WT_DIR}" ]]; then
    cur_head_sha="$(git -C "${DISPOSABLE_WT_DIR}" rev-parse HEAD 2>/dev/null || echo unknown)"
  fi
  local prev_lockfile_sha=""
  local prev_head_sha=""
  if [[ -f "${STATE_FILE}" ]]; then
    prev_lockfile_sha="$(jq -r '.last_lockfile_sha // empty' "${STATE_FILE}" 2>/dev/null || true)"
    prev_head_sha="$(jq -r '.last_head_sha // empty' "${STATE_FILE}" 2>/dev/null || true)"
  fi

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
      dim "  [dry-run] would wipe ${DISPOSABLE_WT_DIR}/node_modules (${nm_reason})"
    else
      step "wiping ${DISPOSABLE_WT_DIR}/node_modules (${nm_reason})"
      rm -rf "${DISPOSABLE_WT_DIR}/node_modules"
    fi
  else
    dim "node_modules: keep (lockfile unchanged)"
  fi

  if [[ -n "${target_reason}" ]]; then
    if [[ "${dry}" -eq 1 ]]; then
      dim "  [dry-run] would wipe ${DISPOSABLE_WT_DIR}/target and src-tauri/target (${target_reason})"
    else
      step "wiping ${DISPOSABLE_WT_DIR}/target and src-tauri/target (${target_reason})"
      rm -rf "${DISPOSABLE_WT_DIR}/target" "${DISPOSABLE_WT_DIR}/src-tauri/target"
    fi
  else
    dim "target: keep (HEAD unchanged)"
  fi

  printf '{"node_modules":"%s","node_modules_reason":"%s","target":"%s","target_reason":"%s"}\n' \
    "$([[ -n "${nm_reason}" ]] && echo wiped || echo keep)" "${nm_reason}" \
    "$([[ -n "${target_reason}" ]] && echo wiped || echo keep)" "${target_reason}"
}

pnpm_install() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: cd ${DISPOSABLE_WT_DIR} && pnpm install --frozen-lockfile"
    return 0
  fi
  step "pnpm install --frozen-lockfile (in disposable worktree)"
  if ! (cd "${DISPOSABLE_WT_DIR}" && pnpm install --frozen-lockfile); then
    die "pnpm install failed inside ${DISPOSABLE_WT_DIR}" 6
  fi
  ok "pnpm install complete"
}

# ─── Phase: kill stale tauri/vite/tuxlink processes + verify port ─────────

kill_stale_dev_processes() {
  local dry="$1"
  local pids_killed=()
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

verify_port_free() {
  local dry="$1"
  if [[ "${dry}" -eq 1 ]]; then
    dim "  [dry-run] would: check port 1420 is free; exit 7 if held"
    return 0
  fi
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
    warn "v2's blanket pkill matched 'tauri dev|target/debug/tuxlink|node.*vite'"
    warn "but something else is holding the port. Free it manually + re-run."
    warn "(tuxlink-8d7y lease will resolve this case via inspect + --force-kill-owned)"
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
    dim "  [dry-run] would: cd ${DISPOSABLE_WT_DIR} && pnpm tauri dev"
    echo "would-launch"
    return 0
  fi
  step "launching: cd ${DISPOSABLE_WT_DIR} && pnpm tauri dev"
  ok "converged at disposable=$(git -C "${DISPOSABLE_WT_DIR}" rev-parse --short HEAD) (origin/main=$(git -C "${REPO_ROOT}" rev-parse --short origin/main))"
  ok "audit log: ${AUDIT_LOG}"
  audit "launching" "$(printf '{"disposable_head":"%s","origin_main":"%s"}' \
    "$(git -C "${DISPOSABLE_WT_DIR}" rev-parse HEAD)" \
    "$(git -C "${REPO_ROOT}" rev-parse origin/main)")"
  cd "${DISPOSABLE_WT_DIR}"
  exec pnpm tauri dev
}

# ─── State-file writer ────────────────────────────────────────────────────

write_state() {
  local dry="$1"
  [[ "${dry}" -eq 1 ]] && return 0
  mkdir -p "$(dirname "${STATE_FILE}")"
  local lockfile="${DISPOSABLE_WT_DIR}/pnpm-lock.yaml"
  local cur_lockfile_sha=""
  [[ -f "${lockfile}" ]] && cur_lockfile_sha="$(sha256sum "${lockfile}" | cut -d' ' -f1)"
  local cur_head_sha="unknown"
  if [[ -d "${DISPOSABLE_WT_DIR}" ]]; then
    cur_head_sha="$(git -C "${DISPOSABLE_WT_DIR}" rev-parse HEAD 2>/dev/null || echo unknown)"
  fi
  cat >"${STATE_FILE}" <<EOF
{
  "last_run_id": "${RUN_ID}",
  "last_lockfile_sha": "${cur_lockfile_sha}",
  "last_head_sha": "${cur_head_sha}",
  "last_origin_main_sha": "$(git -C "${REPO_ROOT}" rev-parse origin/main)",
  "disposable_worktree_path": "${DISPOSABLE_WT_DIR}"
}
EOF
}

# ─── Main ─────────────────────────────────────────────────────────────────

main() {
  parse_args "$@"

  step "tuxlink converge-build v2 (run ${RUN_ID})"
  step "build target: ${DISPOSABLE_WT_DIR} (detached @ origin/main)"
  if [[ "${FLAG_DRY_RUN}" -eq 1 ]]; then
    warn "DRY RUN — no mutations will be performed"
  fi

  # Codex P2 #1 (2026-06-01): hold a flock on .local/converge-build.lock
  # for the duration of any mutating run. Two simultaneous invocations
  # from the SAME operator checkout would otherwise race the
  # worktree-prune + add, rm -rf node_modules, pnpm install, and state
  # writes. The lock is per-operator-checkout (different checkouts have
  # different .local/ paths); cross-checkout coordination is the
  # dev-server-lease's job (tuxlink-8d7y / PR #206).
  if [[ "${FLAG_DRY_RUN}" -eq 0 ]] && command -v flock >/dev/null 2>&1; then
    mkdir -p "${REPO_ROOT}/.local"
    exec 199>>"${REPO_ROOT}/.local/converge-build.lock"
    if ! flock -n 199; then
      die "another converge-build is in flight in this checkout (${REPO_ROOT}); refusing" 9
    fi
  fi

  # 1. Fetch fresh origin refs.
  step "phase 1/6 — git fetch origin --prune"
  fetch_prune "${FLAG_DRY_RUN}"
  audit "fetched" "$(printf '{"origin_main":"%s"}' \
    "$(git -C "${REPO_ROOT}" rev-parse origin/main 2>/dev/null || echo unknown)")"

  # 2. Informational: classify operator's branch + warn on dead/orphan.
  step "phase 2/6 — operator branch classification (informational)"
  local branch_json; branch_json="$(classify_operator_branch)"
  echo "  ${branch_json}" >&2
  audit "operator_branch" "${branch_json}"
  case "${branch_json}" in
    *bd-issue-merged-dead*|*adhoc-merged-dead*)
      warn "operator branch has a MERGED PR — committing there is the orphan-post-merge mode."
      warn "tuxlink-21j8's .githooks/pre-commit refuses this; activate via bash scripts/install-githooks.sh"
      ;;
    *adhoc-unowned*)
      dim "operator branch is unowned (no bd-id prefix, no PR). build is not affected."
      ;;
  esac

  # 3. Ensure disposable worktree exists + is synced to origin/main.
  step "phase 3/6 — disposable worktree ensure + sync"
  ensure_disposable_worktree "${FLAG_DRY_RUN}"
  audit "disposable_synced" "$(printf '{"path":"%s","head":"%s"}' \
    "${DISPOSABLE_WT_DIR}" \
    "$(test -d "${DISPOSABLE_WT_DIR}" && git -C "${DISPOSABLE_WT_DIR}" rev-parse HEAD 2>/dev/null || echo unknown)")"

  # 4. Build artifact freshness + install.
  step "phase 4/6 — build artifact freshness"
  local wipe_json; wipe_json="$(maybe_wipe_build_artifacts "${FLAG_DRY_RUN}")"
  audit "install_decision" "${wipe_json}"
  pnpm_install "${FLAG_DRY_RUN}"

  # 5. Kill stale dev processes + verify port.
  step "phase 5/6 — kill stale tauri/vite/tuxlink processes + verify port 1420"
  local killed_json; killed_json="$(kill_stale_dev_processes "${FLAG_DRY_RUN}")"
  audit "processes_killed" "$(printf '{"pids":%s}' "${killed_json}")"
  verify_port_free "${FLAG_DRY_RUN}"

  # 6. Write state + launch.
  step "phase 6/6 — launch"
  write_state "${FLAG_DRY_RUN}"
  audit "convergence_complete_pre_launch" "$(printf '{"dry_run":%d}' "${FLAG_DRY_RUN}")"
  launch_tauri_dev "${FLAG_DRY_RUN}"
}

main "$@"
