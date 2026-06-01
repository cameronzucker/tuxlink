# dev-server-lease.sh — host-level lease for the tuxlink tauri dev server.
#
# Sourced by scripts/dev-server-lease.sh (the CLI wrapper) and — after
# tuxlink-qepd PR #203 merges — by scripts/converge-build.sh (replacing
# its blanket `pkill -f "tauri dev|target/debug/tuxlink|node.*vite"` with
# lease-aware ownership checks). Addresses Codex 2026-06-01 P1 #6:
#
#   "Killing every `tauri dev` / Vite / tuxlink PID is unsafe in parallel
#    work. This disrupts active agent work and may kill the wrong
#    worktree's process. Add a host-level build/dev-server lease. Show
#    the owning PID, cwd, branch, and SHA for port 1420; terminate only
#    the lease holder after confirmation or via a documented
#    --force-kill-owned path."
#
# The lease lives at ~/.config/tuxlink/dev-server.json (XDG_CONFIG_HOME if
# set) — a JSON object recording the process that claimed port 1420:
#
#   {
#     "pid":          <int>,
#     "cwd":          "/.../worktrees/bd-...",
#     "branch":       "bd-tuxlink-...",
#     "sha":          "<40-hex git SHA at acquire time>",
#     "started_at":   "<ISO-8601 UTC>",
#     "version":      1
#   }
#
# Functions exported (callers compare exit codes and use the global
# DS_LEASE_* output vars rather than parsing stdout):
#
#   ds_lease_acquire <branch> <sha>
#     Atomically write the lease file claiming ownership for this PID.
#     If a live owner already exists, exit 7 ("port owned") and populate
#     DS_LEASE_OWNER_* without overwriting. If the existing lease is
#     stale (its PID is dead OR its cwd is gone), overwrite + warn.
#
#   ds_lease_release
#     Delete the lease file IFF this PID is the recorded owner. No-op
#     otherwise. Called from converge-build's trap on tauri-dev exit
#     (post-#203 integration).
#
#   ds_lease_inspect
#     Read the lease file + cross-check port 1420. Populates DS_LEASE_*.
#     Echoes a JSON object summarizing both the lease and the port; exit
#     codes: 0=consistent (lease+port agree), 1=no-lease+no-port-owner,
#     2=stale-lease (file present but PID dead), 3=port-orphan (port held
#     by a PID not in the lease), 4=split-brain (lease + port disagree).
#
#   ds_lease_clear_stale
#     If the lease file points to a dead PID or a missing cwd, delete
#     it. Returns 0 if cleared, 1 if there was no lease, 2 if the lease
#     was live (NOT cleared).

# ─── Globals populated by ds_lease_* functions ────────────────────────────

DS_LEASE_FILE=""
DS_LEASE_OWNER_PID=""
DS_LEASE_OWNER_CWD=""
DS_LEASE_OWNER_BRANCH=""
DS_LEASE_OWNER_SHA=""
DS_LEASE_OWNER_STARTED_AT=""
DS_LEASE_PORT_PID=""

# ─── Configuration ────────────────────────────────────────────────────────

# XDG_CONFIG_HOME honored; fall back to ~/.config.
ds_lease_file() {
  local cfg_root="${XDG_CONFIG_HOME:-${HOME}/.config}"
  printf '%s/tuxlink/dev-server.json' "${cfg_root}"
}

# Port the tauri dev server binds (strictPort).
DS_LEASE_PORT="1420"

# ─── Helpers ──────────────────────────────────────────────────────────────

_ds_lease_now() {
  date -u +%Y-%m-%dT%H:%M:%SZ
}

# Is PID alive? Uses kill -0 which sends no signal, only checks existence.
# Returns 0 if alive, 1 if not.
_ds_lease_pid_alive() {
  local pid="$1"
  [ -n "${pid}" ] || return 1
  kill -0 "${pid}" 2>/dev/null
}

# Is the cwd still a valid directory? (operator may have disposed the
# worktree without releasing the lease — stale).
_ds_lease_cwd_exists() {
  local cwd="$1"
  [ -n "${cwd}" ] && [ -d "${cwd}" ]
}

# Read the lease file into DS_LEASE_OWNER_* globals.
# Returns 0 if file exists + parses; 1 if missing or unparseable.
# Codex P1 #2 (2026-06-01): symmetric jq-with-python3-fallback so that the
# read path doesn't silently misclassify "have lease but no jq" as "no lease"
# (which would let acquire overwrite a live lease).
_ds_lease_read() {
  local f; f="$(ds_lease_file)"
  DS_LEASE_FILE="${f}"
  DS_LEASE_OWNER_PID=""
  DS_LEASE_OWNER_CWD=""
  DS_LEASE_OWNER_BRANCH=""
  DS_LEASE_OWNER_SHA=""
  DS_LEASE_OWNER_STARTED_AT=""
  [ -f "${f}" ] || return 1
  if command -v jq >/dev/null 2>&1; then
    DS_LEASE_OWNER_PID="$(jq -r '.pid // empty' "${f}" 2>/dev/null || true)"
    DS_LEASE_OWNER_CWD="$(jq -r '.cwd // empty' "${f}" 2>/dev/null || true)"
    DS_LEASE_OWNER_BRANCH="$(jq -r '.branch // empty' "${f}" 2>/dev/null || true)"
    DS_LEASE_OWNER_SHA="$(jq -r '.sha // empty' "${f}" 2>/dev/null || true)"
    DS_LEASE_OWNER_STARTED_AT="$(jq -r '.started_at // empty' "${f}" 2>/dev/null || true)"
  elif command -v python3 >/dev/null 2>&1; then
    # Python fallback. Single subshell does the JSON parse + emits
    # tab-separated fields so we round-trip via read -d ''.
    local fields
    fields="$(python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as fp:
        d = json.load(fp)
except Exception:
    sys.exit(1)
out = [str(d.get(k, "")) for k in ("pid", "cwd", "branch", "sha", "started_at")]
print("\t".join(out))
' "${f}" 2>/dev/null || true)"
    if [ -n "${fields}" ]; then
      IFS=$'\t' read -r DS_LEASE_OWNER_PID DS_LEASE_OWNER_CWD DS_LEASE_OWNER_BRANCH \
        DS_LEASE_OWNER_SHA DS_LEASE_OWNER_STARTED_AT <<<"${fields}"
    fi
  else
    printf '✗ dev-server-lease: neither jq nor python3 available — refusing to read lease at %s\n' "${f}" >&2
    return 2
  fi
  [ -n "${DS_LEASE_OWNER_PID}" ]
}

# Atomic write: write to a temp file in the same directory, then rename.
# Atomic on POSIX (rename within same filesystem).
_ds_lease_write() {
  local pid="$1" cwd="$2" branch="$3" sha="$4" started_at="$5"
  local f; f="$(ds_lease_file)"
  mkdir -p "$(dirname "${f}")"
  local tmp
  tmp="$(mktemp "${f}.XXXXXX")"
  if command -v jq >/dev/null 2>&1; then
    jq -n \
      --argjson pid "${pid}" \
      --arg cwd "${cwd}" \
      --arg branch "${branch}" \
      --arg sha "${sha}" \
      --arg started_at "${started_at}" \
      '{pid: $pid, cwd: $cwd, branch: $branch, sha: $sha, started_at: $started_at, version: 1}' \
      >"${tmp}"
  else
    # Fallback minimal JSON (still valid for jq-less reads).
    printf '{"pid":%d,"cwd":%s,"branch":%s,"sha":%s,"started_at":%s,"version":1}\n' \
      "${pid}" \
      "$(printf '%s' "${cwd}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
      "$(printf '%s' "${branch}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
      "$(printf '%s' "${sha}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
      "$(printf '%s' "${started_at}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
      >"${tmp}"
  fi
  mv -f "${tmp}" "${f}"
}

# Identify the PID holding port 1420, if any. Echoes PID or empty string.
# Codex P1 #3 (2026-06-01): ss being PRESENT is not the same as ss WORKING —
# unprivileged users on some kernels get "Cannot open netlink socket" and ss
# returns 1. Fall back to lsof on ANY ss failure, and return non-zero exit
# if neither tool could produce a definitive answer (so callers can treat
# "unknown" as "do not classify as free").
_ds_lease_port_pid() {
  local ss_rc=99 lsof_rc=99 result=""
  if command -v ss >/dev/null 2>&1; then
    # ss -lntp output line example:
    #   LISTEN 0 511 *:1420 *:* users:(("node",pid=12345,fd=23))
    local ss_out
    ss_out="$(ss -lntp 2>/dev/null)"
    ss_rc=$?
    if [ "${ss_rc}" -eq 0 ]; then
      result="$(printf '%s' "${ss_out}" | awk -v port=":${DS_LEASE_PORT}\$" '
        $4 ~ port {
          match($0, /pid=[0-9]+/)
          if (RLENGTH > 0) {
            p = substr($0, RSTART+4, RLENGTH-4)
            print p
            exit
          }
        }')"
      printf '%s' "${result}"
      return 0
    fi
  fi
  if command -v lsof >/dev/null 2>&1; then
    result="$(lsof -ti :"${DS_LEASE_PORT}" -sTCP:LISTEN 2>/dev/null | head -1)"
    lsof_rc=$?
    if [ "${lsof_rc}" -eq 0 ] || [ "${lsof_rc}" -eq 1 ]; then
      # lsof returns 1 when no match — also valid "no owner" answer.
      printf '%s' "${result}"
      return 0
    fi
  fi
  # Both tools missing or both failed — port status is genuinely unknown.
  return 1
}

# ─── Public API ──────────────────────────────────────────────────────────

# ds_lease_acquire <branch> <sha>
# Returns:
#   0 — lease acquired (or this PID was already the owner; idempotent)
#   7 — port held by a live owner; lease file populated DS_LEASE_OWNER_*
#   8 — could not obtain advisory lock (another acquire in flight)
#
# Codex P1 #1 (2026-06-01): acquire holds an advisory flock on a lockfile
# adjacent to the lease for the duration of the read-modify-write window,
# so two concurrent acquires cannot both pass the stale-check + both write.
# The lockfile is separate from the lease file so a held flock is robust
# to atomic-rename of the lease itself.
ds_lease_acquire() {
  local branch="$1" sha="$2"
  local pid=$$
  local cwd; cwd="$(pwd)"
  local started_at; started_at="$(_ds_lease_now)"
  local f; f="$(ds_lease_file)"
  local lockfile="${f}.lock"
  mkdir -p "$(dirname "${f}")"

  # Open the lockfile on FD 200 and obtain a non-blocking exclusive lock.
  # If flock is unavailable, proceed without the mutex (best-effort —
  # surface a clear warning so the operator knows we degraded).
  if command -v flock >/dev/null 2>&1; then
    exec 200>>"${lockfile}"
    if ! flock -n 200; then
      printf '⚠ dev-server-lease: another acquire is in flight (lockfile %s); refusing this attempt\n' \
        "${lockfile}" >&2
      exec 200>&-
      return 8
    fi
  else
    printf '⚠ dev-server-lease: flock not installed — concurrent acquire is unsafe\n' >&2
  fi

  # Trap to release the flock on every return path (success + every error).
  local rc=0
  if _ds_lease_read; then
    if [ "${DS_LEASE_OWNER_PID}" = "${pid}" ]; then
      _ds_lease_write "${pid}" "${cwd}" "${branch}" "${sha}" "${started_at}"
      rc=0
    elif _ds_lease_pid_alive "${DS_LEASE_OWNER_PID}" \
        && _ds_lease_cwd_exists "${DS_LEASE_OWNER_CWD}"; then
      rc=7  # live owner; do not overwrite
    else
      printf '⚠ dev-server-lease: clearing stale lease (PID %s dead or cwd %s missing)\n' \
        "${DS_LEASE_OWNER_PID}" "${DS_LEASE_OWNER_CWD}" >&2
      _ds_lease_write "${pid}" "${cwd}" "${branch}" "${sha}" "${started_at}"
      rc=0
    fi
  else
    _ds_lease_write "${pid}" "${cwd}" "${branch}" "${sha}" "${started_at}"
    rc=0
  fi

  if command -v flock >/dev/null 2>&1; then
    flock -u 200 2>/dev/null || true
    exec 200>&-
  fi
  return "${rc}"
}

# ds_lease_release
# Delete the lease IFF the current PID is the recorded owner.
# Returns: 0 (released or not-owner — both no-op terminal states).
ds_lease_release() {
  local pid=$$
  if _ds_lease_read; then
    if [ "${DS_LEASE_OWNER_PID}" = "${pid}" ]; then
      rm -f "${DS_LEASE_FILE}"
    fi
  fi
  return 0
}

# ds_lease_inspect
# Echoes a JSON summary; populates DS_LEASE_*.
# Exit codes:
#   0 — consistent (lease and port either both empty or both reference
#       the same live PID)
#   1 — empty (no lease, no port owner)
#   2 — stale (lease present, PID dead or cwd missing, port may or may
#       not be free)
#   3 — port-orphan (port held by a PID that does NOT match the lease,
#       and the lease is empty)
#   4 — split-brain (lease and port both populated but point at
#       different PIDs)
ds_lease_inspect() {
  local have_lease=0
  if _ds_lease_read; then
    have_lease=1
  fi
  DS_LEASE_PORT_PID="$(_ds_lease_port_pid 2>/dev/null || true)"

  # A lease is "live" only if BOTH the PID is alive AND the cwd still
  # exists — matches ds_lease_clear_stale's notion of stale (Codex P1 #4
  # 2026-06-01: inspect used to only check kill -0, inconsistent with the
  # documented "PID dead or cwd missing" semantics).
  local lease_pid_alive=0
  if [ "${have_lease}" -eq 1 ] \
      && _ds_lease_pid_alive "${DS_LEASE_OWNER_PID}" \
      && _ds_lease_cwd_exists "${DS_LEASE_OWNER_CWD}"; then
    lease_pid_alive=1
  fi

  # Emit JSON summary regardless of exit code.
  if command -v jq >/dev/null 2>&1; then
    jq -n \
      --arg lease_pid "${DS_LEASE_OWNER_PID:-}" \
      --arg lease_cwd "${DS_LEASE_OWNER_CWD:-}" \
      --arg lease_branch "${DS_LEASE_OWNER_BRANCH:-}" \
      --arg lease_sha "${DS_LEASE_OWNER_SHA:-}" \
      --arg lease_started_at "${DS_LEASE_OWNER_STARTED_AT:-}" \
      --arg port_pid "${DS_LEASE_PORT_PID:-}" \
      --arg port "${DS_LEASE_PORT}" \
      --argjson lease_pid_alive "${lease_pid_alive}" \
      '{lease: {pid: $lease_pid, cwd: $lease_cwd, branch: $lease_branch, sha: $lease_sha, started_at: $lease_started_at, pid_alive: ($lease_pid_alive == 1)}, port: {number: $port, owner_pid: $port_pid}}'
  fi

  if [ "${have_lease}" -eq 0 ] && [ -z "${DS_LEASE_PORT_PID}" ]; then
    return 1  # empty
  fi
  if [ "${have_lease}" -eq 1 ] && [ "${lease_pid_alive}" -eq 0 ]; then
    return 2  # stale lease
  fi
  if [ "${have_lease}" -eq 0 ] && [ -n "${DS_LEASE_PORT_PID}" ]; then
    return 3  # port orphan
  fi
  if [ "${have_lease}" -eq 1 ] && [ -n "${DS_LEASE_PORT_PID}" ] \
      && [ "${DS_LEASE_OWNER_PID}" != "${DS_LEASE_PORT_PID}" ]; then
    return 4  # split-brain
  fi
  return 0
}

# ds_lease_clear_stale
# Returns: 0 if cleared, 1 if no lease, 2 if lease was live (NOT cleared).
ds_lease_clear_stale() {
  if ! _ds_lease_read; then
    return 1
  fi
  if _ds_lease_pid_alive "${DS_LEASE_OWNER_PID}" \
      && _ds_lease_cwd_exists "${DS_LEASE_OWNER_CWD}"; then
    return 2
  fi
  rm -f "${DS_LEASE_FILE}"
  return 0
}
