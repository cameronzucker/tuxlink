"""Tests for get_tuxlink_sessions.py.

Primarily regression coverage for the 2026-05-18 incident where the script
and the block-main-checkout-race hook resolved the lease directory to
different filesystem locations (script: .claude/session-leases/, hook:
<git-common-dir>/session-leases/), causing the script to silently
under-report live sessions and giving agents false grounds to argue with
the hook. See bd issue tuxlink-arv.

Run from any worktree's repo root:
  python3 -m unittest discover .claude/scripts/tests/
"""

import subprocess
import sys
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPT_DIR))

from get_tuxlink_sessions import resolve_lease_dir  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[3]


class LeaseDirResolution(unittest.TestCase):
    """The script MUST agree with the hook on where leases live, or it will
    silently under-report live sessions. The hook resolves via
    `git rev-parse --git-common-dir`/session-leases (see
    .claude/hooks/block-main-checkout-race.sh line 41). The script must
    match exactly."""

    def test_lease_dir_matches_git_common_dir(self):
        common_dir = subprocess.check_output(
            ["git", "rev-parse", "--git-common-dir"],
            cwd=REPO_ROOT, text=True,
        ).strip()
        cd = Path(common_dir)
        if not cd.is_absolute():
            cd = (REPO_ROOT / cd).resolve()
        expected = (cd / "session-leases").resolve()
        actual = resolve_lease_dir(REPO_ROOT).resolve()
        self.assertEqual(
            actual, expected,
            f"Script lease dir {actual} does not match hook lease dir "
            f"{expected}. This is the 2026-05-18 tuxlink-arv bug pattern.",
        )


import json
import os
import shutil
import tempfile
import uuid
from datetime import datetime, timezone
from io import StringIO
from unittest.mock import patch

import get_tuxlink_sessions  # noqa: E402


class MainEndToEnd(unittest.TestCase):
    """Mutation killer per Codex round 5 findings #1 + #2.

    Defeats two mutants the unit test in LeaseDirResolution misses:
    1. Helper added correctly but `main()` still uses the old hardcoded path —
       end-to-end output is wrong even though the unit test passes.
    2. Helper returns `repo/.git/session-leases/` (hardcoded) — coincidentally
       correct from main checkout, broken from linked worktree where `.git` is
       a FILE not a dir.
    """

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="tuxlink-arv-test-")
        self.main_repo = Path(self.tmp) / "main"
        self.main_repo.mkdir()
        self.wt_path = Path(self.tmp) / "wt"

        env = {
            **os.environ,
            "GIT_AUTHOR_NAME": "test", "GIT_AUTHOR_EMAIL": "t@t",
            "GIT_COMMITTER_NAME": "test", "GIT_COMMITTER_EMAIL": "t@t",
        }
        # -c core.hooksPath=/dev/null bypasses the project's commit-msg hook
        # (which requires Agent: trailers) — temp repo doesn't need it.
        subprocess.check_call(
            ["git", "-c", "core.hooksPath=/dev/null",
             "init", "-b", "main", str(self.main_repo)],
            stderr=subprocess.DEVNULL,
        )
        subprocess.check_call(
            ["git", "-c", "core.hooksPath=/dev/null",
             "-C", str(self.main_repo), "commit", "--allow-empty", "-m", "init"],
            stderr=subprocess.DEVNULL, env=env,
        )
        subprocess.check_call(
            ["git", "-c", "core.hooksPath=/dev/null",
             "-C", str(self.main_repo), "worktree", "add",
             str(self.wt_path), "-b", "wt-branch"],
            stderr=subprocess.DEVNULL,
        )

        # Write a unique lease into the git-common-dir/session-leases/.
        self.moniker = f"mutation-killer-{uuid.uuid4().hex[:8]}"
        lease_dir = self.main_repo / ".git" / "session-leases"
        lease_dir.mkdir()
        ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")
        (lease_dir / f"{uuid.uuid4().hex}.json").write_text(json.dumps({
            "sessionId": uuid.uuid4().hex,
            "moniker": self.moniker,
            "repo": str(self.main_repo),
            "cwd": str(self.main_repo),
            "branch": "main",
            "isMainCheckout": True,
            "lastSeenUtc": ts,
        }))

        # Per plan review R2 finding #2 + Codex impl-diff round (P3 finding 2026-05-18):
        # Drop a LEASE-SHAPED denied-attempts.jsonl with fresh ts + unique moniker so
        # main() WOULD print this moniker if the glob ever regresses from "*.json"
        # to "*.json*" (which would match .jsonl too). The earlier shape (a non-lease
        # JSON line with a "not_a_lease" key) passed the test vacuously even under the
        # mutated glob because table output never prints raw JSON keys — the assertion
        # was not actually a kill-shot. This shape is.
        self.jsonl_sentinel = f"jsonl-mutant-sentinel-{uuid.uuid4().hex[:8]}"
        (lease_dir / "denied-attempts.jsonl").write_text(json.dumps({
            "sessionId": uuid.uuid4().hex,
            "moniker": self.jsonl_sentinel,
            "repo": str(self.main_repo),
            "cwd": str(self.main_repo),
            "branch": "main",
            "isMainCheckout": False,
            "lastSeenUtc": ts,
        }) + "\n")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run_main_from(self, cwd: Path) -> str:
        """Run main() with CLAUDE_PROJECT_DIR=cwd, capture stdout."""
        assert cwd.is_dir(), f"test setup failed: cwd {cwd} is not a directory"
        captured = StringIO()
        with patch.dict(os.environ, {"CLAUDE_PROJECT_DIR": str(cwd)}):
            with patch("sys.stdout", captured):
                with patch("sys.argv", ["get_tuxlink_sessions.py"]):
                    get_tuxlink_sessions.main()
        return captured.getvalue()

    def test_main_lists_lease_from_main_checkout(self):
        output = self._run_main_from(self.main_repo)
        self.assertIn(
            self.moniker, output,
            f"Expected moniker {self.moniker} in main()'s output from main "
            f"checkout. Got:\n{output}",
        )

    def test_main_lists_lease_from_linked_worktree(self):
        output = self._run_main_from(self.wt_path)
        self.assertIn(
            self.moniker, output,
            f"Expected moniker {self.moniker} in main()'s output from "
            f"linked worktree (where .git is a file). Got:\n{output}",
        )

    def test_main_skips_jsonl_files_in_lease_dir(self):
        """A mutant glob like '*.json*' would pick up denied-attempts.jsonl.
        Per Codex P3 (impl-diff round 2026-05-18): the .jsonl sentinel is
        lease-shaped with a fresh timestamp + unique moniker, so a mutated
        glob would cause main() to PRINT the moniker (lease passes the
        liveness filter). assertNotIn on the unique moniker is then a
        kill-shot assertion. The earlier "non-lease-shaped jsonl + assert
        'not_a_lease' key absent" was vacuous because table output never
        prints raw JSON keys.
        """
        output = self._run_main_from(self.main_repo)
        # The .json lease's moniker appears (lease was processed correctly).
        self.assertIn(self.moniker, output)
        # The .jsonl's unique moniker does NOT appear (the file was skipped
        # by the `*.json` glob). Under a mutant `*.json*` glob, this would
        # appear in the table — the assertion fails, the mutant is caught.
        self.assertNotIn(self.jsonl_sentinel, output)
