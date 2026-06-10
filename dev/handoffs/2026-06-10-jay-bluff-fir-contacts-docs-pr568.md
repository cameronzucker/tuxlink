# Handoff - 2026-06-10 jay-bluff-fir - Contacts docs

## Summary

Created PR #568 to fix the WLE-help-derived contacts/address-book docs gap.
The PR adds a first-class user-guide topic for Contacts and groups, then links
to it from Compose, Settings, and the migration guide.

## Branch and PR

- Worktree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-ng91-contacts-docs`
- Branch: `bd-tuxlink-ng91/contacts-docs`
- PR: https://github.com/cameronzucker/tuxlink/pull/568
- Base: `main`
- Issue: `tuxlink-ng91`

## Files changed

- `docs/user-guide/34-contacts-and-groups.md`
  - Contacts surface location.
  - Contact fields.
  - Add-from-message flow.
  - Suggested contacts behavior.
  - Groups and send-time expansion.
  - Compose autocomplete behavior.
  - Winlink Express migration limits.
  - `contacts.json` data/corrupt-file behavior.
- `docs/user-guide/19-composing.md`
  - Links recipient autocomplete to the new topic.
- `docs/user-guide/27-settings.md`
  - Clarifies contacts/groups as app-local data and links the new topic.
- `docs/user-guide/32-from-express-or-pat.md`
  - Maps Express Address Book / Group Addresses to Tuxlink Contacts.
  - Notes missing import/export tooling.
- `src/help/topics.ts`
  - Adds `34-contacts-and-groups` to the Using Tuxlink section.
- `src/help/topics.test.ts`
  - Updates expected topic count to 34.
- `src-tauri/src/search/docs_bundle.rs`
  - Adds `34-contacts-and-groups` to the search bundle.
  - Also adds previously-sidebar-visible `33-operating-modes` to the search
    bundle so in-app search and sidebar coverage match.

## Validation

- `pnpm lint:docs`
- `pnpm exec vitest run src/help/topics.test.ts`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- pre-push `pnpm lint:docs`

## Notes

`tuxlink-ng91` remains open in bd until PR #568 merges. The shared `.beads`
JSONL state in the root checkout remains dirty from unrelated tracker work; no
tracker file changes are included in this PR.

Root checkout state at handoff time was still:

- Branch: `bd-tuxlink-xygm/recover-handoffs`
- Dirty: staged/modified `.beads/issues.jsonl`
- Untracked: `dev/handoffs/2026-06-10-arroyo-lichen-grouse-request-center-reskin-shipped-pr559.md`

Do not clean that root state without operator direction.

## Next action

Watch PR #568 CI. If it passes, merge normally. If it fails, the most likely
surfaces are docs link lint, topic registry tests, or Rust compile around the
docs search bundle.
