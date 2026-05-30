#!/usr/bin/env python3
"""new_tuxlink_worktree.py — one-command ADR-0008-compliant worktree creation.

Creates a worktree at worktrees/<bd-id-or-slug>/ off the specified base branch
(default: main), creates a per-task branch inside, and optionally claims
a bd issue + records the worktree path in the issue body via bd remember.

This is the friction-reducer for the worktree-mandatory rule (ADR 0008).
Without this, every worktree creation is a multi-step manual flow; with this,
one command satisfies the ownership + path + branch conventions.

Usage:
  # Standard form — bd issue REQUIRED per ADR 0008 §2 (every worktree binds to
  # a bd issue, no exceptions for long-lived worktrees). If you don't have an
  # issue yet, create one first with `bd create ...`.
  .claude/scripts/new_tuxlink_worktree.py --slug har-saml-fix --issue tuxlink-wkz

  # With session moniker recorded in the bd note for forensics:
  .claude/scripts/new_tuxlink_worktree.py --slug quick-fix --issue tuxlink-wkz --moniker cedar

  # Custom base branch (e.g. stacked PR on a sibling branch still in review):
  .claude/scripts/new_tuxlink_worktree.py --slug logs --issue tuxlink-eil --base bd-other-id/parent-feature

Ported from support-tools/.claude/scripts/New-LfstWorktree.ps1 per Decision 3
of the 2026-05-17 LFST→tuxlink port catalog (Python for cross-platform reuse).
"""

import argparse
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


SLUG_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")


def resolve_repo() -> Path:
    """Resolve repo root from CLAUDE_PROJECT_DIR env or script-relative fallback."""
    env_repo = os.environ.get("CLAUDE_PROJECT_DIR")
    if env_repo and Path(env_repo).is_dir():
        return Path(env_repo).resolve()
    script_dir = Path(__file__).resolve().parent
    return (script_dir / ".." / "..").resolve()


def run(cmd: list[str], cwd: Path, check: bool = True) -> subprocess.CompletedProcess:
    """Run a subprocess; raise on non-zero exit unless check=False."""
    result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if check and result.returncode != 0:
        sys.stderr.write(f"\n{' '.join(cmd)} failed (exit {result.returncode}):\n")
        sys.stderr.write(result.stderr)
        sys.exit(result.returncode)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--slug", required=True, help="Short slug (lowercase, alphanumeric + dashes)")
    parser.add_argument(
        "--issue",
        required=True,
        help=(
            "bd issue ID this worktree binds to. REQUIRED per ADR 0008 §2 — "
            "every worktree must be claimed by a bd issue (no orphan worktrees). "
            "If you don't have an issue, create one first with `bd create ...`."
        ),
    )
    parser.add_argument("--base", default="main", help="Base branch (default: main)")
    parser.add_argument("--moniker", help="Session moniker — recorded in the bd note for forensics")
    args = parser.parse_args()

    if not SLUG_RE.match(args.slug):
        sys.stderr.write(
            f"Invalid slug '{args.slug}'. Must match ^[a-z0-9][a-z0-9-]*$ "
            "(lowercase, alphanumeric + dashes, no leading dash).\n"
        )
        return 2

    repo = resolve_repo()
    if not (repo / ".git").exists():
        sys.stderr.write(f"Not a git repo: {repo}\n")
        return 2

    # --issue is now required (per ADR 0008 §2 + codex D4-P2 review). Branch
    # convention: bd-<id>/<slug> per ADR 0004. Worktree path: worktrees/<name>/
    # per ADR 0008 §2.3 (worktrees/ is gitignored at project root).
    worktree_name = f"bd-{args.issue}-{args.slug}"
    branch_name = f"bd-{args.issue}/{args.slug}"

    worktree_path = repo / "worktrees" / worktree_name
    if worktree_path.exists():
        sys.stderr.write(
            f"Path already exists: {worktree_path}\n"
            f"Either reuse it (cd into it) or pick a different slug.\n"
        )
        return 2

    print(f"Fetching origin...")
    run(["git", "fetch", "origin"], cwd=repo)

    print(f"Creating worktree at {worktree_path} on branch '{branch_name}' off origin/{args.base}...")
    run(
        ["git", "worktree", "add", str(worktree_path), "-b", branch_name, f"origin/{args.base}"],
        cwd=repo,
    )

    # Claim the bd issue + record the worktree path. Per codex D4-P2 review:
    # bd remember accepts only one positional argument (the insight string), so
    # `bd remember <issue> <note>` fails. Use `bd update <id> --append-notes
    # <note>` instead, which appends to the issue's notes field with newline
    # separation (preserves any existing notes).
    print(f"Claiming bd issue {args.issue}...")
    bd_status = ""
    claim = run(["bd", "update", args.issue, "--claim"], cwd=repo, check=False)
    if claim.returncode != 0:
        bd_status = (
            f"⚠ bd update --claim returned exit {claim.returncode}; worktree was created "
            f"but the bd issue is NOT claimed.\n  Run manually: bd update {args.issue} --claim"
        )
    else:
        now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
        note = f"Worktree path: {worktree_path}. Branch: {branch_name}. Created {now}"
        if args.moniker:
            note += f" by {args.moniker}."
        else:
            note += "."
        notes_result = run(["bd", "update", args.issue, "--append-notes", note], cwd=repo, check=False)
        if notes_result.returncode != 0:
            bd_status = (
                f"⚠ bd update --append-notes returned exit {notes_result.returncode}; worktree path NOT "
                f"recorded in issue notes.\n  Run manually: bd update {args.issue} --append-notes '{note}'"
            )

    print()
    print("=== Worktree created ===")
    print(f"Path:     {worktree_path}")
    print(f"Branch:   {branch_name} (off origin/{args.base})")
    print(f"bd issue: {args.issue} (claimed)")
    if bd_status:
        print()
        print(bd_status)
    print()
    print("Next steps:")
    print(f'  1. cd "{worktree_path}"')
    print(f"  2. Do your work; commits land on '{branch_name}'")
    print(f"  3. git push -u origin {branch_name}")
    print(f"  4. gh pr create --base {args.base} --head {branch_name} --title '...' --body '...'")
    print(f"  5. After review: gh pr merge <#> --merge --delete-branch (per 2026-05-17 port catalog Decision 1)")
    print()
    print("Disposal when work is merged (per ADR 0009 ritual — git worktree remove is hook-banned):")
    print(f'  cd "{worktree_path}"')
    print("  git status --short                                            # tracked dirty")
    print("  git ls-files --others --exclude-standard                      # untracked")
    print("  git ls-files --others --ignored --exclude-standard            # gitignored on disk (critical: .beads/embeddeddolt/ class)")
    print("  git stash list                                                # worktree-scoped stashes")
    print(f'  cd "{repo}"                                                  # CRITICAL: cd back BEFORE archiving — relative paths in the doomed worktree get deleted by rm -rf below')
    print("  # if any at-risk content: commit + push to a topic branch OR archive:")
    print(f'  #   tar czf "{repo}/.claude/worktree-archives/{worktree_name}-$(date -u +%Y%m%dT%H%M%SZ).tar.gz" "{worktree_path}"')
    print(f'  rm -rf "{worktree_path}"')
    print(f"  git worktree prune")

    return 0


if __name__ == "__main__":
    sys.exit(main())
