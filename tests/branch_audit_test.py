#!/usr/bin/env python3
"""Tests for scripts/branch-audit.sh — CI scheduled branch audit.

The audit script enumerates remote branches via `git for-each-ref`,
classifies each via `.githooks/lib/branch-state.sh`, and reports
orphan-post-merge branches (merged-dead + commits ahead of main).

Tests build a self-contained fake git repo with:
  - origin/main with a merged-tip commit
  - origin/bd-tuxlink-XYZ/merged-clean — branch matches merge tip (no orphan)
  - origin/bd-tuxlink-ABC/orphan-post-merge — branch has commits AFTER the
    merge tip (the v1p failure mode)
  - origin/bd-tuxlink-DEF/active — open-PR-equivalent, no merged PR

A `gh` stub on PATH returns canned JSON for `gh pr list --head <branch>`
calls so the classifier returns the right state without needing real
GitHub auth.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
AUDIT_SCRIPT = REPO_ROOT / "scripts" / "branch-audit.sh"
WORKFLOW_FILE = REPO_ROOT / ".github" / "workflows" / "branch-audit.yml"


class _FakeRepo:
    """Builds a self-contained fake git repo with a configurable branch
    topology. The repo is bare-and-clone — actually: we build a "source"
    repo with branches, then clone it as 'origin' so the test repo has
    proper `refs/remotes/origin/*` refs."""

    def __init__(self):
        self.tmp = tempfile.mkdtemp(prefix="tuxlink-audit-test-")
        self.source = Path(self.tmp) / "source"
        self.clone = Path(self.tmp) / "clone"
        self.source.mkdir()
        self.clone.mkdir()

    def _git(self, repo: Path, *args, **kw):
        env = {**os.environ}
        # Hermetic: no user .gitconfig interference.
        env["GIT_CONFIG_GLOBAL"] = "/dev/null"
        env.setdefault("GIT_AUTHOR_EMAIL", "test@example.com")
        env.setdefault("GIT_AUTHOR_NAME", "Test")
        env.setdefault("GIT_COMMITTER_EMAIL", "test@example.com")
        env.setdefault("GIT_COMMITTER_NAME", "Test")
        return subprocess.run(
            ["git", "-C", str(repo), *args],
            check=True,
            capture_output=True,
            text=True,
            env=env,
            **kw,
        )

    def setup(self, topology: dict):
        """topology: dict of branch_name -> list of commit messages.
        'main' is required; other branches are made from main + extra commits.

        Special 'orphan_after' key on a branch dict: indicates that some
        commits should be made AFTER the main-merge happens (the v1p
        pattern).
        """
        # Initialize source as a regular repo (will be cloned as origin).
        self._git(self.source, "init", "-q", "-b", "main")
        # Seed root commit on main.
        (self.source / "README.md").write_text("test\n")
        self._git(self.source, "add", "README.md")
        self._git(self.source, "commit", "-q", "-m", "root")

        # Build each branch.
        for branch, commits in topology.items():
            if branch == "main":
                # Just add extra commits on main.
                for msg in commits:
                    (self.source / f"main-{msg}").write_text(msg)
                    self._git(self.source, "add", f"main-{msg}")
                    self._git(self.source, "commit", "-q", "-m", f"main: {msg}")
                continue
            # Other branches: branch from current main, add commits.
            self._git(self.source, "checkout", "-q", "-b", branch, "main")
            for msg in commits:
                fn = f"{branch.replace('/', '__')}-{msg}"
                (self.source / fn).write_text(msg)
                self._git(self.source, "add", fn)
                self._git(self.source, "commit", "-q", "-m", f"{branch}: {msg}")
            # Back to main for next branch.
            self._git(self.source, "checkout", "-q", "main")

        # Now clone source as origin into clone.
        subprocess.run(
            ["git", "clone", "-q", str(self.source), str(self.clone)],
            check=True,
        )
        # The clone's `origin` IS source. Fetch all branches into clone's
        # remote-tracking refs (clone usually only tracks main by default).
        self._git(self.clone, "fetch", "origin")
        self._git(self.clone, "remote", "update", "origin")

    def merge_branch_to_main(self, branch: str):
        """Simulate a PR merge: merge branch into main on the source side,
        then re-fetch in the clone so origin/main + origin/<branch>
        diverge correctly. Returns the merge-commit SHA on main."""
        self._git(self.source, "checkout", "-q", "main")
        self._git(self.source, "merge", "--no-ff", branch, "-m", f"Merge {branch}")
        merge_sha = self._git(
            self.source, "rev-parse", "HEAD"
        ).stdout.strip()
        # Push the merged state back into the clone.
        self._git(self.clone, "fetch", "origin")
        return merge_sha

    def add_orphan_commits_to(self, branch: str, n: int):
        """Add commits on branch AFTER it's been merged — the v1p pattern."""
        self._git(self.source, "checkout", "-q", branch)
        for i in range(n):
            fn = f"orphan-{branch.replace('/', '__')}-{i}"
            (self.source / fn).write_text(str(i))
            self._git(self.source, "add", fn)
            self._git(
                self.source, "commit", "-q", "-m", f"{branch} orphan: {i}"
            )
        self._git(self.source, "checkout", "-q", "main")
        self._git(self.clone, "fetch", "origin")

    def cleanup(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class _GhStub:
    """Stub `gh` on PATH. The stub responds to:
      - `gh pr list --head <branch> --state <merged|closed|open>`
      - `gh pr list --head <branch> --state merged --json mergeCommit`
    Configurable via `merged_prs` (dict of branch -> merge_sha) and
    `closed_prs` / `open_prs` sets.
    """

    def __init__(
        self,
        merged_prs: dict[str, str] | None = None,
        closed_prs: set[str] | None = None,
        open_prs: set[str] | None = None,
    ):
        self.merged_prs = merged_prs or {}
        self.closed_prs = closed_prs or set()
        self.open_prs = open_prs or set()
        self.tmp = tempfile.mkdtemp(prefix="tuxlink-gh-audit-stub-")
        self._orig_path = os.environ.get("PATH", "")

    def __enter__(self):
        # Build a Python-based gh stub for flexibility (the bash one in the
        # state-machine tests would also work but a Python stub handles the
        # --json + --jq combos more cleanly).
        gh = Path(self.tmp) / "gh"
        # Write the stub as a Python script invoked via env shebang.
        gh.write_text(
            textwrap.dedent(
                f"""\
                #!/usr/bin/env python3
                import sys, json
                MERGED = {dict(self.merged_prs)!r}
                CLOSED = {sorted(self.closed_prs)!r}
                OPEN = {sorted(self.open_prs)!r}

                args = sys.argv[1:]
                # We only handle `gh pr list` and `gh pr view`.
                if not args or args[0] != "pr":
                    sys.exit(0)
                if args[1] == "list":
                    head = ""
                    state = ""
                    json_fields = ""
                    jq_expr = None
                    i = 2
                    while i < len(args):
                        if args[i] == "--head" and i + 1 < len(args):
                            head = args[i + 1]; i += 2; continue
                        if args[i] == "--state" and i + 1 < len(args):
                            state = args[i + 1]; i += 2; continue
                        if args[i] == "--json" and i + 1 < len(args):
                            json_fields = args[i + 1]; i += 2; continue
                        if args[i] == "--jq" and i + 1 < len(args):
                            jq_expr = args[i + 1]; i += 2; continue
                        i += 1

                    results = []
                    if state == "merged" and head in MERGED:
                        results = [{{"number": 1, "mergedAt": "2026-05-31T00:00:00Z", "mergeCommit": {{"oid": MERGED[head]}}}}]
                    elif state == "closed" and head in CLOSED:
                        results = [{{"number": 2, "state": "CLOSED"}}]
                    elif state == "open" and head in OPEN:
                        results = [{{"number": 3}}]

                    if jq_expr:
                        # Implement the trivial jq exprs we use in our scripts.
                        if jq_expr == ".[0].mergeCommit.oid // empty":
                            if results and "mergeCommit" in results[0]:
                                print(results[0]["mergeCommit"]["oid"])
                            sys.exit(0)
                        if jq_expr == ".[0].number // empty":
                            if results:
                                print(results[0].get("number", ""))
                            sys.exit(0)
                        if jq_expr == ".[].name":
                            sys.exit(0)
                        # Fallback — emit empty.
                        sys.exit(0)
                    print(json.dumps(results))
                    sys.exit(0)
                # gh pr view / label list / issue list — return empty JSON [].
                if args[1] == "view":
                    print("{{}}")
                    sys.exit(0)
                sys.exit(0)
                """
            )
        )
        gh.chmod(0o755)
        os.environ["PATH"] = f"{self.tmp}:{self._orig_path}"
        return self

    def __exit__(self, *exc):
        os.environ["PATH"] = self._orig_path
        shutil.rmtree(self.tmp, ignore_errors=True)


def _run_audit(repo_dir: Path) -> tuple[int, str, str, dict, str]:
    """Run scripts/branch-audit.sh inside repo_dir. Returns
    (rc, stdout, stderr, github_output_dict, body_content)."""
    # GITHUB_OUTPUT must be a writable file for the script to record fields.
    gh_output = tempfile.NamedTemporaryFile(
        mode="w", delete=False, prefix="gh-output-", suffix=".env"
    )
    gh_output.close()
    runner_temp = tempfile.mkdtemp(prefix="runner-temp-")
    try:
        # Symlink the script's dependencies into the test repo so the
        # `git rev-parse --show-toplevel` + `.githooks/lib/branch-state.sh`
        # source path works.
        (repo_dir / ".githooks").symlink_to(REPO_ROOT / ".githooks")
        (repo_dir / "scripts").symlink_to(REPO_ROOT / "scripts")

        env = {**os.environ}
        env["GITHUB_OUTPUT"] = gh_output.name
        env["RUNNER_TEMP"] = runner_temp
        proc = subprocess.run(
            ["bash", str(AUDIT_SCRIPT)],
            cwd=repo_dir,
            capture_output=True,
            text=True,
            env=env,
            check=False,
            timeout=60,
        )

        # Parse GITHUB_OUTPUT into a dict.
        out_dict = {}
        with open(gh_output.name) as fp:
            for line in fp:
                line = line.strip()
                if "=" in line:
                    k, _, v = line.partition("=")
                    out_dict[k] = v

        # Read the body file BEFORE we delete runner_temp in the finally.
        # The body_file path the script emits lives under runner_temp, so
        # we must capture its content while runner_temp still exists.
        body_content = ""
        body_path = out_dict.get("body_file", "")
        if body_path and Path(body_path).is_file():
            body_content = Path(body_path).read_text()

        return proc.returncode, proc.stdout, proc.stderr, out_dict, body_content
    finally:
        os.unlink(gh_output.name)
        shutil.rmtree(runner_temp, ignore_errors=True)


class BranchAuditTest(unittest.TestCase):
    """End-to-end tests against fake-repo topologies."""

    def setUp(self):
        self.repo = _FakeRepo()

    def tearDown(self):
        self.repo.cleanup()

    def test_clean_repo_reports_zero_orphans(self):
        """No merged-dead branches → clean audit → exit 0 + clean=true."""
        self.repo.setup(
            {
                "main": ["c1"],
                "bd-tuxlink-aaa/active": ["work"],
            }
        )
        # No merged PRs at all → classifier returns "active" or "unknown".
        with _GhStub(merged_prs={}, open_prs={"bd-tuxlink-aaa/active"}):
            rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
        self.assertEqual(rc, 0, msg=stderr)
        self.assertEqual(gh_out.get("orphan_count"), "0")
        self.assertEqual(gh_out.get("clean"), "true")

    def test_orphan_post_merge_detected(self):
        """Branch with merged PR + post-merge commits → orphan-post-merge."""
        self.repo.setup(
            {
                "main": [],
                "bd-tuxlink-v1p/forms": ["work1", "work2"],
            }
        )
        merge_sha = self.repo.merge_branch_to_main("bd-tuxlink-v1p/forms")
        # Add 2 post-merge commits — these are the orphan commits.
        self.repo.add_orphan_commits_to("bd-tuxlink-v1p/forms", 2)
        with _GhStub(
            merged_prs={"bd-tuxlink-v1p/forms": merge_sha},
        ):
            rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
        self.assertEqual(rc, 0, msg=stderr)
        self.assertEqual(
            gh_out.get("orphan_count"), "1", msg=f"stderr={stderr}"
        )
        self.assertEqual(gh_out.get("clean"), "false")
        # Body content should mention the orphan branch.
        self.assertIn("bd-tuxlink-v1p/forms", body)
        self.assertIn("Orphan-post-merge", body)

    def test_merged_clean_branch_not_flagged(self):
        """Merged branch with NO post-merge commits → not flagged."""
        self.repo.setup(
            {
                "main": [],
                "bd-tuxlink-clean/work": ["a", "b"],
            }
        )
        merge_sha = self.repo.merge_branch_to_main("bd-tuxlink-clean/work")
        # No orphan commits added — branch remote-head equals merge-base.
        with _GhStub(merged_prs={"bd-tuxlink-clean/work": merge_sha}):
            rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
        self.assertEqual(rc, 0, msg=stderr)
        self.assertEqual(
            gh_out.get("orphan_count"), "0", msg=f"stderr={stderr}"
        )

    def test_unknown_classification_keeps_audit_non_clean(self):
        """Codex P1 (2026-06-01) — when gh is unreachable for some branches,
        they bucket as 'unknown'. The audit must NOT emit clean=true (which
        would let the workflow close the tracking issue) even if no orphans
        were found in the branches it could classify.
        """
        # Repo with one branch + main. gh stub that always exits non-zero
        # → classifier returns "unknown" → script must report non-clean.
        self.repo.setup(
            {
                "main": [],
                "bd-tuxlink-mystery/work": ["a"],
            }
        )
        tmp = tempfile.mkdtemp(prefix="tuxlink-gh-fail-")
        try:
            gh_path = Path(tmp) / "gh"
            gh_path.write_text(
                "#!/usr/bin/env bash\n"
                "echo 'auth required' >&2\n"
                "exit 1\n"
            )
            gh_path.chmod(0o755)
            orig_path = os.environ.get("PATH", "")
            os.environ["PATH"] = f"{tmp}:{orig_path}"
            try:
                rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
            finally:
                os.environ["PATH"] = orig_path
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        # gh fails → classifier returns "unknown" for the branch.
        # Even with 0 orphans, clean must NOT be true.
        self.assertEqual(rc, 0, msg=stderr)
        self.assertNotEqual(
            gh_out.get("clean"),
            "true",
            msg=(
                "Codex P1: unknown classifications must not produce clean=true. "
                f"gh_out={gh_out!r} stderr={stderr!r}"
            ),
        )
        self.assertTrue(int(gh_out.get("unknown_count", "0")) > 0)

    def test_bot_branch_ignored(self):
        """Bot-owned branches (release-please--*) skipped from audit."""
        self.repo.setup(
            {
                "main": [],
                "release-please--branches--main": ["bump"],
            }
        )
        merge_sha = self.repo.merge_branch_to_main(
            "release-please--branches--main"
        )
        self.repo.add_orphan_commits_to("release-please--branches--main", 1)
        # Even though it has post-merge commits, the audit ignores
        # release-please branches.
        with _GhStub(
            merged_prs={"release-please--branches--main": merge_sha}
        ):
            rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
        self.assertEqual(rc, 0, msg=stderr)
        self.assertEqual(gh_out.get("orphan_count"), "0")


    def test_squash_merged_branch_bucketed_separately(self):
        """Codex P2 (2026-06-01) — branches whose merge-commit is a single-
        parent commit (squash/rebase merge) cannot be checked for post-merge
        orphans via the 'ahead of main' heuristic. They must bucket into
        squash_merged_branches instead of orphan_branches even if they have
        commits ahead of main."""
        # Set up: a branch with commits, then SQUASH-merged to main (single
        # parent commit on main, NOT a no-ff merge).
        self.repo.setup(
            {
                "main": [],
                "bd-tuxlink-squashed/work": ["w1", "w2"],
            }
        )
        # Do a squash-merge manually (not via merge_branch_to_main which
        # does --no-ff). The squash retains the branch's content but as
        # a single new commit on main.
        self.repo._git(self.repo.source, "checkout", "-q", "main")
        self.repo._git(
            self.repo.source,
            "merge",
            "--squash",
            "bd-tuxlink-squashed/work",
        )
        self.repo._git(
            self.repo.source,
            "commit",
            "-q",
            "-m",
            "Squashed: bd-tuxlink-squashed/work",
        )
        squash_commit = self.repo._git(
            self.repo.source, "rev-parse", "HEAD"
        ).stdout.strip()
        # Update the clone to see the squash-merge.
        self.repo._git(self.repo.clone, "fetch", "origin")
        with _GhStub(
            merged_prs={"bd-tuxlink-squashed/work": squash_commit},
        ):
            rc, stdout, stderr, gh_out, body = _run_audit(self.repo.clone)
        self.assertEqual(rc, 0, msg=stderr)
        # The branch should bucket as squash-merged, NOT as orphan.
        self.assertEqual(
            gh_out.get("orphan_count"), "0", msg=stderr,
        )
        self.assertEqual(
            gh_out.get("squash_merged_count"), "1", msg=stderr,
        )

    def test_markdown_escape_branch_names_with_pipe_and_backtick(self):
        """Codex P3 (2026-06-01) — branch names with markdown-significant
        characters (pipe, backtick) must be escaped in the issue body so
        they don't corrupt markdown table parsing or break inline-code spans."""
        # Test the escape helper directly via the script's sourced library.
        # Using subprocess to source + invoke _audit_md_escape.
        r = subprocess.run(
            [
                "bash",
                "-c",
                (
                    "set -e\n"
                    # Source the script via grep-based exec (the script's
                    # `main` is at the end; sourcing it would run main).
                    "AUDIT='" + str(REPO_ROOT / "scripts" / "branch-audit.sh") + "'\n"
                    # Run the function in isolation by extracting it.
                    "eval \"$(sed -n '/^_audit_md_escape()/,/^}/p' \"$AUDIT\")\"\n"
                    "_audit_md_escape 'foo|bar`baz\\\\qux'\n"
                ),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(r.returncode, 0, msg=r.stderr)
        # `|` → `\|`, backtick → backslash-backtick, backslash → escaped.
        self.assertIn("\\|", r.stdout)
        self.assertIn("\\`", r.stdout)


class WorkflowSanityTest(unittest.TestCase):
    """Lightweight syntactic checks on the workflow file."""

    def test_workflow_file_parses(self):
        """The workflow YAML must parse cleanly."""
        try:
            import yaml  # type: ignore
        except ImportError:
            self.skipTest("PyYAML not available; skipping YAML parse check")
        with WORKFLOW_FILE.open() as fp:
            data = yaml.safe_load(fp)
        # PyYAML loads YAML's `on:` key as Python boolean True (because
        # YAML 1.1 treats 'on' as boolean). Accept either form.
        self.assertTrue("jobs" in data, msg=f"missing jobs key: {data.keys()}")
        on_key = True if True in data else "on"
        self.assertIn(on_key, data, msg=f"missing on/True key: {data.keys()}")
        on_value = data[on_key]
        self.assertIn("schedule", on_value)
        self.assertIn("workflow_dispatch", on_value)
        self.assertIn("audit", data["jobs"])

    def test_workflow_uses_env_var_injection_not_direct_interpolation(self):
        """Codex/security-hook check: untrusted inputs (issue body, etc.)
        must not be interpolated directly into `run:` blocks. We use
        env-var pattern throughout."""
        content = WORKFLOW_FILE.read_text()
        # Search for the unsafe pattern: bare ${{ }} in a run block.
        # The grep is loose — we just confirm any ${{ ... }} that appears
        # IS in an `env:` block, not bare in `run:`.
        run_lines = []
        in_run_block = False
        for line in content.splitlines():
            stripped = line.strip()
            if stripped.startswith("run:") or stripped.startswith("run: |"):
                in_run_block = True
                continue
            if in_run_block:
                # Pop out of the run block when we hit a top-level key
                # (heuristic: a new dashed step or `- name:`).
                if (
                    stripped.startswith("- ")
                    or stripped.startswith("- name:")
                    or stripped.startswith("name:")
                    or (stripped.startswith("env:") and not run_lines)
                ):
                    in_run_block = False
                else:
                    run_lines.append(line)
        for line in run_lines:
            self.assertNotIn(
                "${{ github.event.",
                line,
                msg=f"unsafe direct interpolation of github.event: {line!r}",
            )
            self.assertNotIn(
                "${{ inputs.",
                line,
                msg=f"unsafe direct interpolation of inputs.*: {line!r}",
            )

    def test_audit_script_is_executable(self):
        self.assertTrue(os.access(AUDIT_SCRIPT, os.X_OK))


if __name__ == "__main__":
    unittest.main(verbosity=2)
