#!/usr/bin/env bash
# Mode 6: stale src-tauri/target/debug/tuxlink from old binary.
#
# Scenario: rebase moves HEAD forward, but cargo's incremental build sees
# only the .rs source changes — it doesn't know the BINARY itself was
# built against pre-rebase code. `tauri dev` then launches a binary
# whose .so/.dll deps may not match the current source.
#
# Handler: converge-build.sh's `maybe_wipe_build_artifacts` — wipes
# src-tauri/target when HEAD changes since the last run (the Codex P1
# #1 fix that landed in PR #203's fix-up commit).

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

tmp="$(mk_tmpdir mode6-)"
trap 'rm -rf "${tmp}"' EXIT

# Build a fake repo + state file claiming the LAST run was at a different HEAD.
make_fake_repo "${tmp}"
echo "lockfile" > "${tmp}/pnpm-lock.yaml"
git -C "${tmp}" add pnpm-lock.yaml
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "seed"
current_head="$(git -C "${tmp}" rev-parse HEAD)"
current_lockfile_sha="$(sha256sum "${tmp}/pnpm-lock.yaml" | cut -d' ' -f1)"

mkdir -p "${tmp}/dev/scratch"
cat > "${tmp}/dev/scratch/converge-build-state.json" <<EOF
{
  "last_run_id": "previous",
  "last_lockfile_sha": "${current_lockfile_sha}",
  "last_head_sha": "0000000000000000000000000000000000000000",
  "last_origin_main_sha": "${current_head}"
}
EOF

# Exercise maybe_wipe_build_artifacts in dry-run mode.
cd "${tmp}"
bash -c '
  set -euo pipefail
  REPO_ROOT="'"${tmp}"'"
  AUDIT_LOG="${REPO_ROOT}/dev/scratch/converge-build.log"
  STATE_FILE="${REPO_ROOT}/dev/scratch/converge-build-state.json"
  LOCKFILE="${REPO_ROOT}/pnpm-lock.yaml"
  FLAG_FRESH=0
  FLAG_DRY_RUN=1
  step() { :; }
  warn() { :; }
  ok() { :; }
  die() { exit "${2:-10}"; }
  dim() { :; }
  audit() { :; }
  C_DIM=""; C_RESET=""
  eval "$(sed -n "/^maybe_wipe_build_artifacts()/,/^}/p" '"${REPO_ROOT}/scripts/converge-build.sh"')"
  maybe_wipe_build_artifacts 1
' > "${tmp}/decision.json" 2> "${tmp}/stderr.log"

decision="$(cat "${tmp}/decision.json")"
# Lockfile UNCHANGED + HEAD CHANGED → node_modules keep + target wiped.
if echo "${decision}" | grep -q '"node_modules":"keep"'; then
  fixture_pass "lockfile unchanged → node_modules KEEP (no wasted wipe)"
else
  fixture_fail "lockfile-unchanged path failed (got: ${decision})"
fi
if echo "${decision}" | grep -q '"target":"wiped"'; then
  fixture_pass "HEAD changed → src-tauri/target WIPED (Codex P1 #1 fix verified)"
else
  fixture_fail "HEAD-change-detect failed (got: ${decision})"
fi

report_and_exit
