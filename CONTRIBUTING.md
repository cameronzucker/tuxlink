# Contributing to Tuxlink

## Conventional Commits format

All commits on `main` and `feat/v0.0.1` follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org):

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Supported types

| Type | Version impact | Use when |
|---|---|---|
| `feat:` | MINOR (or PATCH pre-1.0) | New user-visible feature |
| `fix:` | PATCH | Bug fix |
| `perf:` | PATCH | Performance improvement, no behavior change |
| `refactor:` | PATCH | Internal restructuring, no user-visible change |
| `docs:` | none | Documentation-only change |
| `test:` | none | Test-only change |
| `build:` | none | Build / dependency change (`Cargo.toml`, `package.json`, AppImage tooling) |
| `ci:` | none | CI / workflow change |
| `chore:` | none | Housekeeping (`.gitignore`, editorconfig, license header sync) |
| `revert:` | inherits | Revert a previous commit |

### Breaking changes

Breaking changes trigger a MAJOR bump (or MINOR pre-1.0; see [VERSIONING.md](VERSIONING.md)):

- Add `!` suffix: `feat!:`, `fix!:`.
- And/or add a `BREAKING CHANGE:` footer with a one-line user-facing explanation.

The `!` and the footer can co-exist; use `!` for quick signaling and the footer when the change requires prose to explain what users must do to upgrade. Footer text flows directly to `CHANGELOG.md` and `UPGRADING.md`.

### Recommended scopes

| Scope | Subsystem |
|---|---|
| `protocol` | Protocol traits, telnet implementation, future VARA/AX.25 backends |
| `wizard` | First-run wizard screens (account check, credentials, test send) |
| `mailbox` | Inbox / Sent / Posted UI |
| `compose` | Compose window, draft persistence |
| `session` | Session log pane, session state |
| `menu` | Native OS menu bar |
| `tray` | System tray, window-close behavior |
| `shell` | Main app shell, status bar, layout |
| `config` | Config file format, XDG path handling |
| `ci` | CI workflows |
| `docs` | Documentation |
| `pitfalls` | Pitfalls docs (`docs/pitfalls/*.md`) |
| `adr` | Architecture decision records (`docs/adr/*`) |

Example: `feat(wizard): add grid-square auto-fill from GPS`

### Subject line

Imperative mood (`add` not `added` / `adds`), ≤72 characters, no trailing period. Body optional; use for non-obvious *why*.

### Mandatory commit trailers

Every commit ends with two trailers, in this order:

```
Agent: <session-moniker>
Co-Authored-By: <model> <email>
```

A PreToolUse hook enforces the `Agent:` trailer; the harness rejects commits missing it. See [CLAUDE.md §Agent identity](CLAUDE.md#agent-identity--pick-a-moniker-at-session-start).

## Branch model

Tuxlink employs a **per-task-branch model** during pre-1.0 development:

1. `main` is the release ledger; tagged versions live there.
2. `feat/v0.0.1` (and successors) is the integration branch for in-progress release work.
3. Each task branches from the integration branch: `task-NN-<slug>` or, with [Beads](https://github.com/steveyegge/beads), `bd-<id>/<slug>`.
4. Task branch → PR against integration branch → review (subagent or human) → **merge-commit (no fast-forward)** → delete task branch. **Squash-merge is banned** per [ADR 0010](docs/adr/0010-no-squash-merge.md); use `gh pr merge <#> --merge --delete-branch`.
5. Integration branch → merge into `main` at the release tag (no-ff per ADR 0010; this may or may not be ff-eligible depending on whether dependabot or similar has landed commits directly on `main` between releases).

A PreToolUse hook rejects direct commits to `main` or `feat/v0.0.1` unless the `ALLOW_INTEGRATION_COMMIT=1` env var is set (carve-out for the merge-commit step). See [CLAUDE.md](CLAUDE.md), [docs/adr/0004-per-task-branch-model.md](docs/adr/0004-per-task-branch-model.md), and [docs/adr/0010-no-squash-merge.md](docs/adr/0010-no-squash-merge.md).

## Local verification

Before pushing a task branch, run the test suite:

```bash
# Rust unit + integration tests
cd src-tauri && cargo test --verbose

# Frontend tests
pnpm vitest run

# Full lint pass
cd src-tauri && cargo clippy --all-targets -- -D warnings
pnpm typecheck

# Browser smoke (UI-touching tasks only; see docs/pitfalls/testing-pitfalls.md)
pnpm tauri dev   # walk the user flow that the change affects
```

## Architecture decisions

ADRs in [docs/adr/](docs/adr/) record substantive architectural choices. PRs that introduce or change an architectural commitment must add an ADR in the same PR. See [docs/adr/README.md](docs/adr/README.md) for the format.

## Live amateur radio operations

Code paths that transmit on real amateur-radio infrastructure are **operator-only**. Automated tests, subagents, CI jobs, and AI agents must NOT initiate transmissions under the project callsign. See [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md) for the consent-gate protocol. This is FCC Part 97 regulatory compliance, not a style rule.

## PR flow

Tuxlink currently has one active maintainer + AI agents. Outside contributions are welcome:

1. Fork the repo.
2. Create a task branch off `feat/v0.0.1` (or the current integration branch).
3. Conventional Commits all the way down, with `Agent:` trailer if your harness has a moniker convention; otherwise use `Agent: <github-username>`.
4. Open PR against the integration branch.
5. The maintainer (or a review subagent) reviews. **Merge-commit (no fast-forward) after approval** per [ADR 0010](docs/adr/0010-no-squash-merge.md). Squash-merge is banned.

The one PR that appears automatically is the [`release-please`](https://github.com/googleapis/release-please) Release PR. See [VERSIONING.md](VERSIONING.md).
