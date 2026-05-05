# 5. Rigorous SemVer via release-please

Date: 2026-05-05
Status: Accepted
Deciders: cameronzucker, alder

## Context

Tuxlink is being developed with an explicit "real deal" / portfolio-grade posture. A predictable failure mode for projects in this class is sloppy versioning: hand-edited `version` strings, ad-hoc tag conventions, missing changelog entries, no clear "what counts as breaking" rule, and reactive scrambles when a user reports "did this break in v0.5 or v0.6?"

The sister Geographica project demonstrated the cost of front-loading versioning discipline: from v1.0.0 onward, every release ships with an automatically-generated `CHANGELOG.md`, a clear MAJOR / MINOR / PATCH rule, a hotfix recipe, and a single mechanism for cutting releases. Reconstructing the version history *after* the fact (in a separate project) was expensive and error-prone.

The mechanism that makes this discipline durable is [release-please](https://github.com/googleapis/release-please) — a Google-maintained automation that:

1. Watches conventional-commits-formatted commits on the release branch.
2. Computes the next version from commit history (`feat:` → minor, `fix:` → patch, `feat!:` / `BREAKING CHANGE:` → major).
3. Opens a "Release PR" containing the version bump, the auto-generated CHANGELOG entry, and any version-file updates.
4. On Release-PR merge: tags the commit, creates a GitHub Release, optionally triggers downstream workflows (AppImage build, etc.).

Release-please replaces hand-managed `version =` edits, `git tag` invocations, and CHANGELOG-by-hand work with a single declarative loop driven by commit messages.

## Decision

Tuxlink adopts release-please as the **only** mechanism for cutting releases. Specifically:

- `.github/workflows/release-please.yml` runs on every push to `main`.
- `.github/release-please-config.json` configures the package as `release-type: simple`, `include-v-in-tag: true`, `bump-minor-pre-major: true` (pre-1.0 breaking changes bump minor, not major).
- All commits on `main` follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org). The contributor scope table is in [CONTRIBUTING.md](../../CONTRIBUTING.md).
- The contract surface for "what counts as breaking" is documented in [VERSIONING.md](../../VERSIONING.md): config schema, IPC contract with Pat, runtime path layout, native menu structure, CLI flags, bundled-Pat compatibility.
- Hand-cut releases (manual `git tag` + manual GitHub Release) are forbidden. If a release is needed and no Release PR exists, the commits since the last release didn't include `feat:` / `fix:` / `perf:` — that's intentional and the answer is "merge the Release PR after a qualifying commit lands."

The hotfix recipe in VERSIONING.md handles the case where a critical fix is needed against a non-latest release.

## Consequences

**Positive:**
- Version numbers are derived, not chosen. Every commit declares its impact via Conventional Commits type; release-please does the math. No human bias on bump granularity.
- CHANGELOG writes itself from commit messages — no "did we forget to update the changelog" failures.
- Release artifacts (tag, GitHub Release, AppImage build trigger) are all produced from one Release-PR merge, so the release process is reproducible and auditable.
- Reverting a release is a known recipe (revert the merge, push, release-please re-opens a corrected Release PR).

**Negative:**
- Conventional Commits discipline is now load-bearing. A wrong commit type silently produces a wrong version bump. Mitigated by commitlint in CI (Task 19) and the `Agent: <moniker>` trailer enforcement, which forces commit-message hygiene.
- release-please's pre-1.0 behavior is non-obvious. The chosen flags (`bump-minor-pre-major: true`) keep the project at `0.x` until an explicit `1.0.0` ship — but a contributor unfamiliar with the flags might expect different math. Documented in VERSIONING.md.
- Adding release-please to tuxlink before v0.0.1 ships means the first Release PR will include the v0.0.1 ship work itself; merging that PR is what produces `v0.0.1`.

## Alternatives considered

- **Hand-managed versioning** (manual `version =` edits, manual `git tag`, manual CHANGELOG entries): rejected. Geographica explicitly demonstrated the value of automation here; reverting to hand-management would forfeit a known-good practice.
- **`conventional-changelog` + manual tag**: rejected. Generates the CHANGELOG but doesn't open a Release PR or compute version bumps. release-please does both.
- **Cargo's own versioning conventions** (Cargo doesn't have a Release-PR equivalent): rejected. release-please can update `Cargo.toml` via the manifest mode if needed; we'll add that wiring in Task 19 when CI is built.
- **Defer release-please until v0.1.** Rejected. The cost of standing it up now is small (~30 minutes); the value (CHANGELOG starting from commit 1, no reconstruction work) is large.
- **Semantic-Release** (Node-ecosystem alternative): rejected. release-please is closer to GitHub-native, has cleaner Release-PR UX, and is what the sister project uses.
