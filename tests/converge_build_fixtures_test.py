#!/usr/bin/env python3
"""Runner for the 7 convergence-build failure-mode fixtures.

Each fixture lives at tests/converge_build_fixtures/NN-name.sh and is a
self-contained bash script that sets up a fake scenario, exercises the
relevant converge-build component (extracted via sed + eval), and
exits non-zero if the handler is broken.

This Python runner discovers the fixtures, executes each in a clean
subprocess (with REPO_ROOT pointing at the project root), and reports
pass/fail per fixture.

Run via:
  python3 -m unittest tests.converge_build_fixtures_test -v
"""

from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURE_DIR = REPO_ROOT / "tests" / "converge_build_fixtures"


def _all_fixtures() -> list[Path]:
    """Return the sorted list of NN-*.sh fixture files."""
    return sorted(p for p in FIXTURE_DIR.glob("*.sh") if not p.name.startswith("_"))


def _run_fixture(fixture: Path) -> subprocess.CompletedProcess:
    """Execute one fixture and capture stdout + stderr."""
    env = {**os.environ, "REPO_ROOT": str(REPO_ROOT)}
    return subprocess.run(
        ["bash", str(fixture)],
        capture_output=True,
        text=True,
        check=False,
        env=env,
        timeout=120,
    )


class ConvergeBuildFixturesTest(unittest.TestCase):
    """One test method per failure mode. Each delegates to the fixture script."""

    def _assert_fixture(self, fixture_name: str):
        path = FIXTURE_DIR / fixture_name
        self.assertTrue(path.is_file(), msg=f"fixture missing: {path}")
        self.assertTrue(
            os.access(path, os.X_OK), msg=f"fixture not executable: {path}"
        )
        result = _run_fixture(path)
        # Fixture prints PASS/FAIL lines + exits 0 only if every assert passed.
        self.assertEqual(
            result.returncode,
            0,
            msg=(
                f"fixture {fixture_name} failed (rc={result.returncode}):\n"
                f"--- stdout ---\n{result.stdout}\n"
                f"--- stderr ---\n{result.stderr}"
            ),
        )

    def test_mode_1_post_merge_commits(self):
        """Orphan post-merge commits on a dead branch → pre-commit hook refuses."""
        self._assert_fixture("01-post-merge-commits.sh")

    def test_mode_2_stale_operator_branch(self):
        """Operator branch N commits behind → fetch_prune + rebase_forward."""
        self._assert_fixture("02-stale-operator-branch.sh")

    def test_mode_3_pnpm_symlink_rot(self):
        """pnpm install up-to-date but symlinks stale → lockfile-change-detect wipes node_modules."""
        self._assert_fixture("03-pnpm-symlink-rot.sh")

    def test_mode_4_beads_jsonl_staged(self):
        """.beads/issues.jsonl staged blocks rebase → named stash + EXIT-trap recovery."""
        self._assert_fixture("04-beads-jsonl-staged.sh")

    def test_mode_5_untracked_collision(self):
        """Untracked-vs-tracked identical-content collision → SHA-compare + auto-remove."""
        self._assert_fixture("05-untracked-collision.sh")

    def test_mode_6_stale_cargo_target(self):
        """Stale src-tauri/target binary → HEAD-change-detect wipes target/."""
        self._assert_fixture("06-stale-cargo-target.sh")

    def test_mode_7_port_1420_collision(self):
        """Port 1420 held by parallel tauri dev → verify_port_free exits 7."""
        self._assert_fixture("07-port-1420-collision.sh")


class FixtureBundleSanityTest(unittest.TestCase):
    """Lightweight checks on the bundle structure itself."""

    def test_all_seven_fixtures_present(self):
        """The 7 catalogued modes each have one fixture file."""
        fixtures = _all_fixtures()
        self.assertEqual(
            len(fixtures), 7, msg=f"expected 7 fixtures, found {len(fixtures)}"
        )
        # Each should be numbered 01..07.
        names = [f.name for f in fixtures]
        for i in range(1, 8):
            prefix = f"{i:02d}-"
            self.assertTrue(
                any(n.startswith(prefix) for n in names),
                msg=f"no fixture with prefix {prefix}; have: {names}",
            )

    def test_all_fixtures_have_shebang_and_lib_source(self):
        """Each fixture must source _lib.sh (for assert_eq / fixture_pass)."""
        for fixture in _all_fixtures():
            content = fixture.read_text()
            self.assertTrue(
                content.startswith("#!/usr/bin/env bash"),
                msg=f"{fixture.name} missing shebang",
            )
            self.assertIn(
                "_lib.sh",
                content,
                msg=f"{fixture.name} does not source _lib.sh",
            )

    def test_readme_lists_all_seven_modes(self):
        """The README explains all 7 modes."""
        readme = (FIXTURE_DIR / "README.md").read_text()
        for i in range(1, 8):
            self.assertIn(
                f"`0{i}-",
                readme,
                msg=f"README missing reference to 0{i}- fixture",
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
