# _lib.sh — shared helpers for the convergence-build failure-mode fixtures.
# Sourced by each NN-name.sh fixture; not directly executable.

set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)}"

# Extract a single function definition from converge-build.sh and source it
# in-place so the fixture can call it without running `main`. Args:
#   $1 = function name (e.g., "stash_bd_state")
extract_and_source_fn() {
  local fn="$1"
  local converge="${REPO_ROOT}/scripts/converge-build.sh"
  if [ ! -f "${converge}" ]; then
    printf '✗ %s not found\n' "${converge}" >&2
    return 1
  fi
  # sed -n '/^<fn>()/,/^}/p' extracts from the function signature to the
  # closing brace. converge-build.sh uses one-function-per-block style.
  local body
  body="$(sed -n "/^${fn}()/,/^}/p" "${converge}")"
  if [ -z "${body}" ]; then
    printf '✗ function %s not found in %s\n' "${fn}" "${converge}" >&2
    return 1
  fi
  eval "${body}"
}

# Build a minimal fake git repo at $1 with an initial commit. The repo has
# `main` as the default branch. Returns 0 on success.
make_fake_repo() {
  local dir="$1"
  mkdir -p "${dir}"
  git -C "${dir}" init -q -b main
  git -C "${dir}" -c user.email=test@example.com -c user.name=Test \
    commit -q --allow-empty -m "root"
  # Quiet down git's default config to avoid surprises.
  git -C "${dir}" config user.email test@example.com
  git -C "${dir}" config user.name "Test User"
}

# Print PASS/FAIL with an explanation; sets _FIXTURE_RC.
_FIXTURE_RC=0
fixture_pass() {
  printf '✓ PASS: %s\n' "$*"
}
fixture_fail() {
  printf '✗ FAIL: %s\n' "$*" >&2
  _FIXTURE_RC=1
}

# Assert two strings equal; pass/fail with description.
assert_eq() {
  local actual="$1" expected="$2" desc="$3"
  if [ "${actual}" = "${expected}" ]; then
    fixture_pass "${desc} (got: ${actual})"
  else
    fixture_fail "${desc}: expected '${expected}', got '${actual}'"
  fi
}

# Assert a command exits with a specific code.
assert_exit() {
  local expected="$1"; shift
  local desc="$1"; shift
  set +e
  "$@" >/dev/null 2>&1
  local rc=$?
  set -e
  if [ "${rc}" -eq "${expected}" ]; then
    fixture_pass "${desc} (exit ${rc})"
  else
    fixture_fail "${desc}: expected exit ${expected}, got ${rc}"
  fi
}

# Assert file exists / doesn't exist.
assert_file_exists() {
  if [ -f "$1" ]; then
    fixture_pass "file exists: $1"
  else
    fixture_fail "file MISSING: $1"
  fi
}
assert_file_absent() {
  if [ ! -f "$1" ]; then
    fixture_pass "file absent: $1"
  else
    fixture_fail "file UNEXPECTEDLY PRESENT: $1"
  fi
}

# Mktemp wrapper that respects $TMPDIR + cleans up via trap.
mk_tmpdir() {
  local prefix="${1:-tuxlink-fixture-}"
  local d
  d="$(mktemp -d -t "${prefix}XXXXXX")"
  printf '%s' "${d}"
}

# At the end of every fixture, exit with $_FIXTURE_RC.
report_and_exit() {
  exit "${_FIXTURE_RC}"
}
