#!/usr/bin/env bash
# Mode 3: pnpm install reports "up to date" but symlinks are stale.
#
# Handler: converge-build.sh's `maybe_wipe_build_artifacts` — wipes
# node_modules when pnpm-lock.yaml's SHA changes since the last run.
#
# This fixture verifies the lockfile-change-detect logic.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

tmp="$(mk_tmpdir mode3-)"
trap 'rm -rf "${tmp}"' EXIT

# Build a fake repo with a pnpm-lock.yaml + a state file claiming a
# DIFFERENT lockfile SHA. maybe_wipe_build_artifacts should compare
# the two and report that wipe is needed.
make_fake_repo "${tmp}"
echo "lockfile_v1" > "${tmp}/pnpm-lock.yaml"
git -C "${tmp}" add pnpm-lock.yaml
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "seed lockfile"

# State file says the LAST run saw a different lockfile.
mkdir -p "${tmp}/dev/scratch"
cat > "${tmp}/dev/scratch/converge-build-state.json" <<EOF
{
  "last_run_id": "previous",
  "last_lockfile_sha": "0000000000000000000000000000000000000000000000000000000000000000",
  "last_head_sha": "$(git -C "${tmp}" rev-parse HEAD)",
  "last_origin_main_sha": "$(git -C "${tmp}" rev-parse HEAD)"
}
EOF

# Extract maybe_wipe_build_artifacts from converge-build.sh and exercise
# it in dry-run mode (so it doesn't actually rm -rf anything but DOES
# emit the structured decision).
#
# The script reads REPO_ROOT-relative paths, so we set REPO_ROOT to tmp.
cd "${tmp}"
REPO_ROOT="${tmp}" \
LOCKFILE="${tmp}/pnpm-lock.yaml" \
STATE_FILE="${tmp}/dev/scratch/converge-build-state.json" \
FLAG_FRESH=0 \
FLAG_DRY_RUN=1 \
  bash -c '
    set -euo pipefail
    REPO_ROOT="'"${tmp}"'"
    AUDIT_LOG="${REPO_ROOT}/dev/scratch/converge-build.log"
    STATE_FILE="${REPO_ROOT}/dev/scratch/converge-build-state.json"
    LOCKFILE="${REPO_ROOT}/pnpm-lock.yaml"
    FLAG_FRESH=0
    FLAG_DRY_RUN=1
    # Define minimal ANSI helpers + audit shim that emits to stderr.
    step() { echo "▶ $*" >&2; }
    warn() { echo "⚠ $*" >&2; }
    ok() { echo "✓ $*" >&2; }
    die() { echo "✗ $*" >&2; exit "${2:-10}"; }
    dim() { echo "  $*" >&2; }
    audit() { :; }
    C_DIM=""; C_RESET=""
    # Source just the function we need.
    eval "$(sed -n "/^maybe_wipe_build_artifacts()/,/^}/p" '"${REPO_ROOT}/scripts/converge-build.sh"')"
    maybe_wipe_build_artifacts 1
  ' > "${tmp}/decision.json" 2> "${tmp}/stderr.log"

# Parse the decision JSON and check that node_modules: wiped.
decision="$(cat "${tmp}/decision.json")"
if echo "${decision}" | grep -q '"node_modules":"wiped"'; then
  fixture_pass "lockfile change detected → node_modules WOULD be wiped"
else
  fixture_fail "lockfile change NOT detected (got: ${decision})"
fi

# Verify the dry-run path emitted "would wipe" rather than actually wiping.
if grep -q "would wipe" "${tmp}/stderr.log"; then
  fixture_pass "dry-run printed 'would wipe' (no actual deletion)"
else
  fixture_fail "dry-run did not print 'would wipe' (stderr: $(cat "${tmp}/stderr.log"))"
fi

report_and_exit
