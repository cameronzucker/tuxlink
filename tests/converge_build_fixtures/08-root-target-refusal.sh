#!/usr/bin/env bash
# Mode 8: a repo-root target/ directory must be refused, independent of the
# disposable worktree's currently-checked-out .gitignore.
#
# Scenario (Codex edvb.1 adrev 2026-06-09): _disposable_is_clean runs BEFORE
# `git checkout --detach origin/main` (ensure_disposable_worktree lines 330 vs
# 355), so it evaluates the PREVIOUS run's .gitignore. b68017a removed the
# repo-root Cargo workspace and restored the invariant "a repo-root target/ is
# unexpected dirt (a convergence regression), never a cache." But if the
# disposable worktree is parked at a commit whose .gitignore still ignores
# /target/ (e.g. the 35-min PR #428 window), `ls-files --others
# --exclude-standard` HIDES a stray root target/, the clean-check passes, the
# sync proceeds, and maybe_wipe_build_artifacts only removes src-tauri/target —
# so the stale root cache survives and the invariant is silently unenforced.
#
# Handler: _disposable_is_clean refuses on the mere presence of
# ${DISPOSABLE_WT_DIR}/target as a directory, regardless of ignore rules.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

tmp="$(mk_tmpdir mode8-)"
trap 'rm -rf "${tmp}"' EXIT

# Fake disposable worktree whose checked-out .gitignore ignores /target/
# (the lagging-ignore-rules condition that hides a root cache).
make_fake_repo "${tmp}"
printf '/target/\nnode_modules/\n' > "${tmp}/.gitignore"
git -C "${tmp}" add .gitignore
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "ignore /target/ (simulates PR #428-window checkout)"

# Sanity: with /target/ ignored, git itself reports the tree clean even with a
# stray root target/ present — this is exactly the blind spot being closed.
mkdir -p "${tmp}/target/debug"
echo "stale root cache" > "${tmp}/target/debug/tuxlink"
hidden="$(git -C "${tmp}" ls-files --others --exclude-standard)"
assert_eq "${hidden}" "" "root target/ is hidden by --exclude-standard (the blind spot)"

# Exercise the real _disposable_is_clean against this worktree.
run_clean_check() {
  bash -c '
    set -euo pipefail
    DISPOSABLE_WT_DIR="'"$1"'"
    eval "$(sed -n "/^_disposable_is_clean()/,/^}/p" "'"${REPO_ROOT}/scripts/converge-build.sh"'")"
    _disposable_is_clean
  '
}

# With a stray root target/ present, the guard MUST refuse (non-zero).
assert_exit 1 "root target/ present → _disposable_is_clean refuses" \
  run_clean_check "${tmp}"

# Control: remove the root target/, leave only ignored node_modules → clean (0).
rm -rf "${tmp}/target"
mkdir -p "${tmp}/node_modules/.cache"
echo "dep" > "${tmp}/node_modules/.cache/x"
assert_exit 0 "no root target/, only ignored caches → _disposable_is_clean passes" \
  run_clean_check "${tmp}"

# Control: real source dirt still refuses (regression guard on the existing path).
echo "fn main() {}" > "${tmp}/main.rs"
assert_exit 1 "untracked source file → _disposable_is_clean still refuses" \
  run_clean_check "${tmp}"

report_and_exit
