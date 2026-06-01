#!/usr/bin/env bash
# Mode 4: .beads/issues.jsonl staged blocks rebase.
#
# Handler (v1): converge-build.sh's `stash_bd_state` named-stashes the
# file before rebase, then `restore_bd_state` pops it after. Codex P1 #3
# fix (in PR #203) added an EXIT trap so a rebase failure surfaces the
# stash for recovery.
# Handler (v2, PR #207 pending): the rebase phase is gone, so this mode
# is structurally eliminated. Fixture tracks v1 until #207 merges.
#
# This fixture verifies the v1 stash + restore logic.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

converge="${REPO_ROOT}/scripts/converge-build.sh"

# Verify the v1 handler functions exist.
for fn in stash_bd_state restore_bd_state trap_on_exit; do
  if grep -q "^${fn}()" "${converge}"; then
    fixture_pass "${fn}() defined in converge-build.sh"
  else
    fixture_fail "${fn}() missing from converge-build.sh"
  fi
done

# Verify the stash label is RUN_ID-suffixed (Codex P3 #21 — named stash
# marker for concurrency-safety).
if grep -q 'STASH_LABEL="converge-build/.*${RUN_ID}.*bd-issues-jsonl"' "${converge}"; then
  fixture_pass "STASH_LABEL is RUN_ID-suffixed (concurrency-safe)"
else
  # Alt match: any RUN_ID reference in the STASH_LABEL line.
  if grep -A 0 'STASH_LABEL' "${converge}" | grep -q 'RUN_ID'; then
    fixture_pass "STASH_LABEL includes RUN_ID"
  else
    fixture_fail "STASH_LABEL does not include RUN_ID"
  fi
fi

# Verify stash_bd_state targets .beads/issues.jsonl specifically.
if grep -A 30 '^stash_bd_state()' "${converge}" | grep -q '\.beads/issues\.jsonl'; then
  fixture_pass "stash_bd_state targets .beads/issues.jsonl"
else
  fixture_fail "stash_bd_state doesn't target .beads/issues.jsonl"
fi

# Verify the EXIT trap is installed (the Codex P1 #3 fix).
if grep -q '^trap trap_on_exit EXIT' "${converge}"; then
  fixture_pass "EXIT trap installed for stash recovery"
else
  fixture_fail "EXIT trap NOT installed (Codex P1 #3 regression)"
fi

# Verify trap surfaces the recovery command on non-zero exit.
if grep -A 30 '^trap_on_exit()' "${converge}" | grep -q 'git stash list'; then
  fixture_pass "trap_on_exit surfaces recovery command (git stash list)"
else
  fixture_fail "trap_on_exit doesn't surface recovery command"
fi

report_and_exit
