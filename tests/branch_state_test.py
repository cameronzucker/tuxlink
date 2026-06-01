#!/usr/bin/env python3
"""Tests for .githooks/lib/branch-state.sh — the branch-state classifier.

Strategy:
- Build a `gh` stub on PATH that returns canned JSON for `gh pr list` calls.
- Initialize a throwaway git repo so `git symbolic-ref --short HEAD` works.
- Source branch-state.sh in a bash subshell and call classify_branch_state.

The hooks themselves (pre-commit / pre-push) are smoke-tested in
tests/branch_state_hooks_test.py (run after the classifier tests pass).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
LIB = REPO_ROOT / ".githooks" / "lib" / "branch-state.sh"


class _GhStub:
    """Context manager that puts a fake `gh` on PATH returning canned JSON.

    The canned mapping is keyed by `(branch, state)` — the same keys
    classify_branch_state queries via `gh pr list --head <branch> --state <state>`.
    Returns "[]" (no matches) for any key not in the mapping.
    """

    def __init__(self, canned: dict[tuple[str, str], str]):
        self.canned = canned
        self.tmpdir = tempfile.mkdtemp(prefix="tuxlink-gh-stub-")
        self.orig_path = os.environ.get("PATH", "")

    def __enter__(self):
        gh_path = Path(self.tmpdir) / "gh"
        # The stub: parse argv, find --head and --state, look up in canned.
        # Build the script line-by-line to avoid textwrap.dedent + multi-line
        # canned-block indentation interactions.
        lines = [
            "#!/usr/bin/env bash",
            "# Test stub for gh — branches by (head, state) tuple",
            'head=""',
            'state=""',
            "while [[ $# -gt 0 ]]; do",
            '  case "$1" in',
            '    --head) head="$2"; shift 2 ;;',
            '    --state) state="$2"; shift 2 ;;',
            "    *) shift ;;",
            "  esac",
            "done",
            'case "${head}|${state}" in',
        ]
        for (branch, state), output in self.canned.items():
            lines.append(f'  "{branch}|{state}") echo {output!r} ;;')
        lines += [
            '  *) echo "[]" ;;',
            "esac",
        ]
        gh_path.write_text("\n".join(lines) + "\n")
        gh_path.chmod(0o755)
        os.environ["PATH"] = f"{self.tmpdir}:{self.orig_path}"
        return self

    def __exit__(self, *exc):
        os.environ["PATH"] = self.orig_path
        shutil.rmtree(self.tmpdir, ignore_errors=True)


def _classify(branch: str) -> str:
    """Call classify_branch_state <branch> in a bash subshell."""
    proc = subprocess.run(
        [
            "bash",
            "-c",
            f". {LIB} && classify_branch_state {branch!r}",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.stdout.strip()


class BranchStateClassifierTest(unittest.TestCase):
    """Cover every state-machine branch in classify_branch_state."""

    def test_protected_main(self):
        self.assertEqual(_classify("main"), "protected")

    def test_protected_master(self):
        self.assertEqual(_classify("master"), "protected")

    def test_protected_release_slash(self):
        self.assertEqual(_classify("release/v1.0.0"), "protected")

    def test_protected_production(self):
        self.assertEqual(_classify("production"), "protected")

    def test_bot_release_please(self):
        self.assertEqual(_classify("release-please--branches--main"), "bot")

    def test_bot_dependabot(self):
        self.assertEqual(_classify("dependabot/npm_and_yarn/lodash-4.17.21"), "bot")

    def test_bot_github_actions(self):
        self.assertEqual(_classify("github-actions/ci-update"), "bot")

    def test_bot_renovate(self):
        self.assertEqual(_classify("renovate/major-react"), "bot")

    def test_empty_branch_returns_unknown(self):
        self.assertEqual(_classify(""), "unknown")

    def test_head_literal_returns_unknown(self):
        self.assertEqual(_classify("HEAD"), "unknown")

    def test_merged_dead_via_gh(self):
        with _GhStub(
            {
                (
                    "bd-tuxlink-v1p/html-forms-execution",
                    "merged",
                ): '[{"number":200,"mergedAt":"2026-05-31T18:00:00Z"}]',
            }
        ):
            self.assertEqual(
                _classify("bd-tuxlink-v1p/html-forms-execution"), "merged-dead"
            )

    def test_closed_dead_via_gh(self):
        with _GhStub(
            {
                ("bd-tuxlink-xxxx/abandoned", "merged"): "[]",
                (
                    "bd-tuxlink-xxxx/abandoned",
                    "closed",
                ): '[{"number":99,"state":"CLOSED"}]',
            }
        ):
            self.assertEqual(
                _classify("bd-tuxlink-xxxx/abandoned"), "closed-dead"
            )

    def test_pr_open_via_gh(self):
        with _GhStub(
            {
                ("bd-tuxlink-qepd/converge-build", "merged"): "[]",
                ("bd-tuxlink-qepd/converge-build", "closed"): "[]",
                (
                    "bd-tuxlink-qepd/converge-build",
                    "open",
                ): '[{"number":203}]',
            }
        ):
            self.assertEqual(
                _classify("bd-tuxlink-qepd/converge-build"), "pr-open"
            )

    def test_active_when_no_pr(self):
        with _GhStub(
            {
                ("bd-tuxlink-new/just-started", "merged"): "[]",
                ("bd-tuxlink-new/just-started", "closed"): "[]",
                ("bd-tuxlink-new/just-started", "open"): "[]",
            }
        ):
            self.assertEqual(
                _classify("bd-tuxlink-new/just-started"), "active"
            )

    def test_merged_predicate_does_not_match_closed_without_merged_at(self):
        """A 'closed' entry that happens to mention 'mergedAt' (which it
        won't, but defensive) must not be misread as merged."""
        with _GhStub(
            {
                ("weird-edge", "merged"): "[]",
                ("weird-edge", "closed"): '[{"number":1,"state":"CLOSED"}]',
                ("weird-edge", "open"): "[]",
            }
        ):
            self.assertEqual(_classify("weird-edge"), "closed-dead")

    def test_gh_unavailable_returns_unknown(self):
        """If gh is removed from PATH, classifier returns 'unknown' (warn+allow)."""
        orig_path = os.environ.get("PATH", "")
        # Build a PATH that contains no gh.
        tmp = tempfile.mkdtemp(prefix="tuxlink-no-gh-")
        try:
            os.environ["PATH"] = tmp  # only /tmp/no-gh — no gh, no git etc.
            # We need git available though, so symlink it in.
            git_bin = shutil.which("git", path=orig_path) or "/usr/bin/git"
            os.symlink(git_bin, Path(tmp) / "git")
            bash_bin = shutil.which("bash", path=orig_path) or "/bin/bash"
            os.symlink(bash_bin, Path(tmp) / "bash")
            # Re-source classifier in this restricted env.
            proc = subprocess.run(
                [bash_bin, "-c", f". {LIB} && classify_branch_state 'anything'"],
                capture_output=True,
                text=True,
                check=False,
                env={**os.environ, "PATH": tmp},
            )
            self.assertEqual(proc.stdout.strip(), "unknown")
        finally:
            os.environ["PATH"] = orig_path
            shutil.rmtree(tmp, ignore_errors=True)


class HookSmokeTest(unittest.TestCase):
    """End-to-end smoke: invoke pre-commit / pre-push with stubbed gh."""

    def _invoke_hook(self, hook_name: str, branch: str, env_extra: dict | None = None) -> subprocess.CompletedProcess:
        """Invoke the hook from a temporary git worktree on the given branch."""
        # The hook needs to be in a real git repo. Use the existing REPO_ROOT
        # (the worktree we're in) but force the branch via env-var tricks is
        # not straightforward; instead, build a fresh tiny repo.
        tmp = tempfile.mkdtemp(prefix="tuxlink-hook-test-")
        try:
            subprocess.run(["git", "init", "-q", tmp], check=True)
            subprocess.run(
                ["git", "-C", tmp, "checkout", "-b", branch],
                check=True,
                capture_output=True,
            )
            # Make sure a HEAD exists so symbolic-ref --short works.
            (Path(tmp) / "README.md").write_text("test\n")
            subprocess.run(
                ["git", "-C", tmp, "add", "README.md"], check=True
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    tmp,
                    "-c",
                    "user.email=test@example.com",
                    "-c",
                    "user.name=Test",
                    "commit",
                    "-q",
                    "-m",
                    "seed",
                ],
                check=True,
                env={**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null"},
            )
            # Symlink the hooks dir in.
            (Path(tmp) / ".githooks").symlink_to(REPO_ROOT / ".githooks")
            subprocess.run(
                ["git", "-C", tmp, "config", "core.hooksPath", ".githooks"],
                check=True,
            )
            # Invoke the hook directly to test classification.
            hook_path = REPO_ROOT / ".githooks" / hook_name
            env = {**os.environ, **(env_extra or {})}
            stdin_input = ""
            if hook_name == "pre-push":
                # Mimic git's stdin protocol: <local-ref> <local-sha> <remote-ref> <remote-sha>
                sha = subprocess.run(
                    ["git", "-C", tmp, "rev-parse", "HEAD"],
                    check=True,
                    capture_output=True,
                    text=True,
                ).stdout.strip()
                stdin_input = (
                    f"refs/heads/{branch} {sha} refs/heads/{branch} {sha}\n"
                )
            return subprocess.run(
                [str(hook_path)],
                cwd=tmp,
                capture_output=True,
                text=True,
                env=env,
                input=stdin_input,
                check=False,
            )
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def test_pre_commit_allows_active_branch(self):
        with _GhStub(
            {
                ("bd-tuxlink-active/work", "merged"): "[]",
                ("bd-tuxlink-active/work", "closed"): "[]",
                ("bd-tuxlink-active/work", "open"): "[]",
            }
        ):
            result = self._invoke_hook("pre-commit", "bd-tuxlink-active/work")
        self.assertEqual(result.returncode, 0, msg=result.stderr)

    def test_pre_commit_refuses_merged_dead(self):
        with _GhStub(
            {
                (
                    "bd-tuxlink-v1p/html-forms",
                    "merged",
                ): '[{"number":200,"mergedAt":"2026-05-31T18:00:00Z"}]',
            }
        ):
            result = self._invoke_hook(
                "pre-commit", "bd-tuxlink-v1p/html-forms"
            )
        self.assertEqual(
            result.returncode,
            1,
            msg=f"expected refuse; stderr={result.stderr}",
        )
        self.assertIn("merged-dead", result.stderr)
        self.assertIn("commit REFUSED", result.stderr)

    def test_pre_commit_override_allows_merged_dead(self):
        with _GhStub(
            {
                (
                    "bd-tuxlink-v1p/html-forms",
                    "merged",
                ): '[{"number":200,"mergedAt":"2026-05-31T18:00:00Z"}]',
            }
        ):
            result = self._invoke_hook(
                "pre-commit",
                "bd-tuxlink-v1p/html-forms",
                env_extra={
                    "TUXLINK_BRANCH_LIFECYCLE_OVERRIDE": "I-know-what-Im-doing"
                },
            )
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertIn("OVERRIDDEN", result.stderr)

    def test_pre_push_refuses_merged_dead(self):
        with _GhStub(
            {
                (
                    "bd-tuxlink-v1p/html-forms",
                    "merged",
                ): '[{"number":200,"mergedAt":"2026-05-31T18:00:00Z"}]',
            }
        ):
            result = self._invoke_hook(
                "pre-push", "bd-tuxlink-v1p/html-forms"
            )
        self.assertEqual(
            result.returncode,
            1,
            msg=f"expected refuse; stderr={result.stderr}",
        )

    def test_pre_push_allows_pr_open(self):
        with _GhStub(
            {
                ("bd-tuxlink-qepd/work", "merged"): "[]",
                ("bd-tuxlink-qepd/work", "closed"): "[]",
                (
                    "bd-tuxlink-qepd/work",
                    "open",
                ): '[{"number":203}]',
            }
        ):
            result = self._invoke_hook("pre-push", "bd-tuxlink-qepd/work")
        self.assertEqual(result.returncode, 0, msg=result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
