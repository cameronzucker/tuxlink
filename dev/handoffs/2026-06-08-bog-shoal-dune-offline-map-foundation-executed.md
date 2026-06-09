# 2026-06-08/09 bog-shoal-dune — offline-map foundation SHIPPED + release-flake fixed + smoke triage

## One-sentence frame

Executed the offline-first map foundation plan end-to-end (#481, merged to main →
release 0.38.0 pending in #483), investigated the #483 release-PR failure (a
pre-existing flaky modem test, fixed in #486), and operator-smoked the map: it
renders + is reachable on both surfaces (GRIB works; Position form usable-but-poor),
with the UX gaps filed as follow-ups.

## What completed this session

1. **Executed tuxlink-z9u4 (offline-first map foundation), all 10 plan tasks** →
   **PR #481, MERGED** (merge commit `8954790`). Bundled public-domain world raster
   + `BaseMap` (EPSG4326 `<ImageOverlay>`, served from `'self'`) + pure math
   (`projection`/`gridGeometry`/`gribRegion`) + `MaidenheadOverlay` + `GridMapPicker`
   (pin+box) + canonical react-leaflet test mock; GRIB region-by-map (item 21 /
   `mxmx`); compose OSM remediation (`714t`: deleted the online apparatus, reverted
   CSP to offline-only); locked-spec grounding correction.
   - Gates: vitest 1664/1664, typecheck, `pnpm build`, clippy all green.
   - **Codex adversarial round** found 4 P2 defects, all fixed in `c75c81f`
     (degenerate-region clamp, canonical-zero hemisphere, off-map drag abort,
     `maxZoom` 4→2 raster-native).
   - **bd CLOSED on merge:** `tuxlink-z9u4`, `tuxlink-mxmx`, `tuxlink-714t`.
2. **Investigated #483 (release-please PR `chore: release 0.38.0`) verify failure.**
   Root cause: a PRE-EXISTING flaky concurrency test
   `modem_commands::tests::connect_rejects_concurrent_call_when_already_in_progress`
   that synced with `std::thread::sleep(50ms)`; under CI load the worker wasn't
   scheduled in time → the busy guard wasn't set → the second-connect rejection
   assertion panicked. NOT from #481 or #482 (neither touches `modem_commands.rs`;
   proven by merge `--stat` + the same code passing on re-run). Filed `tuxlink-753p`.
3. **Fixed the flake → PR #486 (open).** Replaced the sleep with a deterministic
   channel handshake (worker signals from inside the factory, which the gate invokes
   only after `try_begin_connect()` sets the guard at `modem_commands.rs:143`).
   Verified: target test 30/30 in a loop, full `modem_commands` module 46/46, clippy
   clean. Test-only; no modem/transmit behavior changed. Closes `tuxlink-753p` on merge.
4. **Operator smoke of the #481 first pass (2026-06-09):**
   - **GRIB region map** (Message → GRIB File Request… → Region): renders + works
     (box-drag → region fields populate). Controls "not amazing" → polish needed.
   - **Position-report form map** (Message → New Message → "GPS Position Report"):
     renders + reachable, but "much worse overall… not really usable for a variety of
     reasons." Confirms the gate is auto-satisfied by the PositionArbiter
     (`active_grid()` → persisted `manual_grid` fallback), so the map shows without
     manual entry on a configured station.
   - Net: the feature is wired + reachable (NOT a backend-only ship); the gap is UX
     quality, which the operator chose to iterate on rather than block the ship.

## Branch / worktree state (READ before disposing anything)

- **`bd-tuxlink-z9u4/offline-map-foundation`** — MERGED (#481). Branch dead (ADR 0017).
  Worktree `worktrees/bd-tuxlink-z9u4-offline-map-foundation/` is now **disposable**
  via the ADR 0009 ritual (reclaims `node_modules` + warm `src-tauri/target/`). Holds
  gitignored Codex transcript `dev/adversarial/2026-06-08-offline-map-foundation-impl-codex.md`
  (local-only). Nothing un-propagated — all work is on main.
- **`bd-tuxlink-753p/deflake-concurrent-connect-test`** — PR #486 OPEN. Worktree
  `worktrees/bd-tuxlink-753p-deflake-concurrent-connect-test/` — **keep until #486
  merges** (PR iteration). Warm `node_modules` + built `target/`; clean working tree.
- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`: `.beads/issues.jsonl` dirty
  (bd auto-manages → Dolt; do not hand-commit). Handoff commits are blocked — see
  Loose ends.

## What is NOT done (next session / operator)

1. **Merge the two open PRs:**
   - **#483** (release 0.38.0) — merge once `verify` is green; re-run if the modem
     flake rolls again before #486 lands.
   - **#486** (de-flake) — merge when checks pass; ends the flake permanently.
2. **Map-picker UX follow-up** (the operator's stated next work):
   - `tuxlink-sdbd` (P2) — Position-report form map rework (not usable).
   - `tuxlink-a1cc` (P3) — GRIB region map control/interaction polish.
   - **START with an operator walkthrough to enumerate the concrete problems, THEN a
     brainstorm/design pass (visual companion) BEFORE coding** — per the project's
     UI-design-first discipline. Specifics were intentionally deferred from this
     session; capture them at follow-up start while the operator is engaged.
3. **Deferred z9u4 follow-ups** (open): `tuxlink-dyop` (Rust tile-gatekeeper + opt-in
   permitted tile server, finer precision); `tuxlink-urbv` (item 18, map-pin into
   Settings/wizard — blocked on in-progress `tuxlink-9xy1`). Both unblock now z9u4 merged.
4. **Dispose** the merged `bd-tuxlink-z9u4` worktree (ADR 0009 ritual).

## Gates respected / loose ends

- No RF/transmit path touched anywhere; RADIO-1 did not gate. CSP stays offline-only.
- The de-flake is test-only (operator green-lit treating it as non-RF; "we'll smoke
  when we can").
- **Unpushed/uncommitted on the main checkout** (the block-main-checkout-race hook
  denies main-checkout history ops while other sessions are live — authoritative per
  ADR 0008, not an end-run candidate): `50738b9` (moss-basalt-hawk handoff) + this
  handoff. They commit/push when the main checkout frees (a session ends) or the
  operator does it. All substantive work is safe on origin (#481 merged, #486 pushed).
- Two older untracked handoffs remain in the main checkout from prior sessions
  (`bison-lupine-sycamore`, `osprey-mink-magpie`) — out of this session's scope.
