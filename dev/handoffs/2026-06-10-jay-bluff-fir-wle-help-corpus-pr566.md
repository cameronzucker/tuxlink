# Handoff - 2026-06-10 jay-bluff-fir - WLE help corpus audit

## Summary

Created PR #566 to mine the legacy Winlink Express CHM help as a Tuxlink docs
gap source without copying WLE prose. The PR adds a tracked audit document and
links it from the design docs index.

## Branch and PR

- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-tdjs-wle-help-corpus`
- Branch: `bd-tuxlink-tdjs/wle-help-corpus`
- PR: https://github.com/cameronzucker/tuxlink/pull/566
- Base: `main`
- Latest commit before this handoff: `e0c31d3 docs: audit Winlink Express help corpus`

## Files changed

- `docs/design/2026-06-10-winlink-express-help-gap-audit.md`
  - Source handling, extraction commands, source hashes.
  - CHM TOC coverage matrix mapped to Tuxlink docs status.
  - Follow-up issue IDs for docs work.
- `docs/design/README.md`
  - Adds the audit to the WLE parity index.
- This handoff file.

## Local reference corpus

The raw/extracted WLE help is intentionally gitignored under:

- `dev/winlink-reference/source/`
- `dev/winlink-reference/express-chm/`

Extraction used `/usr/bin/archmage`. The tracked audit records the source
hashes. Do not commit raw CHM contents, screenshots, extracted HTML, or copied
help prose.

## Follow-up beads filed

- `tuxlink-ng91` - docs: contacts/address book and group-address coverage.
- `tuxlink-v3aq` - docs: message-management and operator-admin coverage.
- `tuxlink-1doe` - docs: connection walkthrough depth.

Parent bead: `tuxlink-tdjs`.

Note: the shared Beads JSONL state is currently staged in the root checkout
with other unrelated tracker changes. The new beads are visible via `bd show`,
but PR #566 intentionally keeps its tracked diff to docs/handoff files.

## Validation

- `git diff --cached --check`
- Pre-push hook: `pnpm lint:docs` / `scripts/lint-docs-links.ts`
- PR #566 CI was in progress when this handoff was written:
  - `verify (ubuntu-latest, amd64)`
  - `verify (ubuntu-24.04-arm, arm64)`

## Checkout state

Worktree status before adding this handoff was clean against
`origin/bd-tuxlink-tdjs/wle-help-corpus`.

Root checkout remains on `bd-tuxlink-xygm/recover-handoffs` with pre-existing
dirty state:

- staged/modified `.beads/issues.jsonl`
- untracked `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`

Do not clean or rewrite that root state without operator direction.

## Next action

Watch PR #566 CI. If it passes, merge when appropriate. If it fails, the likely
surface is docs/link validation or a repo-wide CI gate unrelated to the
docs-only audit.
