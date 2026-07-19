# Handoff — Contacts restoration SHIPPED (PR #1151); four-issue cluster closed

- **Agent:** owl-moraine-sycamore (same session as the earlier 2026-07-18 handoff)
- **Merged:** PR #1151, 2026-07-18T09:21Z (merge f6cd7052)
- **Closed:** tuxlink-6vn4x (umbrella), tuxlink-pw5nk, tuxlink-f0th0, tuxlink-c6m7 (close-with-evidence)

## What shipped

Canonical design was the operator-provided mock
`dev/scratch/qa-r3-renders/contacts-unified-v1.png` (main repo, gitignored).
The real gap was provenance, not missing fields: the Rust model already had
mode+frequency on `channels[]`, but `merge_for_upsert` discarded anything the
UI sent. Now:

- `Channel.source: observed | manual | serde(other) unknown`; old
  contacts.json loads unchanged. UI owns the manual set (add/edit/remove);
  observations are unforgeable, un-clobberable, never cap-evicted from under
  a manual dial. Provenance flows through `PeerChannelDto.source`.
- Editor gains RADIO DIALS (transport + MHz); detail fuses dials with
  observed stats (honest verbs, "no attempts yet" for virgin dials, distinct
  same-freq observed variants keep their rows); GROUPS chips + add-to-group
  on the detail; editor renders in the detail pane (roster never unmounts);
  orphaned GroupEditor deleted.
- Process: two parallel subagents (Rust/frontend), parent-integrated; Codex
  adversarial round (GPT-5.5) → 3 accepted P2s fixed with regression tests
  (transcript: `dev/adversarial/2026-07-18-contacts-restoration-codex.md`,
  main repo, local-only). Frontend 363 files / 4562 tests green in-worktree;
  Rust compiled+tested in CI both arches.

## Known-flake note (pre-existing, NOT this PR)

`winlink_backend::native_read_state_tests::packet_answer_p2p_intent_records_incoming_accepted_observation`
failed the first arm64 verify run and passed on rerun — bd already tracks it
as timing-dependent (~2/3 failure on loaded runners, fails on docs-only
commits). It cost one CI round trip here; a deflake session should pick up
the existing bd issue.

## Renders (main repo, gitignored)

`dev/scratch/6vn4x-contacts-after.png` (roster vs mock),
`6vn4x-contact-detail.png` (click→detail: fused dials + groups chips — the
tuxlink-c6m7 closing evidence), `6vn4x-editor-dials.png` (editor in pane,
roster visible).

## Worktree state

None from this arc — `worktrees/bd-tuxlink-6vn4x-contacts-restoration`
disposed per ADR 0009 after merge (clean inventory; renders + adrev
transcript copied to the main repo first). This handoff's own ephemeral
worktree is disposed after its direct-to-main push. Another live session
holds the main checkout (ribbon-fixes work) — the race hook is doing its job.
