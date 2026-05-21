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

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path


def resolve_repo() -> Path:
    """Resolve repo root from CLAUDE_PROJECT_DIR env or script-relative fallback."""
    env_repo = os.environ.get("CLAUDE_PROJECT_DIR")
    if env_repo and Path(env_repo).is_dir():
        return Path(env_repo).resolve()
    script_dir = Path(__file__).resolve().parent
    return (script_dir / ".." / "..").resolve()


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


def parse_iso_utc(s: str) -> datetime | None:
    """Parse an ISO-8601 timestamp into a UTC-aware datetime. Returns None on failure."""
    if not s:
        return None
    # Handle the trailing Z + fractional seconds shapes our bash hook writes.
    s = s.replace("Z", "+00:00")
    try:
        dt = datetime.fromisoformat(s)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--ttl-minutes", type=int, default=30, help="Liveness window in minutes (default: 30)")
    parser.add_argument("--include-stale", action="store_true", help="Show sessions whose lease is older than TTL")
    args = parser.parse_args()

    repo = resolve_repo()
    lease_dir = resolve_lease_dir(repo)
    if not lease_dir.exists():
        print("No active tuxlink sessions (lease directory does not exist yet).")
        return 0

    now = datetime.now(timezone.utc)
    cutoff = now - timedelta(minutes=args.ttl_minutes)

    sessions = []
    main_holder = None

    for lease_file in sorted(lease_dir.glob("*.json")):
        if lease_file.name == "main-checkout.json":
            try:
                with lease_file.open() as f:
                    ml = json.load(f)
                ml_last = parse_iso_utc(ml.get("lastSeenUtc", ""))
                if ml_last and ml_last > cutoff:
                    sid = ml.get("sessionId", "")
                    main_holder = f"{ml.get('moniker', '(unknown)')} ({sid[:8] if sid else '?'})"
            except (json.JSONDecodeError, OSError):
                pass
            continue

        try:
            with lease_file.open() as f:
                lease = json.load(f)
        except (json.JSONDecodeError, OSError):
            continue

        last_seen = parse_iso_utc(lease.get("lastSeenUtc", ""))
        if not last_seen:
            continue

        age_min = int((now - last_seen).total_seconds() / 60)
        live = last_seen > cutoff
        if not live and not args.include_stale:
            continue

        sid = lease.get("sessionId", "")
        sessions.append({
            "moniker": lease.get("moniker", "(unknown)"),
            "checkout": "main" if lease.get("isMainCheckout", False) else "worktree",
            "branch": lease.get("branch", "?"),
            "last_seen": f"{age_min}m ago",
            "state": "live" if live else "stale",
            "session_id": sid[:8] if sid else "?",
            "_live": live,
        })

    if not sessions:
        print("No live tuxlink sessions in this repo.")
        if main_holder:
            print(f"Main-checkout lease holder: {main_holder}")
        return 0

    sessions.sort(key=lambda s: (not s["_live"], s["moniker"]))

    headers = ["Moniker", "Checkout", "Branch", "Last seen", "State", "Session"]
    rows = [[s["moniker"], s["checkout"], s["branch"], s["last_seen"], s["state"], s["session_id"]] for s in sessions]
    widths = [max(len(h), *(len(str(r[i])) for r in rows)) for i, h in enumerate(headers)]

    print()
    print("Active tuxlink sessions in this repo:")
    print("  " + "  ".join(h.ljust(w) for h, w in zip(headers, widths)))
    print("  " + "  ".join("-" * w for w in widths))
    for r in rows:
        print("  " + "  ".join(str(c).ljust(w) for c, w in zip(r, widths)))
    print()

    if main_holder:
        print(f"Main-checkout lease holder: {main_holder}")
    else:
        print("Main-checkout lease: not held (any session may take it for integration work).")
    print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
