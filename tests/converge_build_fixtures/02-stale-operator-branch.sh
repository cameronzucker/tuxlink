#!/usr/bin/env bash
# Mode 2: operator branch N commits behind origin/main.
#
# Handler (v1, currently on main): converge-build.sh's `fetch_prune` +
#   `rebase_forward` bring the operator branch forward to origin/main.
# Handler (v2, PR #207 pending): the operator branch is never mutated;
#   the build target is origin/main via a disposable worktree.
#
# This fixture verifies that the v1 fetch_prune + rebase_forward
# functions are wired correctly. PR #207 will change semantics — this
# fixture's expectations track v1 until then.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

# Verify converge-build.sh contains the expected handler functions.
# (Sourcing the entire script would run main; extract-function is the
# safe pattern for these isolated tests.)
converge="${REPO_ROOT}/scripts/converge-build.sh"
if ! grep -q '^fetch_prune()' "${converge}"; then
  fixture_fail "converge-build.sh missing fetch_prune()"
else
  fixture_pass "fetch_prune() defined in v1 converge-build.sh"
fi
if ! grep -q '^rebase_forward()' "${converge}"; then
  fixture_fail "converge-build.sh missing rebase_forward()"
else
  fixture_pass "rebase_forward() defined in v1 converge-build.sh"
fi

# Verify the rebase phase exits 4 on failure (operator-actionable code).
if grep -A 20 '^rebase_forward()' "${converge}" | grep -q 'exit 4'; then
  fixture_pass "rebase_forward exits 4 on rebase failure (operator-actionable)"
else
  fixture_fail "rebase_forward missing exit-4 path"
fi

# Verify the audit log records the rebase outcome.
if grep -A 30 '^rebase_forward()' "${converge}" | grep -q 'audit "rebased"\|audit "rebase_failed"'; then
  fixture_pass "rebase outcome is audited"
else
  fixture_fail "rebase outcome not in audit log"
fi

report_and_exit
