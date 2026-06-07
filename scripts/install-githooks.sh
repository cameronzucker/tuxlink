#!/usr/bin/env bash
# install-githooks.sh — one-step activation of tuxlink's .githooks/ directory.
#
# Sets `core.hooksPath` to .githooks/ and verifies the commit-msg +
# pre-commit + pre-push hooks are executable. Idempotent; safe to re-run.
#
# Implements docs/adr/0017-branch-state-machine.md activation instructions.
#
# Run from the repo root (or any worktree):
#   bash scripts/install-githooks.sh

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOKS_DIR="${REPO_ROOT}/.githooks"

if [ ! -d "${HOOKS_DIR}" ]; then
  printf '✗ %s not found — are you in the tuxlink repo?\n' "${HOOKS_DIR}" >&2
  exit 1
fi

# Activate the hooks. Per-worktree config (worktrees inherit from main repo's
# core.hooksPath setting; nothing per-worktree needed).
current_hooksPath="$(git config --get core.hooksPath 2>/dev/null || true)"
if [ "${current_hooksPath}" = ".githooks" ]; then
  printf '✓ core.hooksPath already set to .githooks (no change)\n'
else
  git config core.hooksPath .githooks
  if [ -n "${current_hooksPath}" ]; then
    printf '✓ core.hooksPath updated: %s → .githooks\n' "${current_hooksPath}"
  else
    printf '✓ core.hooksPath set to .githooks\n'
  fi
fi

# Make sure the hook scripts are executable. git config the hookspath does
# not make non-executable scripts run.
for hook in commit-msg pre-commit pre-push; do
  hookpath="${HOOKS_DIR}/${hook}"
  if [ ! -f "${hookpath}" ]; then
    printf '✗ missing %s\n' "${hookpath}" >&2
    exit 1
  fi
  if [ ! -x "${hookpath}" ]; then
    chmod +x "${hookpath}"
    printf '✓ chmod +x %s\n' "${hookpath}"
  else
    printf '✓ executable: %s\n' "${hookpath}"
  fi
done

# Sanity check: do the hook scripts and classifier library parse cleanly?
for hook in commit-msg pre-commit pre-push; do
  if ! bash -n "${HOOKS_DIR}/${hook}" 2>&1; then
    printf '✗ %s has syntax errors\n' "${HOOKS_DIR}/${hook}" >&2
    exit 1
  fi
done
printf '✓ hook script syntax OK\n'

if ! bash -n "${HOOKS_DIR}/lib/branch-state.sh" 2>&1; then
  printf '✗ branch-state.sh has syntax errors\n' >&2
  exit 1
fi
printf '✓ branch-state.sh syntax OK\n'

# Print where we landed for the operator.
cat <<EOF

Branch lifecycle hooks active. Future commits + pushes go through:
  ${HOOKS_DIR}/commit-msg
  ${HOOKS_DIR}/pre-commit
  ${HOOKS_DIR}/pre-push

Branch lifecycle override (documented escape hatch, loud + audited):
  TUXLINK_BRANCH_LIFECYCLE_OVERRIDE=I-know-what-Im-doing git commit ...

Agent trailer override (documented escape hatch, loud + audited):
  TUXLINK_AGENT_TRAILER_OVERRIDE=I-know-what-Im-doing git commit ...

Audit log: dev/scratch/branch-lifecycle-overrides.log
ADR:       docs/adr/0017-branch-state-machine.md
EOF
