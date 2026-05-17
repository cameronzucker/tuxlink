#!/usr/bin/env python3
"""get_agent_moniker.py — generate a unique session moniker.

Draws 3 words without replacement from a 100-word pool of plant / animal /
geographic nouns and hyphen-joins them. Combinatorial space ≈ 970,200 trios;
collision probability under 1% across project lifetime.

Pre-flights against `git log --all --grep="^Agent: <candidate>"` (per the
feedback_moniker_collision_pre_flight.md memory entry) — if a collision is
detected, retries up to --max-attempts times before giving up. This replaces
the prior manual grep + git log dance at session start.

Usage:
  .claude/scripts/get_agent_moniker.py
    → prints a fresh moniker like `towhee-wren-aspen`

  .claude/scripts/get_agent_moniker.py --since=90d
    → check collisions only within the last 90 days (default: all-time)

  .claude/scripts/get_agent_moniker.py --max-attempts 20
    → try up to 20 candidates before giving up (default: 10)

Ports the LFST script support-tools/.claude/scripts/Get-AgentMoniker.ps1
per Decision 3 of the 2026-05-17 LFST→tuxlink port catalog (Python for
cross-platform reuse).
"""

import argparse
import os
import random
import re
import subprocess
import sys
from pathlib import Path


# 100-word pool: 33 plants + 33 animals + 34 geographic features.
# Curated for distinctiveness (low ambient-collision risk in code/docs),
# pronounceability, and ctrl-F-friendliness. Lowercase, no punctuation.
POOL = sorted([
    # Plants (33)
    "alder", "basil", "birch", "cedar", "clover", "cypress", "dahlia",
    "fern", "fir", "hemlock", "ivy", "juniper", "larch", "lichen", "lupine",
    "magnolia", "maple", "moss", "oak", "pine", "poplar", "redwood", "sage",
    "sequoia", "sorrel", "spruce", "sumac", "sycamore", "tamarack", "thistle",
    "vetch", "willow", "yew",
    # Animals (33)
    "badger", "beaver", "bison", "cardinal", "condor", "falcon", "finch",
    "fox", "grouse", "harrier", "hawk", "heron", "jay", "kestrel",
    "kingfisher", "kite", "magpie", "marten", "mink", "opossum", "oriole",
    "osprey", "owl", "peregrine", "pika", "plover", "raven", "salamander",
    "sparrow", "swallow", "tanager", "towhee", "wren",
    # Geographic (34)
    "arroyo", "atoll", "basalt", "basin", "bayou", "bluff", "bog", "butte",
    "canyon", "chasm", "cove", "crag", "delta", "dune", "esker", "fen",
    "fjord", "glade", "gorge", "granite", "gulch", "gully", "isthmus",
    "knoll", "marsh", "mesa", "moraine", "ridge", "sandbar", "savanna",
    "shoal", "slate", "taiga", "vale",
])

assert len(POOL) == 100, f"POOL must have exactly 100 words; has {len(POOL)}"
assert len(set(POOL)) == 100, "POOL has duplicates"


def resolve_repo() -> Path:
    """Resolve repo root.

    Prefers CLAUDE_PROJECT_DIR IF it points at a directory that is itself a git
    repo. Otherwise falls back to the script-relative path (..\\..). Per codex
    2026-05-17 B3 review: previously, a misconfigured CLAUDE_PROJECT_DIR (e.g.,
    pointing at `/`) would silently make git log fail, and `moniker_taken`
    would silently treat the failure as "no collision" — letting duplicate
    monikers through. The fallback here makes a non-git CLAUDE_PROJECT_DIR
    behave the same as no env var at all.
    """
    env_repo = os.environ.get("CLAUDE_PROJECT_DIR")
    if env_repo:
        env_path = Path(env_repo)
        if env_path.is_dir() and (env_path / ".git").exists():
            return env_path.resolve()
    script_dir = Path(__file__).resolve().parent
    return (script_dir / ".." / "..").resolve()


def moniker_taken(candidate: str, repo: Path, since: str | None) -> bool | None:
    """Check git log --all --grep for prior use.

    Returns True if a prior commit's `Agent: <candidate>` trailer matches.
    Returns False if no match found. Returns **None** if the git log call
    itself failed (caller must distinguish "no collision" from "check failed"
    — per codex 2026-05-17 B3 review: silently treating check-failed as
    no-collision lets duplicate monikers through).
    """
    cmd = ["git", "log", "--all", f"--grep=^Agent: {re.escape(candidate)}", "--oneline"]
    if since:
        cmd.append(f"--since={since}")
    try:
        result = subprocess.run(cmd, cwd=repo, capture_output=True, text=True)
    except (FileNotFoundError, OSError):
        # git binary not on PATH, or other subprocess setup failure
        return None
    if result.returncode != 0:
        # git ran but failed (e.g., not a git repo, ambiguous ref, etc.)
        return None
    return bool(result.stdout.strip())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--since",
        default=None,
        help="Collision-check window (e.g. '90d', '3.months'). Default: all-time.",
    )
    parser.add_argument(
        "--max-attempts",
        type=int,
        default=10,
        help="Max generation attempts before giving up (default: 10).",
    )
    parser.add_argument(
        "--no-pre-flight",
        action="store_true",
        help="Skip the git-log collision check (emergency / offline use).",
    )
    args = parser.parse_args()

    repo = resolve_repo()
    rng = random.SystemRandom()

    for attempt in range(1, args.max_attempts + 1):
        words = rng.sample(POOL, 3)
        moniker = "-".join(words)
        if args.no_pre_flight:
            print(moniker)
            return 0
        taken = moniker_taken(moniker, repo, args.since)
        if taken is None:
            # Pre-flight check itself failed (git missing, repo path wrong, etc.).
            # Fail closed per codex 2026-05-17 B3 review — refuse to ship an
            # unchecked moniker; force the operator to choose explicitly.
            sys.stderr.write(
                f"Pre-flight check failed at attempt {attempt}: git log returned an error "
                f"against repo={repo}. Either:\n"
                f"  - Set CLAUDE_PROJECT_DIR explicitly to a valid git repo path, OR\n"
                f"  - Verify the script-relative repo path resolves correctly, OR\n"
                f"  - Re-run with --no-pre-flight to accept the risk of a duplicate moniker.\n"
            )
            return 1
        if not taken:
            print(moniker)
            return 0
        sys.stderr.write(f"# attempt {attempt}: {moniker} already used; retrying\n")

    sys.stderr.write(
        f"Failed to generate a collision-free moniker in {args.max_attempts} attempts. "
        f"Either expand --max-attempts or shrink --since window.\n"
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
