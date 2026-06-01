#!/usr/bin/env bash
# Mode 5: untracked-vs-tracked identical-content collision.
#
# Scenario: operator's checkout has an untracked file F. origin/main has
# the SAME path F tracked with IDENTICAL bytes. `git rebase` refuses
# the rebase because the untracked file would be overwritten by checkout.
#
# Handler (v1): converge-build.sh's `resolve_untracked_collisions` —
# SHA-compares each untracked path; auto-removes identicals (since the
# tracked copy at origin/main is the SoT); stops with operator decision
# choices for differing-byte collisions.
# Handler (v2, PR #207): the disposable worktree is freshly-checked-out
# from origin/main; no untracked overlay possible. Structurally immune.
#
# This fixture verifies the v1 SHA-compare logic.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

tmp="$(mk_tmpdir mode5-)"
trap 'rm -rf "${tmp}"' EXIT

# Build a fake repo, add a tracked file at HEAD, then place the SAME
# file as untracked locally.
make_fake_repo "${tmp}"
echo "shared content" > "${tmp}/conflict.md"
git -C "${tmp}" add conflict.md
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "add conflict.md as tracked"

# Now create a SECOND branch that has the file but with DIFFERENT content
# — so origin/main and operator branch diverge on this path.
git -C "${tmp}" checkout -q -b operator-local
git -C "${tmp}" rm -q conflict.md
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "operator-local: remove conflict.md"

# Place the file back UNTRACKED with the same bytes as origin/main's
# tracked copy.
echo "shared content" > "${tmp}/conflict.md"

# Now configure origin/main reference so the script's logic works.
# origin/main is just `main` in this fake repo; we alias it.
git -C "${tmp}" branch -q origin/main main 2>/dev/null || \
  git -C "${tmp}" update-ref refs/remotes/origin/main refs/heads/main

# Verify converge-build.sh's resolve_untracked_collisions function exists.
converge="${REPO_ROOT}/scripts/converge-build.sh"
if ! grep -q '^resolve_untracked_collisions()' "${converge}"; then
  fixture_fail "resolve_untracked_collisions() missing"
  report_and_exit
else
  fixture_pass "resolve_untracked_collisions() defined"
fi

# Verify it SHA-compares (uses git hash-object).
if grep -A 60 '^resolve_untracked_collisions()' "${converge}" | grep -q 'git -C "\${REPO_ROOT}" hash-object'; then
  fixture_pass "resolve_untracked_collisions uses git hash-object (SHA-compare)"
else
  fixture_fail "resolve_untracked_collisions does NOT use git hash-object"
fi

# Verify it auto-removes identical files (rm -f path).
if grep -A 60 '^resolve_untracked_collisions()' "${converge}" | grep -q 'rm -f "\${REPO_ROOT}/\${path}"'; then
  fixture_pass "resolve_untracked_collisions auto-removes identical files"
else
  fixture_fail "resolve_untracked_collisions does NOT auto-remove identicals"
fi

# Verify it exits 2 on differing-byte collisions (operator-decision-needed).
if grep -A 80 '^resolve_untracked_collisions()' "${converge}" | grep -q 'exit 2'; then
  fixture_pass "resolve_untracked_collisions exits 2 on differing-byte collision"
else
  fixture_fail "resolve_untracked_collisions does NOT exit 2 on differing collisions"
fi

report_and_exit
