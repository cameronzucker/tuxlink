# Versioning Policy

Tuxlink is a Linux-native desktop application distributed as an AppImage. We follow [Semantic Versioning](https://semver.org) (`MAJOR.MINOR.PATCH`) with the project-specific adaptations below.

## What the version number promises

Tuxlink's SemVer applies to surfaces a user interacts with *without editing repo files*:

- The configuration file format at `$XDG_CONFIG_HOME/tuxlink/config.json` (`schema_version` field, required keys).
- The runtime path layout at `$XDG_RUNTIME_DIR/tuxlink/*` (PID files, sockets, log files).
- The IPC contract with the bundled Pat process (HTTP endpoints, port allocation, signal handling).
- The native OS menu structure (menu items, accelerators) — user automation may key on these.
- The system tray API (icon, click-to-show, context menu items).
- The CLI flags accepted by the AppImage launcher (`--config`, `--no-tray`, future expansions).
- The bundled-Pat version range (which Pat versions Tuxlink is compatible with).

A release that breaks any of the above requires a **MAJOR** bump. A release that only touches internal Rust modules, React component structure, or test fixtures is **MINOR / PATCH**.

**The rule in one line:** *If a user with a working install has to do anything beyond download-the-new-AppImage to upgrade, that's a MAJOR.*

## MAJOR / MINOR / PATCH rules

| Level | Trigger | Conventional Commits marker |
|---|---|---|
| **MAJOR** (X → X+1.0.0) | Any change to the contract surface above. Config format change; PID-file path change; bundled-Pat compatibility break; menu accelerator removal. | `feat!:` / `fix!:` or `BREAKING CHANGE:` footer |
| **MINOR** (X.Y → X.Y+1.0) | New user-visible feature. New menu item. New CLI flag. New supported Pat version. Non-breaking behavior changes. | `feat:` |
| **PATCH** (X.Y.Z → X.Y.Z+1) | Bug fix. Performance improvement. Dependency bump with no behavior change. Internal refactor. | `fix:`, `perf:`, `refactor:` |
| *(no bump)* | Docs-only change. Test-only change. CI / tooling change. Chore. | `docs:`, `test:`, `ci:`, `chore:`, `build:` |

## Pre-1.0 behavior

Tuxlink is currently in the `0.x` series. Per `release-please-config.json`:

- `bump-minor-pre-major: true` — `feat!:` bumps minor (`0.0.X → 0.1.0`) instead of jumping to `1.0.0`.
- `bump-patch-for-minor-pre-major: false` — `feat:` still bumps minor. (Default.)

This keeps the `0.x` trajectory smooth: features land as minor bumps, breakings land as minor bumps, fixes as patches. The first `1.0.0` ship is the explicit "full Winlink Express parity" milestone (see project roadmap in [README.md](README.md)).

## Branch model

`main` is the release ledger. All tagged versions (`v0.0.1`, `v0.1.0`, `v1.0.0`) are commits on `main`.

The integration branch for in-progress release work is `feat/v0.0.1` (and successor `feat/v0.1.0`, etc.). Per-task branches fork from the integration branch and merge back as **merge-commits with no fast-forward** (squash-merge is banned per [ADR 0010](docs/adr/0010-no-squash-merge.md)). See [CONTRIBUTING.md §Branch model](CONTRIBUTING.md#branch-model).

Release branches are escape hatches. A `release/X.Y` branch is created lazily — only when a critical bug is reported against a released version and the affected user cannot safely upgrade to the latest. The branch is forked from the tag, the fix is applied and tagged, and the fix is cherry-picked back to `main`.

**The default answer to bug reports against older versions is "upgrade to latest and retest."** Hotfix branches exist for the case where that answer is unacceptable.

## Hotfix recipe

```bash
# Step 1: branch from the release tag
git switch -c release/0.1 v0.1.0

# Step 2: apply the fix (or cherry-pick from main if already fixed there)
git cherry-pick <sha>

# Step 3: tag the patch release on the branch
git tag v0.1.1

# Step 4: push branch + tag; release-please picks it up and cuts a release
git push origin release/0.1 v0.1.1

# Step 5: ensure the fix also exists on main (cherry-pick if the release
#         branch was built from v0.1.0 before the fix landed on main)
git switch main
git cherry-pick <sha>  # if needed
```

## Tag format

Tags use the `v` prefix: `v0.0.1`, `v0.1.0`, `v1.2.3`. This matches GitHub release conventions and the `release-please` default with `include-v-in-tag: true`.

## Release cadence

No fixed schedule. Releases ship when there are meaningful user-visible changes (`feat:`, `fix:`, or `perf:` commits) on `main`. In practice `release-please` opens a Release PR within minutes of the first qualifying commit; the maintainer merges it when ready.

**Releases are never cut by hand.** Merging the `release-please`-authored Release PR is the only release mechanism. If a release is needed and no Release PR exists, the prior commits did not include a `feat:` / `fix:` / `perf:` — that's intentional.

## Pre-release markers

Reserved. If needed for a beta or release-candidate, the `-rc.N` suffix follows SemVer 2.0.0 (e.g., `v1.0.0-rc.1`). Not in use for the `0.0.x` series.

## Change history

See [CHANGELOG.md](CHANGELOG.md) for the user-visible change list per release. See [UPGRADING.md](UPGRADING.md) for upgrade instructions on MAJOR releases. See [docs/adr/](docs/adr/) for the reasoning behind significant architectural decisions.
