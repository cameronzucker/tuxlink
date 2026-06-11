# 2026-06-10 knoll-bluff-alder README screenshots

## Summary

Refreshed the README app screenshots from the running frontend and opened PR
#580.

## Branch and PR

- Worktree: `worktrees/bd-tuxlink-thzd-readme-live-screenshots`
- Branch: `bd-tuxlink-thzd/readme-live-screenshots`
- Commit: `8bb2202 docs(readme): refresh app screenshots`
- PR: https://github.com/cameronzucker/tuxlink/pull/580
- Bead: `tuxlink-thzd` (README: refresh screenshots from running app)

## What changed

- Replaced the README hero mockup with
  `docs/readme/images/tuxlink-mailbox.png`, generated from the current app
  shell with privacy-safe sample data.
- Replaced the first-run wizard mockup with
  `docs/readme/images/tuxlink-first-run-wizard.png`.
- Replaced the stale/broken third screenshot target with
  `docs/readme/images/tuxlink-request-center.png`.
- Added `dev/readme-screenshot-harness/` so future README screenshots can be
  regenerated from real React components in WebKitGTK using canned Tauri IPC
  responses.

## Validation

- `pnpm lint:docs` passed.
- `pnpm typecheck` passed.
- `pnpm build` passed. Existing Vite dynamic-import/chunk-size warnings were
  emitted.
- README image path existence scan passed.
- PNG nonblank pixel sanity check passed for all three new images.
- Pre-push hook ran `pnpm lint:docs` and passed.

## CI

As of 2026-06-10 19:40 MST, GitHub CI passed for PR #580:

- `verify (ubuntu-latest, amd64)`: passed in 7m46s
- `verify (ubuntu-24.04-arm, arm64)`: passed in 10m27s

Recheck with:

```bash
gh pr checks 580 --watch=false
```

## Working Tree State

- Screenshot worktree is clean and pushed:
  `worktrees/bd-tuxlink-thzd-readme-live-screenshots`.
- Local dev server on port 1420 was stopped; no listener remains.
- Root checkout remains on `bd-tuxlink-xygm/recover-handoffs` with pre-existing
  dirty `.beads` / handoff / design-doc state. Do not clean casually.

## Next Suggested Action

PR #580 is ready for review/merge. Start in the screenshot worktree above if
review feedback requires changes.
