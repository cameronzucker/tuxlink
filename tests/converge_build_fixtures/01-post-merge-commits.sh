#!/usr/bin/env bash
# Mode 1: orphan post-merge commits on a dead branch.
#
# Handler: .githooks/pre-commit (ADR 0017, PR #204 merged on 2026-06-01).
# This fixture verifies the integration: with .githooks active, an
# attempt to commit on a merged-dead branch is refused.
#
# This duplicates one PR #204 hook smoke test by design — a regression
# in either the classifier or the hook installation must be caught here
# too, since the test bundle is the integration-level safety net.

. "$(dirname "${BASH_SOURCE[0]}")/_lib.sh"

tmp="$(mk_tmpdir mode1-)"
trap 'rm -rf "${tmp}"' EXIT

# Build a fake repo + symlink the project's .githooks into it.
make_fake_repo "${tmp}"
ln -s "${REPO_ROOT}/.githooks" "${tmp}/.githooks"
git -C "${tmp}" config core.hooksPath .githooks
git -C "${tmp}" checkout -q -b bd-tuxlink-mode1/dead

# Stub `gh` so classify_branch_state returns merged-dead.
gh_stub="${tmp}/bin"
mkdir -p "${gh_stub}"
cat >"${gh_stub}/gh" <<'EOF'
#!/usr/bin/env bash
# Args: pr list --head <branch> --state merged --limit 1 --json number,mergedAt
head=""
state=""
while [ $# -gt 0 ]; do
  case "$1" in
    --head) head="$2"; shift 2 ;;
    --state) state="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ "${head}" = "bd-tuxlink-mode1/dead" ] && [ "${state}" = "merged" ]; then
  echo '[{"number":99,"mergedAt":"2026-05-31T00:00:00Z"}]'
else
  echo '[]'
fi
EOF
chmod +x "${gh_stub}/gh"

# Attempt the commit. The hook should refuse.
export PATH="${gh_stub}:${PATH}"
echo "test" > "${tmp}/test.txt"
git -C "${tmp}" add test.txt
set +e
git -C "${tmp}" -c user.email=test@example.com -c user.name=Test \
  commit -q -m "should be refused" 2>"${tmp}/stderr.log"
rc=$?
set -e
assert_eq "${rc}" "1" "pre-commit hook refuses orphan-post-merge commit"
if grep -q 'merged-dead' "${tmp}/stderr.log"; then
  fixture_pass "hook's refuse message mentions 'merged-dead'"
else
  fixture_fail "hook's refuse message missing 'merged-dead' classification"
fi

report_and_exit
