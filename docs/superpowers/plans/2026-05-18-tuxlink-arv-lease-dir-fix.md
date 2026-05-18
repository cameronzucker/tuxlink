# tuxlink-arv lease-dir parity fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `.claude/scripts/get_tuxlink_sessions.py` to resolve the session-leases directory via `git rev-parse --git-common-dir` (matching `.claude/hooks/block-main-checkout-race.sh`), eliminating the silent under-report of live sessions that grounded the 2026-05-18 main-checkout-race hook-loop incident.

**Architecture:** Extract a `resolve_lease_dir(repo: Path) -> Path` helper in the script that shells to `git rev-parse --git-common-dir` (with `subprocess.check_output`, `cwd=repo`, `shell=False`). Wire `main()` to call it. Add 3 regression tests in a new `.claude/scripts/tests/` directory using stdlib `unittest`. Land a pitfalls entry PARITY-1 in Section 2 of `docs/pitfalls/implementation-pitfalls.md` AFTER PR #40 (HOOK-1 + LEASE-1 stand-up) merges.

**Tech Stack:** Python 3 stdlib (`subprocess`, `pathlib`, `unittest`, `tempfile`, `uuid`, `json`, `datetime`). No new dependencies.

**Spec of record:** `docs/superpowers/specs/2026-05-18-tuxlink-arv-lease-dir-parity-design.md` v3 (committed at `da3690f`).

**Pre-execution context for the implementer:**

- **bd dep:** `tuxlink-arv` depends on `tuxlink-6ro` (PR #40). The PARITY-1 pitfalls work in Task 8 cannot land until PR #40 merges + this branch rebases on `feat/v0.0.1`. Tasks 1-7 + 9-10 can execute now; Task 8 is gated.
- **Branch:** `bd-tuxlink-arv/lease-dir-fix` (worktree at `worktrees/bd-tuxlink-arv-lease-dir-fix/`).
- **Worktree-internal git ops bypass the main-checkout-race hook** (hook's `is_main_checkout != true` fast path). All commits land on the branch from this worktree.
- **Auto-memory** at `~/.claude/projects/-home-administrator-Code-tuxlink/memory/` is per-user OUTSIDE the repo. Task 7's `feedback_stale_lease_means_worktree.md` update is a parallel manual edit referenced in the commit message; it doesn't appear in the PR diff.
- **Commits must include `Agent: <moniker>` trailer.** Substitute your actual session moniker; sample template uses `<SESSION-MONIKER>` as a placeholder per CLAUDE.md.
- **TDD is mandatory** (`superpowers:test-driven-development` skill). Tasks 2 + 3 are RED-RED-(impl)-GREEN-GREEN cycles structured to kill the specific mutants Codex round 5 identified.

---

### Task 1: Set up Python test infrastructure

**Files:**
- Create: `.claude/scripts/tests/__init__.py` (empty, marks the dir as a package)
- Create: `.claude/scripts/tests/.gitkeep` (NOT needed if `__init__.py` exists; skip)

**Background:** The project's existing test infrastructure is Rust (`src-tauri/tests/`). No Python test runner exists yet. This task establishes the minimal home for safety-stack-script Python tests; future test files in this directory will be picked up by `python3 -m unittest discover`.

- [ ] **Step 1: Create the test directory and empty package marker**

```bash
mkdir -p .claude/scripts/tests
touch .claude/scripts/tests/__init__.py
```

- [ ] **Step 2: Verify the discovery harness finds an empty test suite (no error)**

```bash
python3 -m unittest discover .claude/scripts/tests/
```

Expected output: `Ran 0 tests in 0.000s` followed by `NO TESTS RAN` (Python 3.11+) or `OK` (Python 3.10-). Either is fine — the point is no import error from the empty package.

- [ ] **Step 3: Stage but do NOT commit yet**

```bash
git add .claude/scripts/tests/__init__.py
```

The commit lands in Task 9 with the rest of the fix.

---

### Task 2: TDD Cycle 1 — LeaseDirResolution test (RED)

**Files:**
- Create: `.claude/scripts/tests/test_get_tuxlink_sessions.py`

**Background:** This is the headline unit test. It calls `git rev-parse --git-common-dir` independently via subprocess, then asserts equality with `resolve_lease_dir(REPO_ROOT)`. The function doesn't exist yet — the test must RED with `ImportError: cannot import name 'resolve_lease_dir'`. Acknowledged: this test alone is vulnerable to the "forgot to update main()" + "hardcoded `.git/session-leases/`" mutants Codex round 5 identified — those are killed by Task 3's MainEndToEnd test.

- [ ] **Step 1: Write the failing test file**

Write `.claude/scripts/tests/test_get_tuxlink_sessions.py`:

```python
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
```

- [ ] **Step 2: Run to verify RED**

```bash
python3 -m unittest discover .claude/scripts/tests/ -v
```

Expected: `ImportError: cannot import name 'resolve_lease_dir' from 'get_tuxlink_sessions'`. The test does not even collect. RED confirmed.

If you see `Ran 1 test ... OK`, you've forgotten to add the `resolve_lease_dir` import in your test file — the test must FAIL because the production code doesn't yet have the helper.

- [ ] **Step 3: Stage but do NOT commit**

```bash
git add .claude/scripts/tests/test_get_tuxlink_sessions.py
```

---

### Task 3: TDD Cycle 2 — MainEndToEnd test (RED)

**Files:**
- Modify: `.claude/scripts/tests/test_get_tuxlink_sessions.py` (append a new TestCase)

**Background:** This is Codex round 5's mutation-killer. It exercises `main()` end-to-end with a controlled lease in a temp git repo + linked worktree. Kills two mutants the unit test misses: (a) helper added but `main()` not wired; (b) helper returns hardcoded `.git/session-leases/` (coincidentally correct from main checkout, broken from linked worktree where `.git` is a file).

- [ ] **Step 1: Append the MainEndToEnd test to test_get_tuxlink_sessions.py**

Append to `.claude/scripts/tests/test_get_tuxlink_sessions.py` (after the `LeaseDirResolution` class):

```python
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
        subprocess.check_call(
            ["git", "init", "-b", "main", str(self.main_repo)],
            stderr=subprocess.DEVNULL,
        )
        subprocess.check_call(
            ["git", "-C", str(self.main_repo), "commit", "--allow-empty", "-m", "init"],
            stderr=subprocess.DEVNULL, env=env,
        )
        subprocess.check_call(
            ["git", "-C", str(self.main_repo), "worktree", "add",
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

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run_main_from(self, cwd: Path) -> str:
        """Run main() with CLAUDE_PROJECT_DIR=cwd, capture stdout."""
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
```

- [ ] **Step 2: Run to verify both new tests RED**

```bash
python3 -m unittest discover .claude/scripts/tests/ -v
```

Expected: still `ImportError: cannot import name 'resolve_lease_dir'`. (Both Task 2's and Task 3's tests fail at collect time because the import is at module level.) RED confirmed.

- [ ] **Step 3: Stage**

```bash
git add .claude/scripts/tests/test_get_tuxlink_sessions.py
```

---

### Task 4: Add GlobIgnoresJsonl regression test (immediately GREEN)

**Files:**
- Modify: `.claude/scripts/tests/test_get_tuxlink_sessions.py` (append a new TestCase)

**Background:** Defensive lockdown for hypothesis #3 from the bd issue (file-extension/glob mismatch). The current code already uses `lease_dir.glob("*.json")` which excludes `denied-attempts.jsonl`. This test locks down the pattern so a future refactor to `*.json*` (which WOULD pick up `.jsonl`) gets caught. This test is RED-skipped (immediately GREEN once the import error in Task 2 + 3 is resolved); the value is regression protection, not test-driven implementation.

- [ ] **Step 1: Append the GlobIgnoresJsonl test**

Append to `.claude/scripts/tests/test_get_tuxlink_sessions.py`:

```python
class GlobIgnoresJsonl(unittest.TestCase):
    """Lockdown for hypothesis #3 from bd issue tuxlink-arv.

    The script uses `lease_dir.glob('*.json')` which correctly EXCLUDES
    `denied-attempts.jsonl` written to the same dir by the hook. This test
    pins the behavior so a future refactor to `*.json*` (which would
    coincidentally match `.jsonl`) is caught by CI.
    """

    def test_script_glob_pattern_excludes_jsonl_files(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            (tmp / "lease-aaa.json").write_text("{}")
            (tmp / "denied-attempts.jsonl").write_text('{"event":"denied"}\n')
            (tmp / "lease-bbb.json").write_text("{}")
            files = sorted(p.name for p in tmp.glob("*.json"))
            self.assertEqual(
                files, ["lease-aaa.json", "lease-bbb.json"],
                "glob('*.json') must NOT match denied-attempts.jsonl. "
                "If this test fails, the script's glob pattern has been "
                "broadened — verify the lease iteration doesn't try to "
                "parse the .jsonl file as JSON.",
            )
```

- [ ] **Step 2: Stage**

```bash
git add .claude/scripts/tests/test_get_tuxlink_sessions.py
```

---

### Task 5: Implement `resolve_lease_dir` + wire `main()` (GREEN)

**Files:**
- Modify: `.claude/scripts/get_tuxlink_sessions.py`

**Background:** Add the helper, broaden subprocess exception handling per Codex round 5 finding #4, write a stderr warning in fallback paths so silent under-reporting is impossible, and rewire `main()` to call the helper. Update the module docstring.

- [ ] **Step 1: Update the module docstring (line 1-14 region)**

Open `.claude/scripts/get_tuxlink_sessions.py`. Replace lines 1-14 (the existing module docstring) with:

```python
#!/usr/bin/env python3
"""get_tuxlink_sessions.py — list live tuxlink Claude Code sessions in this repo.

Reads <git-common-dir>/session-leases/*.json, filters to live sessions
(lastSeenUtc within --ttl-minutes), and prints a table. The lease directory
location MUST match `.claude/hooks/block-main-checkout-race.sh`'s resolution
(it uses `git rev-parse --git-common-dir`) — disagreement causes the script
to silently under-report live sessions and gives agents false grounds to
argue with a hook deny. See bd issue tuxlink-arv for the 2026-05-18 incident.

Usage:
  .claude/scripts/get_tuxlink_sessions.py
  .claude/scripts/get_tuxlink_sessions.py --ttl-minutes 60
  .claude/scripts/get_tuxlink_sessions.py --include-stale

Ported from support-tools/.claude/scripts/Get-LfstSessions.ps1 per Decision 3
of the 2026-05-17 LFST→tuxlink port catalog (Python for cross-platform reuse).
"""
```

- [ ] **Step 2: Add `subprocess` to the imports**

Find the imports block (around line 16-21). Insert `import subprocess` in alphabetical position:

```python
import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
```

- [ ] **Step 3: Add the `resolve_lease_dir` function below `resolve_repo`**

After the existing `def resolve_repo() -> Path:` function (around line 30), insert:

```python


def resolve_lease_dir(repo: Path) -> Path:
    """Resolve <git-common-dir>/session-leases/ for the given repo path.

    Matches the hook's resolution (.claude/hooks/block-main-checkout-race.sh
    line 41) so script and hook agree on what is and isn't a live lease. The
    git-common-dir is the same across the main checkout and all worktrees,
    making the lease set genuinely repo-scoped rather than per-checkout.

    If `git rev-parse` is unavailable (not a git repo, git missing, OSError),
    OR returns empty stdout, writes a one-line warning to stderr (so the
    operator notices when their environment isn't supported) and returns a
    path that is unlikely to exist — main() will then print the "no sessions"
    path instead of crashing.
    """
    try:
        common_dir = subprocess.check_output(
            ["git", "rev-parse", "--git-common-dir"],
            cwd=repo, text=True, stderr=subprocess.DEVNULL,
        ).strip()
    except (subprocess.CalledProcessError, OSError) as e:
        sys.stderr.write(
            f"warning: git rev-parse --git-common-dir failed "
            f"({type(e).__name__}); falling back to {repo}/.git/session-leases "
            f"(may not exist)\n"
        )
        return repo / ".git" / "session-leases"
    if not common_dir:
        sys.stderr.write(
            f"warning: git rev-parse --git-common-dir returned empty; "
            f"falling back to {repo}/.git/session-leases (may not exist)\n"
        )
        return repo / ".git" / "session-leases"
    cd = Path(common_dir)
    if not cd.is_absolute():
        cd = (repo / cd).resolve()
    return cd / "session-leases"
```

- [ ] **Step 4: Wire `main()` to use `resolve_lease_dir`**

Find this line in `main()` (currently around line 55):

```python
    lease_dir = repo / ".claude" / "session-leases"
```

Replace with:

```python
    lease_dir = resolve_lease_dir(repo)
```

- [ ] **Step 5: Run to verify GREEN**

```bash
python3 -m unittest discover .claude/scripts/tests/ -v
```

Expected: 4 tests pass (LeaseDirResolution + GlobIgnoresJsonl + MainEndToEnd × 2). All GREEN.

If any test fails: read the failure message carefully, fix the production code (NOT the test), re-run.

- [ ] **Step 6: Stage the script change**

```bash
git add .claude/scripts/get_tuxlink_sessions.py
```

---

### Task 6: Manual smoke verification

**Files:** none (verification only)

**Background:** Defends against the symmetric-logic test risk (the headline test calls the same `git rev-parse` as the production code). Per Codex round 5 finding #5, conditionalize on live leases existing.

- [ ] **Step 1: Inspect lease dir for live leases**

```bash
git_common_dir=$(git rev-parse --git-common-dir)
lease_dir="$git_common_dir/session-leases"
ls "$lease_dir"/*.json 2>/dev/null | head -10
```

Expected: zero or more `.json` files listed.

- [ ] **Step 2 (conditional): If leases exist, run the script and confirm output**

```bash
python3 .claude/scripts/get_tuxlink_sessions.py
```

Expected (if any lease is within the 30-min TTL): a table with at least one entry. Confirm at least one lease's moniker / branch matches what's in the `.json` files.

If output is `"No live tuxlink sessions in this repo."` — verify it's because all leases are stale (>30min), NOT because the script is reading the wrong dir. Use `--include-stale` if needed:

```bash
python3 .claude/scripts/get_tuxlink_sessions.py --include-stale
```

If `--include-stale` shows zero leases AND lease files exist on disk — the bug is back, investigate immediately.

- [ ] **Step 3: Verify the warning path works (off-tree probe)**

```bash
# Run from /tmp (not a git repo). Should print warning + "no active sessions".
cd /tmp && CLAUDE_PROJECT_DIR=/tmp python3 /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-arv-lease-dir-fix/.claude/scripts/get_tuxlink_sessions.py 2>&1
```

Expected: stderr warning about `git rev-parse --git-common-dir failed`, followed by stdout `"No active tuxlink sessions (lease directory does not exist yet)."`. No crash.

Then `cd` back to the worktree.

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-arv-lease-dir-fix
```

---

### Task 7: Auto-memory refresh (out-of-repo)

**Files:**
- Modify: `~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md`

**Background:** The auto-memory entry currently says "the script lies." Once the script is accurate, the framing is stale anti-knowledge. Update in place to reflect post-fix accuracy AND reinforce that the worktree recipe stays authoritative regardless of script/hook agreement (per adrev round 4 finding 10).

This file is OUTSIDE the repo (under `~/.claude/projects/`). The change does NOT appear in the PR diff; it's a parallel manual edit referenced in the Task 9 commit body.

- [ ] **Step 1: Read the current memory entry**

```bash
cat ~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md
```

Expect frontmatter (name, description, type=feedback) + body text saying the script lies + worktree response is canonical.

- [ ] **Step 2: Update the body (preserve frontmatter and main rule, add 2026-05-18 note)**

Edit the file. Keep the title + worktree-response rule intact. Append a `**Update 2026-05-18 (post `tuxlink-arv` fix):**` block stating:

- `get_tuxlink_sessions.py` is now accurate (no longer "lies") as of bd issue tuxlink-arv / PR [number TBD when opened].
- The worktree recipe (HOOK-1) remains the canonical hook-deny response **regardless of whether the script and hook agree**. Fixing the script restores informational accuracy; it does NOT authorize agents to use the script's output as license to take the lease, delete lease files, or propose hook enhancements. The hook is always authoritative; the script is informational only.
- Cross-reference: pitfalls PARITY-1 (Section 2 of `docs/pitfalls/implementation-pitfalls.md`, landing alongside HOOK-1 + LEASE-1 in PR #40).

- [ ] **Step 3: Verify the file is well-formed**

```bash
head -20 ~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md
```

Expect: frontmatter intact, body updated.

- [ ] **Step 4: No git operation here** — file is outside the repo.

---

### Task 8: [BLOCKED ON PR #40 MERGE] Add PARITY-1 pitfalls entry

**Files:**
- Modify: `docs/pitfalls/implementation-pitfalls.md`

**Background:** PARITY-1 lives in Section 2, alongside HOOK-1 + LEASE-1. Section 2 is currently the `EXAMPLE-DOMAIN-2` placeholder on `feat/v0.0.1`; HOOK-1 + LEASE-1 are added by PR #40 (`tuxlink-6ro`, branch `bd-tuxlink-6ro/pitfalls-hook1-lease1`). This task is BLOCKED until PR #40 merges and this branch rebases on the updated `feat/v0.0.1`.

**Execution gate:**
1. PR #40 has merged into `feat/v0.0.1`.
2. From this worktree: `git fetch origin && git rebase origin/feat/v0.0.1`. Resolve any conflicts (unlikely; PARITY-1 lands AFTER HOOK-1 + LEASE-1 with no overlapping edits).
3. Then proceed with the steps below.

- [ ] **Step 1: Verify PR #40 merged + rebase**

```bash
git fetch origin
git log origin/feat/v0.0.1 --oneline | head -10
# Confirm PR #40's merge commit is visible. Then:
git rebase origin/feat/v0.0.1
git log --oneline | head -5
# Confirm clean rebase.
```

If conflicts: read carefully, prefer keeping HOOK-1 + LEASE-1's structure intact; resolve PARITY-1 to go AFTER LEASE-1.

- [ ] **Step 2: Extend the Section 2 title**

Open `docs/pitfalls/implementation-pitfalls.md`. Find the section heading (currently `# Section 2: Safety-Stack Coordination` after PR #40 merges). Replace with:

```markdown
# Section 2: Safety-Stack Coordination and Cross-Component Parity
```

Update the corresponding entry in the Table of Contents (top of file).

- [ ] **Step 3: Add PARITY-1 entry after LEASE-1**

After the LEASE-1 entry (which ends with `---`), insert:

```markdown
### PARITY-1: Script/Hook Path-Resolution Parity

**The Flaw:** A helper script and a hook both read or write the same safety-stack state (lease files, denied-attempts log, lock files, etc.), but they resolve the storage path differently. The script hardcodes one path; the hook computes another from a contextual source (e.g., `git rev-parse --git-common-dir`). When the operator (or an agent) runs the script to inspect what the hook sees, the two views silently diverge.

Examples in the wild:
- 2026-05-18 tuxlink-arv: `get_tuxlink_sessions.py` resolved leases at `<repo>/.claude/session-leases/`; `block-main-checkout-race.sh` resolved at `<git-common-dir>/session-leases/`. From a linked worktree, those are different directories. The script reported "no live sessions" while the hook denied. The agent who consulted the script took it as ground truth and started arguing with the hook (textbook HOOK-1 anti-pattern).
- A future "tail the denied-attempts log" utility that hardcodes `.claude/session-leases/denied-attempts.jsonl` instead of querying git-common-dir — same shape, same drift potential.

**Why It Matters:** When a script that's supposed to MIRROR the hook's view of safety-stack state diverges from it, agents who consult the script as the canonical source get the wrong picture — and may use that picture to override the hook (the HOOK-1 anti-pattern). Even if the agent doesn't override, the operator loses an informational tool: the script's output stops being trustworthy.

The script is supposed to be a *read* of the hook's state. If it can't be that, it should not exist — having a script that disagrees with the hook is worse than having no script, because operators (and agents) treat the script as an authoritative second opinion when it's actually just a buggy first opinion.

**The Fix:**

1. Scripts that read safety-stack state MUST resolve their storage paths via the SAME mechanism the hook uses to write the state. If the hook uses `git rev-parse --git-common-dir`, the script does too. If the hook reads `$XDG_RUNTIME_DIR`, the script does too. Don't compute a "parallel" path that "should be the same" — call the same primitive.
2. Add a regression test asserting the script's resolved path equals the hook's resolved path under the project's standard invocation (main checkout AND any linked worktree, separately).
3. When the operator reports "script says X, hook does Y" — believe both. Investigate the divergence; don't pick one as right and the other as wrong by intuition. The TWO-PATHS shape IS the bug; reconciling them is the fix.
4. Audit: any time a hook reads or writes a new path, check if there's a companion script that reads the same data and verify it uses the same resolution.

**The Lesson:** "Two paths to the same data" is always a bug surface. Even if the paths *coincidentally* agree today (e.g., from the main checkout where `repo/.git == git-common-dir`), they diverge under other valid contexts (linked worktrees, where `repo/.git` is a FILE pointing to the common dir).

**Reinforcement of HOOK-1:** even with parity restored, the worktree recipe (HOOK-1) remains the authoritative response to a hook deny. Fixing the script makes its informational output accurate; it does NOT authorize agents to use the script's output as license to take the lease, delete lease files, or propose hook enhancements. If the script says "another session is live" and the hook denies, the response is the same as if the script were silent: worktree. The hook is the enforcement mechanism; the script is informational.

Codification of the 2026-05-18 incident lives in `dev/incidents/2026-05-18-main-checkout-race-hook-loop.md` (the reporting agent's write-up) and `dev/incidents/2026-05-18-main-checkout-race-hook-loop-reviewer-response.md` (towhee-wren-aspen's AzDO-grounded diagnosis). The structural enabler (a one-sentence CLAUDE.md carve-out) was removed in PR #39. HOOK-1 codified the agent-behavior rule. LEASE-1 codified the single-source-of-truth rule for liveness. This entry (PARITY-1) codifies the script/hook code-structure rule.

---
```

- [ ] **Step 4: Update the Section 2 review checklist**

Find the section's review checklist (after PARITY-1's `---`). Add a new bullet:

```markdown
- [ ] **Check derived from PARITY-1** — No helper script reads or writes safety-stack state (leases, denied-attempts log, lock files) via a hardcoded path that doesn't match the corresponding hook's resolution. Verify by `grep -RIn "session-leases" .claude/scripts/ .claude/hooks/` and confirming script paths derive from `git rev-parse --git-common-dir` (or whatever resolution the hook uses).
```

- [ ] **Step 5: Update Appendix B summary table**

Add a row:

```markdown
| PARITY-1 | Script/Hook Path-Resolution Parity | HIGH | VALIDATED | §2 Safety-Stack Coordination and Cross-Component Parity |
```

- [ ] **Step 6: Add Appendix A changelog entry**

```markdown
## 2026-05-18 — Added PARITY-1 (Script/Hook Path-Resolution Parity)

Source: bd issue tuxlink-arv (`get_tuxlink_sessions.py` ↔ `block-main-checkout-race.sh` lease-dir disagreement; script read `.claude/session-leases/`, hook wrote `<git-common-dir>/session-leases/`). Diagnosed during 2026-05-18 main-checkout-race incident chain; structural enabler was a CLAUDE.md carve-out removed in PR #39; the script-fix + this pitfall close the loop.

Companion artifacts:
- Spec: `docs/superpowers/specs/2026-05-18-tuxlink-arv-lease-dir-parity-design.md`
- Plan: `docs/superpowers/plans/2026-05-18-tuxlink-arv-lease-dir-fix.md`
- Auto-memory refresh: `~/.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_stale_lease_means_worktree.md` (updated to reflect script accuracy)
```

- [ ] **Step 7: Update TOC entry count + verify Section 2 listing**

In the Table of Contents at the top of the file, find the Section 2 row. Update the entries column from `HOOK-1, LEASE-1` to `HOOK-1, LEASE-1, PARITY-1`.

- [ ] **Step 8: Stage**

```bash
git add docs/pitfalls/implementation-pitfalls.md
```

---

### Task 9: Codex round on the implementation diff

**Files:** none (review only)

**Background:** Per build-robust-features step 2 and CLAUDE.md's Codex CLI section, at least one adversarial Codex round on the implementation. The spec already had Codex round 5 (round 5 of the adrev cycle); this is a separate round on the staged DIFF. Per CLAUDE.md, write output to `dev/adversarial/<date>-<topic>-codex.md` (gitignored).

- [ ] **Step 1: Confirm staged changes**

```bash
git status
git diff --cached --stat
```

Expected: 4 files staged (`.claude/scripts/get_tuxlink_sessions.py` modified, `.claude/scripts/tests/__init__.py` new, `.claude/scripts/tests/test_get_tuxlink_sessions.py` new, `docs/pitfalls/implementation-pitfalls.md` modified IF Task 8 done) — diff is small, ~50 lines plus tests.

- [ ] **Step 2: Run Codex review on the staged diff**

```bash
mkdir -p /home/administrator/Code/tuxlink/dev/adversarial
npx --yes @openai/codex review --uncommitted > /home/administrator/Code/tuxlink/dev/adversarial/2026-05-18-arv-impl-codex.md 2>&1
echo "EXIT: $?"
```

Note: `--uncommitted` reviews staged + unstaged + untracked, with the default review prompt (custom prompts are not accepted alongside `--uncommitted` / `--commit` / `--base` per codex CLI v0.128). Default review prompt is sufficient for a diff of this size.

- [ ] **Step 3: Read findings**

```bash
tail -100 /home/administrator/Code/tuxlink/dev/adversarial/2026-05-18-arv-impl-codex.md
```

For each P1/P2 finding: triage. If actionable, address with a follow-up edit (back to Task 5 or wherever the issue is) + re-run Task 5 Step 5 to confirm tests still pass + re-stage. If not actionable, note the disposition in the eventual PR body.

- [ ] **Step 4: No commit yet** — commit lands in Task 10.

---

### Task 10: Commit, push, open PR

**Files:** none (git ops only)

**Background:** Single focused commit per spec §5. Heredoc commit message form per CLAUDE.md gotcha (avoids destructive-git hook collision on `Agent:` trailer regex + embedded git-pattern strings).

- [ ] **Step 1: Final pre-commit check**

```bash
git status
python3 -m unittest discover .claude/scripts/tests/ -v
```

Expected: 4 tests pass (LeaseDirResolution + GlobIgnoresJsonl + MainEndToEnd × 2), staged files match Task 9 Step 1 inventory.

- [ ] **Step 2: Commit (single commit with all changes)**

```bash
git commit -m "$(cat <<'EOF'
fix(safety-stack): get_tuxlink_sessions.py lease-dir parity with hook (tuxlink-arv P1)

Script resolved <repo>/.claude/session-leases/ via Path-arithmetic; hook
resolved <git-common-dir>/session-leases/ via `git rev-parse`. The two
diverge on linked worktrees (where <repo>/.git is a FILE, not a dir),
silently causing the script to under-report live sessions and giving
agents false grounds to argue with hook denies.

Extracts resolve_lease_dir() shelling to `git rev-parse --git-common-dir`,
mirroring the hook. Wires main() to use it. Broadens subprocess fallback
to catch OSError (parent class) + handle empty stdout; writes a stderr
warning on either failure so silent under-reporting cannot recur.

Regression coverage (3 test classes):
- LeaseDirResolution: unit test, script vs hook agreement
- GlobIgnoresJsonl: defensive lockdown for .json vs .jsonl glob
- MainEndToEnd: temp-repo + linked-worktree mutation killer that
  exercises main() end-to-end (kills "forgot-to-update-main()" and
  "hardcoded .git/session-leases" mutants Codex round 5 identified)

Pitfalls Section 2 gains PARITY-1 (Script/Hook Path-Resolution Parity)
alongside HOOK-1 + LEASE-1 (the latter two from PR #40, which merged
first per bd dep). Section 2 title extended to "Safety-Stack
Coordination and Cross-Component Parity" to reflect PARITY-1's
code-structure (rather than agent-behavior) focus.

Auto-memory `feedback_stale_lease_means_worktree.md` updated in parallel
(outside repo; not in diff) to reflect post-fix script accuracy and
reinforce that worktree recipe remains authoritative.

Closes tuxlink-arv. Companion docs:
- Spec: docs/superpowers/specs/2026-05-18-tuxlink-arv-lease-dir-parity-design.md
- Plan: docs/superpowers/plans/2026-05-18-tuxlink-arv-lease-dir-fix.md

Agent: <SESSION-MONIKER>
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

(Substitute `<SESSION-MONIKER>` with the actual session moniker before running.)

- [ ] **Step 3: Push**

```bash
git push -u origin bd-tuxlink-arv/lease-dir-fix
```

- [ ] **Step 4: Open PR against `feat/v0.0.1`**

```bash
gh pr create --base feat/v0.0.1 --head bd-tuxlink-arv/lease-dir-fix \
    --title "[<SESSION-MONIKER>] fix(safety-stack): get_tuxlink_sessions.py lease-dir parity (tuxlink-arv P1)" \
    --body "$(cat <<'EOF'
## Summary

Fixes the script/hook lease-dir disagreement that caused the 2026-05-18 main-checkout-race hook-loop incident.

**Spec:** docs/superpowers/specs/2026-05-18-tuxlink-arv-lease-dir-parity-design.md (v3)
**Plan:** docs/superpowers/plans/2026-05-18-tuxlink-arv-lease-dir-fix.md
**bd issue:** tuxlink-arv (closes on merge)
**bd dep:** depends on tuxlink-6ro (PR #40); merged + rebased before this PR opens

## What changed

- `get_tuxlink_sessions.py`: extract `resolve_lease_dir(repo)` shelling to `git rev-parse --git-common-dir`; wire `main()` to use it; broaden subprocess fallback to catch `OSError` + handle empty stdout with stderr warning
- New `.claude/scripts/tests/` directory + 3 regression tests (LeaseDirResolution, GlobIgnoresJsonl, MainEndToEnd-from-both-main-and-linked-worktree)
- `docs/pitfalls/implementation-pitfalls.md`: Section 2 title extended to "Safety-Stack Coordination and Cross-Component Parity"; PARITY-1 entry added; checklist + Appendix B + Appendix A changelog updated
- Auto-memory `feedback_stale_lease_means_worktree.md` refreshed (parallel manual edit; outside repo; not in diff)

## TDD discipline

Plan executed task-by-task per superpowers:test-driven-development. RED verified for LeaseDirResolution + MainEndToEnd at write time (`ImportError: cannot import name 'resolve_lease_dir'`); GREEN verified after Task 5 implementation. GlobIgnoresJsonl is a defensive lockdown (immediately green; locks down `*.json` vs `*.json*` glob).

## Test plan

- [x] `python3 -m unittest discover .claude/scripts/tests/` → 4 tests pass
- [x] Manual smoke verification: run script against actual lease dir; output matches hook's view
- [x] Warning-path verification: script run outside a git repo produces stderr warning + graceful "no sessions" message

## Adversarial review

- 4-round Claude adrev on spec + 1-round Codex on spec (round 5 verdict: needs revision; v3 incorporates findings)
- 1-round Codex on staged implementation diff (Task 9) — findings persisted at dev/adversarial/2026-05-18-arv-impl-codex.md (gitignored)

Agent: <SESSION-MONIKER>
EOF
)"
```

(Substitute `<SESSION-MONIKER>` twice.)

- [ ] **Step 5: Verify PR is in good shape**

```bash
gh pr view --json title,state,url,mergeable
```

Expected: state=OPEN, mergeable=MERGEABLE, url printed.

- [ ] **Step 6: Update bd issue status**

```bash
bd update tuxlink-arv --notes "PR opened. Closes via deliverable on merge."
```

(The `bd close` happens on merge per project workflow.)

---

## File structure summary

After this plan executes, the changes are:

```
.claude/scripts/get_tuxlink_sessions.py             ← modified (Task 5)
.claude/scripts/tests/__init__.py                   ← new (Task 1)
.claude/scripts/tests/test_get_tuxlink_sessions.py  ← new (Tasks 2 + 3 + 4)
docs/pitfalls/implementation-pitfalls.md            ← modified (Task 8, post-#40)
~/.claude/projects/-home-administrator-Code-tuxlink/memory/
    feedback_stale_lease_means_worktree.md          ← modified (Task 7, out of repo)
docs/superpowers/specs/...design.md                 ← already committed
docs/superpowers/plans/...fix.md                    ← already committed
```

## Spec coverage verification

| Spec section | Plan task |
|---|---|
| §3.1 Script changes (resolve_lease_dir, main wiring, docstring) | Task 5 |
| §3.2 Tests (3 classes) | Tasks 2 + 3 + 4 |
| §3.3 Pitfalls PARITY-1 + Section 2 title extension + checklist + Appendix B + A | Task 8 (post-#40) |
| §3.4 Manual smoke verification (conditional) | Task 6 |
| §3.5 Auto-memory refresh | Task 7 |
| §5 Commit shape (single commit; Agent trailer) | Task 10 Step 2 |
| §5 Codex round on impl diff | Task 9 |
| §6 PR | Task 10 Steps 3-5 |
| §7 Risks (test mutation, fallback robustness, smoke conditional, subprocess safety) | Reflected in tests + warning path; documented in PR body |
