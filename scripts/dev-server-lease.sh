#!/usr/bin/env bash
# dev-server-lease.sh — CLI wrapper for the host-level dev-server lease.
#
# Sub-commands:
#   inspect          Show the current lease + port-1420 owner.
#   acquire          Claim the lease for this process (no tauri dev launch).
#                    Most useful from inside a script (e.g. converge-build).
#   release          Release the lease IFF this PID is the recorded owner.
#   clear-stale      Delete a stale lease (dead PID OR missing cwd). Refuses
#                    to clear a live lease unless --force.
#   force-kill-owned Terminate the live owner + clear the lease + free port
#                    1420 (per Codex 2026-06-01 P1 #6, this is the documented
#                    operator-explicit path).
#
# After tuxlink-qepd PR #203 merges, the converge-build.sh `kill_stale_dev_processes`
# function will be reworked to call `inspect` first, then `force-kill-owned`
# only when `--force-kill-owned` was passed to converge-build. That stitching
# lands in a follow-up PR.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/dev-server-lease.sh
. "${SCRIPT_DIR}/lib/dev-server-lease.sh"

if [ -t 1 ]; then
  C_RED=$'\033[31m'
  C_YELLOW=$'\033[33m'
  C_GREEN=$'\033[32m'
  C_BLUE=$'\033[34m'
  C_BOLD=$'\033[1m'
  C_DIM=$'\033[2m'
  C_RESET=$'\033[0m'
else
  C_RED='' C_YELLOW='' C_GREEN='' C_BLUE='' C_BOLD='' C_DIM='' C_RESET=''
fi

usage() {
  cat <<EOF
dev-server-lease.sh — manage the tuxlink tauri dev-server lease.

USAGE:
  scripts/dev-server-lease.sh <command> [args]

COMMANDS:
  inspect                       Show lease + port-1420 status as JSON; exit
                                code reports consistency (0=consistent,
                                1=empty, 2=stale, 3=port-orphan, 4=split-brain).
  acquire <branch> <sha>        Claim the lease for the current process.
                                Useful from scripts. Exits 7 if a live
                                owner already holds it.
  release                       Release the lease (no-op if not owner).
  clear-stale [--force]         Delete a stale lease. With --force, also
                                deletes a live lease (LOUD; use sparingly).
  force-kill-owned              Terminate the live lease holder, then
                                delete the lease + verify port 1420 is
                                free. The operator-explicit path for the
                                "I really do need this port" case.
  -h | --help                   This message.

LEASE FILE: $(ds_lease_file)
PORT:       ${DS_LEASE_PORT}

EOF
}

cmd_inspect() {
  # Codex P2 (2026-06-01): the rc=$? capture is correct, but the outer
  # script runs under `set -e`, which sees ds_lease_inspect's non-zero
  # return as an error and exits before the case statement runs. Disable
  # errexit for this function's body so the human-diagnostic branches
  # actually fire.
  local rc
  set +e
  ds_lease_inspect
  rc=$?
  set -e
  case "${rc}" in
    0) printf '%s✓ lease + port are consistent%s\n' "${C_GREEN}" "${C_RESET}" >&2 ;;
    1) printf '%s○ no lease + port-1420 is free%s\n' "${C_DIM}" "${C_RESET}" >&2 ;;
    2) printf '%s⚠ stale lease — PID %s is dead or cwd %s is missing; clear-stale to remove%s\n' \
         "${C_YELLOW}" "${DS_LEASE_OWNER_PID}" "${DS_LEASE_OWNER_CWD}" "${C_RESET}" >&2 ;;
    3) printf '%s⚠ port-orphan — port %s held by PID %s but no lease file; some non-tuxlink process is holding the port%s\n' \
         "${C_YELLOW}" "${DS_LEASE_PORT}" "${DS_LEASE_PORT_PID}" "${C_RESET}" >&2 ;;
    4) printf '%s⚠ split-brain — lease claims PID %s, port held by PID %s%s\n' \
         "${C_RED}" "${DS_LEASE_OWNER_PID}" "${DS_LEASE_PORT_PID}" "${C_RESET}" >&2 ;;
  esac
  return "${rc}"
}

cmd_acquire() {
  local branch="${1:-}" sha="${2:-}"
  if [ -z "${branch}" ] || [ -z "${sha}" ]; then
    printf '%s✗ acquire requires <branch> <sha> args%s\n' "${C_RED}" "${C_RESET}" >&2
    return 2
  fi
  if ds_lease_acquire "${branch}" "${sha}"; then
    printf '%s✓ lease acquired by PID %d (branch %s SHA %s)%s\n' \
      "${C_GREEN}" "$$" "${branch}" "${sha:0:12}" "${C_RESET}" >&2
    return 0
  fi
  printf '%s✗ port %s already held by live owner:%s\n' \
    "${C_RED}${C_BOLD}" "${DS_LEASE_PORT}" "${C_RESET}" >&2
  printf '    PID:        %s\n' "${DS_LEASE_OWNER_PID}" >&2
  printf '    cwd:        %s\n' "${DS_LEASE_OWNER_CWD}" >&2
  printf '    branch:     %s\n' "${DS_LEASE_OWNER_BRANCH}" >&2
  printf '    SHA:        %s\n' "${DS_LEASE_OWNER_SHA:0:12}" >&2
  printf '    started:    %s\n' "${DS_LEASE_OWNER_STARTED_AT}" >&2
  printf '\nTo terminate the owner + reclaim the port:\n' >&2
  printf '    scripts/dev-server-lease.sh force-kill-owned\n' >&2
  return 7
}

cmd_release() {
  ds_lease_release
  printf '%s✓ released (no-op if not owner)%s\n' "${C_DIM}" "${C_RESET}" >&2
}

cmd_clear_stale() {
  local force=0
  [ "${1:-}" = "--force" ] && force=1
  if ds_lease_clear_stale; then
    printf '%s✓ stale lease cleared%s\n' "${C_GREEN}" "${C_RESET}" >&2
    return 0
  fi
  local rc=$?
  case "${rc}" in
    1)
      printf '%s○ no lease to clear%s\n' "${C_DIM}" "${C_RESET}" >&2
      ;;
    2)
      if [ "${force}" -eq 1 ]; then
        printf '%s⚠ --force: clearing LIVE lease (PID %s, cwd %s)%s\n' \
          "${C_YELLOW}" "${DS_LEASE_OWNER_PID}" "${DS_LEASE_OWNER_CWD}" "${C_RESET}" >&2
        rm -f "${DS_LEASE_FILE}"
        return 0
      fi
      printf '%s✗ lease is live (PID %s alive, cwd %s exists); use --force to override%s\n' \
        "${C_RED}" "${DS_LEASE_OWNER_PID}" "${DS_LEASE_OWNER_CWD}" "${C_RESET}" >&2
      return 2
      ;;
  esac
}

cmd_force_kill_owned() {
  if ! _ds_lease_read; then
    printf '%s○ no lease — nothing to kill%s\n' "${C_DIM}" "${C_RESET}" >&2
    return 1
  fi
  local owner_pid="${DS_LEASE_OWNER_PID}"
  if ! _ds_lease_pid_alive "${owner_pid}"; then
    printf '%s○ lease PID %s already dead; clearing stale lease%s\n' \
      "${C_DIM}" "${owner_pid}" "${C_RESET}" >&2
    rm -f "${DS_LEASE_FILE}"
    return 0
  fi
  printf '%s⚠ TERMINATING lease owner: PID %s on %s (%s)%s\n' \
    "${C_YELLOW}${C_BOLD}" "${owner_pid}" "${DS_LEASE_OWNER_BRANCH}" \
    "${DS_LEASE_OWNER_CWD}" "${C_RESET}" >&2
  # SIGTERM first; if still alive after 2s, escalate to SIGKILL.
  kill -TERM "${owner_pid}" 2>/dev/null || true
  sleep 2
  if _ds_lease_pid_alive "${owner_pid}"; then
    printf '%s⚠ PID %s did not exit after SIGTERM; sending SIGKILL%s\n' \
      "${C_YELLOW}" "${owner_pid}" "${C_RESET}" >&2
    kill -KILL "${owner_pid}" 2>/dev/null || true
    sleep 1
  fi
  rm -f "${DS_LEASE_FILE}"
  # Verify port is now free.
  local port_pid; port_pid="$(_ds_lease_port_pid 2>/dev/null || true)"
  if [ -n "${port_pid}" ]; then
    printf '%s✗ port %s still held by PID %s after kill; manual intervention required%s\n' \
      "${C_RED}${C_BOLD}" "${DS_LEASE_PORT}" "${port_pid}" "${C_RESET}" >&2
    return 7
  fi
  printf '%s✓ lease cleared + port %s confirmed free%s\n' \
    "${C_GREEN}" "${DS_LEASE_PORT}" "${C_RESET}" >&2
}

main() {
  if [ $# -eq 0 ]; then
    usage
    exit 0
  fi
  local cmd="$1"; shift
  case "${cmd}" in
    inspect)            cmd_inspect ;;
    acquire)            cmd_acquire "$@" ;;
    release)            cmd_release ;;
    clear-stale)        cmd_clear_stale "$@" ;;
    force-kill-owned)   cmd_force_kill_owned ;;
    -h|--help|help)     usage ;;
    *)
      printf '%s✗ unknown command: %s%s\n' "${C_RED}" "${cmd}" "${C_RESET}" >&2
      usage
      exit 2
      ;;
  esac
}

main "$@"
