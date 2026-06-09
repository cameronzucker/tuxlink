# 2026-06-08 osprey-mink-magpie — Contacts+Favorites COMPLETE → PR #472 (READY, awaiting operator smoke + merge)

> **UPDATE (same session):** PR #472 hit merge conflicts after #464/#465/#468/#470 landed on
> main. RESOLVED this session: merged origin/main into the branch (`5439b19`), kept BOTH
> features in the 3 conflicting files (MessageView, FolderSidebar, AppShell), completed the
> FZ-M1 compact coordination (Contacts now in the rail + flyout, with a test), and re-ran the
> FULL gate on the MERGED tree — cargo test (exit 0), clippy --all-targets -D warnings (exit 0),
> tsc clean, full vitest **160 files / 1818 tests** pass. Pushed; **PR #472 is now MERGEABLE /
> CLEAN**. The "resolve #464/#465 conflicts" item below is DONE. Remaining: operator browser
> smoke → merge. Follow-up noted: #465 shipped its own `src/catalog/distance.ts` instead of the
> shared `src/forms/position/distance.ts` haversine — a de-dup cleanup (not a blocker).

## Summary

Finished the Favorites frontend (B3–B7) + wrap-up (C1–C3) + opened the PR. The entire Contacts+Favorites feature (tuxlink-raez + tuxlink-egmp) is now code-complete and green across every gate. **PR #472 is OPEN and READY (not draft).** The only remaining step is the operator's browser smoke, then merge. Executed via subagent-driven-development with a two-stage (spec + quality) review per task; the cross-provider Codex code adrev (C2) ran for real and its findings are fixed.

## Branch / state

- **Worktree:** `worktrees/bd-tuxlink-raez-contacts-favorites` on `bd-tuxlink-raez/contacts-favorites`. **All work pushed; up to date with origin.** Working tree clean except gitignored scratch (`dev/scratch/pr-body.md`, `dev/scratch/codex-c2-prompt.txt`, `dev/adversarial/2026-06-08-contacts-favorites-codex.md` — all gitignored, local-only).
- **PR:** [#472](https://github.com/cameronzucker/tuxlink/pull/472) — base `main`, head `bd-tuxlink-raez/contacts-favorites`. READY. CI running. **Do NOT self-merge — operator browser-smokes first.**
- **bd:** `tuxlink-raez` + `tuxlink-egmp` remain `in_progress` (close them after the PR merges, not before). New: `tuxlink-fkxb` (packet relay-chain favorites, additive forward gap, depends on egmp). `bd remember` key `favorites-relay-chain-gap` stored.

## This session's commits (14, all on the branch, all pushed)

`90374f6` B3 useFavorites · `739582b` B4 distance · `7cb8095` B5a favorite_tod_hint · `b9862a6` B5b favorites UI · `52057cd` B5b polish · `daf897b` Tauri arg-key fix · `9dc4ea2` B6-ARDOP · `42efb14` B6-ARDOP polish · `0f5739a` B6-Packet · `9e0988d` B6-Telnet · `03ff18a` B7 App-level test · `0ffd285` A6 deleted-group block · `832a4c5` C2-P1 fresh-contacts-at-send · `d41730b` C2-P2 instant recents ordering.

## Quality gates — ALL GREEN

- `cargo test` full: exit 0. `cargo clippy --all-targets -- -D warnings`: exit 0.
- `pnpm exec tsc --noEmit`: clean. Full `pnpm exec vitest run`: **146 files / 1673 tests passed**, no worker zombies.
- C2 Codex adrev: real 24k-line review. Verdict: RADIO-1 prefill, ToD hinting, distance source, Tauri arg-keys all SAFE. Two findings (P1 stale group expansion, P2 lexical timestamp ordering) fixed + tested.

## Key decisions made this session (validated by review/adrev; flagged for operator)

1. **`favorite_tod_hint` IPC command added (B5a)** — resolved a seam: the design keeps the offset-local ToD gate in Rust (H1/H2) but the backend exposed no IPC for it; the frontend now reaches the existing `tod_hint` via a thin read-only command instead of re-implementing bucketing in JS.
2. **Tauri camelCase arg-key fix (daf897b)** — this codebase auto-maps camelCase JS keys → snake_case Rust params. The favorites invokes initially used snake_case (`ts_local`/`unit_id`), which silently no-op at runtime; the mocked tests hid it. Fixed to `tsLocal`/`unitId`. (Worth a transferable memory: mocked invoke tests can't catch arg-key/Tauri-binding bugs.)
3. **ARDOP reached+failed double-record** — a session that reaches `connected-*` (records reached) then fails the b2f exchange (records failed) logs both. Intentional (distinct empirical facts), documented in source. Operator may want exchange-failure to suppress the earlier reached — a smoke-time call.

## Operator smoke checklist (before merging #472)

- Walk Contacts: list/detail, groups-on-top, suggestions, add-from-sender, GroupEditor; Compose To/Cc autocomplete + group chip; send a group (members expand, no `group:` token on the wire); delete a group mid-compose → send blocks with the error.
- Walk Favorites per mode: ARDOP/Packet/Telnet show Favorites/Recent/Manual tabs; VARA shows Manual-only. A favorite's Connect PRE-FILLS the target (ARDOP) / host+transport (Telnet) and does NOT transmit — the Start click does. Star-to-promote. Distance + the connection-record line render.
- **UX note:** ARDOP defaults to the Favorites tab; with no saved favorites it's empty and you click Manual to type a target. If you'd prefer auto-defaulting to Manual when favorites+recents are empty, that's a one-line FavoritesTabs change — say so and the next session does it.
- Self-smoke evidence was NOT captured (the agent leaves the live browser walk to you per RADIO-1/operator-smoke division). `scripts/converge-build.sh` from the worktree (free :1420 first).

## Merge coordination (#464 fzm1-responsive, #465 catalog-builder still open)

- `src/forms/position/distance.ts` is NEW; #465 imports `haversineKm` from it.
- `src-tauri/src/lib.rs` has contiguous labeled `// contacts` + `// favorites` registration blocks.
- B6 edited the interiors of `Ardop/Packet/TelnetRadioPanel.tsx`; FZ-M1's `RadioPanel` container wrap should compose cleanly. Resolve any overlap at integration.

## Pending (not blocking this PR)

- Operator browser smoke → merge #472 → close raez + egmp.
- `tuxlink-fkxb` packet relay-chain favorites (additive, later).
- Packet Start button has no in-flight disable (pre-existing; rapid clicks could queue dup connects). Minor follow-up if desired.
