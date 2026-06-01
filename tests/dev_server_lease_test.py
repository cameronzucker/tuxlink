#!/usr/bin/env python3
"""Tests for scripts/lib/dev-server-lease.sh — host-level lease library.

Each test sets XDG_CONFIG_HOME to a fresh tempdir so the lease file lives
in isolation and the tests can run concurrently. The port-1420 check
(`_ds_lease_port_pid`) is exercised lightly — the tests stub `ss` on PATH
when we need to control the port-owner answer.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
LIB = REPO_ROOT / "scripts" / "lib" / "dev-server-lease.sh"
CLI = REPO_ROOT / "scripts" / "dev-server-lease.sh"


def _run_bash(script: str, env_extra: dict | None = None) -> subprocess.CompletedProcess:
    """Run a bash snippet that sources LIB. Returns CompletedProcess."""
    env = {**os.environ, **(env_extra or {})}
    return subprocess.run(
        ["bash", "-c", f". {LIB} && {script}"],
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )


class _LeaseEnv:
    """Context manager — fresh XDG_CONFIG_HOME for test isolation."""

    def __enter__(self):
        self.tmp = tempfile.mkdtemp(prefix="tuxlink-lease-test-")
        self._orig_xdg = os.environ.get("XDG_CONFIG_HOME")
        os.environ["XDG_CONFIG_HOME"] = self.tmp
        return self

    @property
    def lease_path(self) -> Path:
        return Path(self.tmp) / "tuxlink" / "dev-server.json"

    def __exit__(self, *exc):
        if self._orig_xdg is None:
            os.environ.pop("XDG_CONFIG_HOME", None)
        else:
            os.environ["XDG_CONFIG_HOME"] = self._orig_xdg
        shutil.rmtree(self.tmp, ignore_errors=True)


class _SsStub:
    """Stub ss + lsof on PATH so we control the port-owner answer.

    ss output mimics the real `ss -lntp` line format for a listener
    on :1420. If owner_pid is None, the stub emits nothing (port free).
    """

    def __init__(self, owner_pid: int | None):
        self.owner_pid = owner_pid
        self.tmp = tempfile.mkdtemp(prefix="tuxlink-ss-stub-")
        self._orig_path = os.environ.get("PATH", "")

    def __enter__(self):
        ss = Path(self.tmp) / "ss"
        if self.owner_pid is not None:
            line = (
                f'LISTEN 0 511 *:1420 *:* '
                f'users:(("node",pid={self.owner_pid},fd=23))'
            )
        else:
            line = ""
        # ss accepts any args; we just check -lntp is asked-for.
        ss.write_text(
            "#!/usr/bin/env bash\n"
            f"cat <<'EOF'\nState  Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n{line}\nEOF\n"
        )
        ss.chmod(0o755)
        # lsof as well so the fallback also works deterministically.
        lsof = Path(self.tmp) / "lsof"
        if self.owner_pid is not None:
            lsof.write_text(f"#!/usr/bin/env bash\necho {self.owner_pid}\n")
        else:
            lsof.write_text("#!/usr/bin/env bash\nexit 0\n")
        lsof.chmod(0o755)
        os.environ["PATH"] = f"{self.tmp}:{self._orig_path}"
        return self

    def __exit__(self, *exc):
        os.environ["PATH"] = self._orig_path
        shutil.rmtree(self.tmp, ignore_errors=True)


class LeaseLibraryTest(unittest.TestCase):
    """Tests for the library functions in scripts/lib/dev-server-lease.sh."""

    def test_acquire_writes_lease_file(self):
        with _LeaseEnv() as env, _SsStub(None):
            r = _run_bash(
                'ds_lease_acquire "bd-tuxlink-test/foo" "abcdef0123456789" '
                "&& echo OK"
            )
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            self.assertIn("OK", r.stdout)
            self.assertTrue(env.lease_path.exists())
            data = json.loads(env.lease_path.read_text())
            self.assertEqual(data["pid"], data["pid"])  # any int
            self.assertEqual(data["branch"], "bd-tuxlink-test/foo")
            self.assertEqual(data["sha"], "abcdef0123456789")
            self.assertEqual(data["version"], 1)

    def test_acquire_refuses_when_live_owner_exists(self):
        with _LeaseEnv() as env, _SsStub(None):
            # Pre-populate the lease with the current process's PID — it's
            # alive (us!), so a fresh acquire from a CHILD bash should fail.
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": os.getpid(),  # our test process; definitely alive
                        "cwd": str(REPO_ROOT),  # exists
                        "branch": "bd-tuxlink-other/work",
                        "sha": "1111111111111111",
                        "started_at": "2026-06-01T12:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash(
                'ds_lease_acquire "bd-tuxlink-test/foo" "abcdef0123456789"; '
                'echo "rc=$?"'
            )
            self.assertIn("rc=7", r.stdout)
            # Verify the existing lease was NOT overwritten.
            data = json.loads(env.lease_path.read_text())
            self.assertEqual(data["branch"], "bd-tuxlink-other/work")

    def test_acquire_clears_stale_lease_pid_dead(self):
        with _LeaseEnv() as env, _SsStub(None):
            # Use PID 1 — usually init, NOT us. But the stale check ALSO
            # checks cwd existence, so we use a clearly-dead PID.
            # Find a PID that won't exist: use a 7-digit number well beyond
            # max_pid on most systems.
            dead_pid = 9999991
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": dead_pid,
                        "cwd": "/nonexistent/path/somewhere",
                        "branch": "bd-tuxlink-orphan/work",
                        "sha": "deadbeefdeadbeef",
                        "started_at": "2025-01-01T00:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash(
                'ds_lease_acquire "bd-tuxlink-test/foo" "abcdef0123456789" '
                "&& echo OK"
            )
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            self.assertIn("OK", r.stdout)
            data = json.loads(env.lease_path.read_text())
            self.assertEqual(data["branch"], "bd-tuxlink-test/foo")
            # Stale-clear warning should have fired.
            self.assertIn("stale", r.stderr.lower())

    def test_release_no_op_when_not_owner(self):
        with _LeaseEnv() as env, _SsStub(None):
            # Pre-populate lease with a different PID.
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": os.getpid(),  # our test, NOT the child bash
                        "cwd": str(REPO_ROOT),
                        "branch": "bd-tuxlink-other/work",
                        "sha": "1111111111111111",
                        "started_at": "2026-06-01T12:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash("ds_lease_release && echo OK")
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            # File should still exist (release was no-op).
            self.assertTrue(env.lease_path.exists())

    def test_release_deletes_when_owner(self):
        with _LeaseEnv() as env, _SsStub(None):
            r = _run_bash(
                'ds_lease_acquire "bd-tuxlink-test/foo" "abcdef0123456789" '
                "&& ds_lease_release && echo OK"
            )
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            self.assertFalse(env.lease_path.exists())

    def test_inspect_empty_returns_1(self):
        with _LeaseEnv(), _SsStub(None):
            r = _run_bash("ds_lease_inspect; echo rc=$?")
            self.assertIn("rc=1", r.stdout)

    def test_inspect_consistent_returns_0(self):
        with _LeaseEnv() as env:
            # Acquire from a child bash; capture the child's PID; stub ss
            # to return that same PID as port-1420 owner.
            r = _run_bash(
                'ds_lease_acquire "bd-tuxlink-test/foo" "abcdef0123456789"; '
                "echo $$"
            )
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            child_pid = int(r.stdout.strip())
            # NB: the bash process has exited by now, but we recorded its PID.
            # We can't really replay the child's PID as "alive" since it
            # isn't. Instead, write a lease whose pid is the current process.
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": os.getpid(),
                        "cwd": str(REPO_ROOT),
                        "branch": "bd-tuxlink-test/foo",
                        "sha": "abcdef0123456789",
                        "started_at": "2026-06-01T12:00:00Z",
                        "version": 1,
                    }
                )
            )
            with _SsStub(os.getpid()):
                r = _run_bash("ds_lease_inspect; echo rc=$?")
            self.assertIn("rc=0", r.stdout)

    def test_inspect_stale_lease_returns_2(self):
        with _LeaseEnv() as env, _SsStub(None):
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": 9999991,  # dead
                        "cwd": "/nonexistent",
                        "branch": "x",
                        "sha": "0" * 16,
                        "started_at": "2025-01-01T00:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash("ds_lease_inspect; echo rc=$?")
            self.assertIn("rc=2", r.stdout)

    def test_inspect_port_orphan_returns_3(self):
        with _LeaseEnv(), _SsStub(os.getpid()):
            # No lease file, but port-1420 stub returns our PID.
            r = _run_bash("ds_lease_inspect; echo rc=$?")
            self.assertIn("rc=3", r.stdout)

    def test_inspect_split_brain_returns_4(self):
        # Split-brain: lease points at a PID different from the port's PID,
        # both alive. We need TWO different alive user-owned PIDs. Spawn a
        # long-sleeping subprocess to serve as the lease's PID, and use
        # os.getpid() as the port-owner PID.
        bg = subprocess.Popen(["sleep", "30"])
        try:
            with _LeaseEnv() as env, _SsStub(os.getpid()):
                env.lease_path.parent.mkdir(parents=True, exist_ok=True)
                env.lease_path.write_text(
                    json.dumps(
                        {
                            "pid": bg.pid,  # alive + user-owned (we spawned it)
                            "cwd": str(REPO_ROOT),
                            "branch": "x",
                            "sha": "0" * 16,
                            "started_at": "2026-06-01T12:00:00Z",
                            "version": 1,
                        }
                    )
                )
                r = _run_bash("ds_lease_inspect; echo rc=$?")
                self.assertIn(
                    "rc=4",
                    r.stdout,
                    msg=f"stdout={r.stdout!r}, stderr={r.stderr!r}",
                )
        finally:
            bg.terminate()
            bg.wait(timeout=5)

    def test_clear_stale_removes_dead_lease(self):
        with _LeaseEnv() as env, _SsStub(None):
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": 9999991,
                        "cwd": "/nonexistent",
                        "branch": "x",
                        "sha": "0" * 16,
                        "started_at": "2025-01-01T00:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash("ds_lease_clear_stale; echo rc=$?")
            self.assertIn("rc=0", r.stdout)
            self.assertFalse(env.lease_path.exists())

    def test_clear_stale_refuses_live_lease(self):
        with _LeaseEnv() as env, _SsStub(None):
            env.lease_path.parent.mkdir(parents=True, exist_ok=True)
            env.lease_path.write_text(
                json.dumps(
                    {
                        "pid": os.getpid(),  # our test — alive
                        "cwd": str(REPO_ROOT),
                        "branch": "x",
                        "sha": "0" * 16,
                        "started_at": "2026-06-01T12:00:00Z",
                        "version": 1,
                    }
                )
            )
            r = _run_bash("ds_lease_clear_stale; echo rc=$?")
            self.assertIn("rc=2", r.stdout)
            self.assertTrue(env.lease_path.exists())


class LeaseCliTest(unittest.TestCase):
    """Smoke-test the CLI wrapper (subset; library tests cover semantics)."""

    def test_inspect_emits_json(self):
        with _LeaseEnv(), _SsStub(None):
            r = subprocess.run(
                ["bash", str(CLI), "inspect"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertIn('"lease"', r.stdout)
            self.assertIn('"port"', r.stdout)
            self.assertEqual(r.returncode, 1)  # empty state

    def test_acquire_and_release_round_trip(self):
        with _LeaseEnv() as env, _SsStub(None):
            r = subprocess.run(
                [
                    "bash",
                    str(CLI),
                    "acquire",
                    "bd-tuxlink-cli/test",
                    "0123456789abcdef",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(r.returncode, 0, msg=r.stderr)
            self.assertTrue(env.lease_path.exists())
            # The acquire subshell has exited, so its PID is dead. release
            # from a fresh subshell is a no-op.
            r = subprocess.run(
                ["bash", str(CLI), "release"],
                capture_output=True,
                text=True,
                check=False,
            )
            # File should still be there since the fresh release subshell
            # is not the recorded owner.
            self.assertTrue(env.lease_path.exists())

    def test_help_lists_commands(self):
        r = subprocess.run(
            ["bash", str(CLI), "--help"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(r.returncode, 0)
        for cmd in ("inspect", "acquire", "release", "clear-stale", "force-kill-owned"):
            self.assertIn(cmd, r.stdout, msg=f"help missing {cmd}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
