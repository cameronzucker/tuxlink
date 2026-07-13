# Handoff — 2026-07-13 (vetch-juniper-badger): Station Intelligence shipped through v0.89.1; QA round-3 findings OPEN

Session arc: resumed the L3 epic → discovered #1076's branch had NEVER run CI
(DIRTY PR) and collided with the peers/P2P epic → operator-approved a
reconciliation design (render-first discipline: **no code without an approved
render of the final result**) → merged, then discovered **Phase D was never
run** (LiveBandStrip/Ft8SetupSurface had ZERO mounts — entire FT-8 feature
dead-wired) → fixed forward (#1082 + reachability test) → released **v0.89.0**
→ two live-QA waves on the operator's R2 → all 11 fixable findings fixed
(#1084, merged 460eff5e) → released **v0.89.1** (installed on the R2, deb
staged + checksum-verified in ~/Downloads) → operator round-3 produced **8 NEW
findings, all OPEN** (below).

## THE OPEN WORK — round-3 findings (operator, live 0.89.1 on R2)

1. **FT-8 shows green/listening on first launch with no radio.** INVESTIGATED,
   NOT false: R2's `~/.config/tuxlink/config.json` has `ft8.enabled: true` +
   device = C-Media USB PnP (persisted by round-2 testing); boot autostart is
   per spec. It IS capturing (silence). Operator finds it misleading —
   **operator ruling needed**: keep autostart (explain in UI?) vs don't persist
   `enabled` across restarts vs demote the ribbon presentation when 0 decodes.
2. **Setup surface is a cramped scroll box** (my wave-2 `max-height:
   min(52vh,480px)` fix for the rig-control clipping). RIGHT fix: needs-setup /
   forced-setup should render as the panel's FULL BODY (replacing map+rail),
   per the approved **firstrun-v2 mock** (full-screen-setup memory:
   `feedback_wizard_is_fullscreen`). Rework StationFinderPanel's setup mount.
3. **BandSubsetPopover clips below the dialog** — it drops DOWN from the
   bottom-anchored strip header. Fix: open UPWARD (`bottom: calc(100% + 4px)`
   on `.si-strip__popover`).
4. **Ribbon FT-8 state strings are lowercase** ("paused") — inconsistent with
   theme ("Off"). Capitalize in DashboardRibbon's ft8 chip.
5. **Map overlay issue UNRESOLVED on 0.89.1.** The z-index:1100 fix IS in the
   tag (verified: `git tag --contains 4a7cb64c` + grep on v0.89.1). So the
   cause is something else — suspects: ancestor `overflow:hidden` clipping,
   the overlays sitting OUTSIDE `.station-finder__map`'s positioned box, or a
   full map REMOUNT ("map reloads" per operator). **Reproduce live on the R2**
   (tooling below) — screenshot the finder open, inspect where
   `.station-finder__layers`/`__reachkey` actually land.
6. **"Refresh off-air" wording + armed-state layout**: operator objects to
   "off-air" (reads as wrong; it's received over the air) — rename (e.g.
   "Refresh from WWV"); the armed note ("Armed for WWVH :45 UTC") collides
   with the local-time cell and forces a blank wrap line — CSS for the
   actions-row spans (`station-finder__offair-note` etc. have NO rules).
7. **Live Decodes tab discoverability** (2nd complaint). The approved mock's
   tab carries a LIVE `34/min` count badge (`si-count`) — verify StationRail's
   tab strip actually renders it (was mid-verification when context filled);
   if missing, wiring it is the designed discoverability fix. If the operator
   still wants the tab gone after that, that's a design change — ask.
8. **No peers visible on the map, no Peers pill.** NOT capabilities —
   `p2p_capabilities` returns all-true hardcoded
   (src-tauri/src/contacts/commands.rs:538). Likely the SAME overlay problem
   as (5) for the pill, and for pins: R2 has 1-2 contacts — check whether any
   have grids (`contacts_read`) — gridless peers get no pin BY DESIGN. Verify
   with finding 5's live inspection.

## Branch / repo state

- **This worktree's branch `bd-tuxlink-b026z.4/offair-button-style` is
  MERGED-DEAD** (#1084 → 460eff5e). Round-3 fixes go on a **fresh branch off
  origin/main** (lifecycle hooks will refuse commits here).
- Open PR: **#1083** (D2/D3 render-harness fixtures + style-probe.py — dev
  tooling, CI green, merge whenever).
- bd: **tuxlink-nkzng (P1)** = VARA bandwidth classes + VARA FM via the
  Winlink channels JSON API (verified: text listings lack bandwidth; no FM
  listing file exists). **tuxlink-b026z.4** notes are current through wave 2.
- Releases: v0.89.0 + v0.89.1 both pre-release (never Latest); promotion is
  operator-only. Release freeze is LIFTED.
- Remaining phase-gates on b026z.4: **D4** (waterfall dB/perf judgment — needs
  operator's eyes on the now-correctly-oriented waterfall with real signals),
  **D5 wire-walk** (operator supplies flows greenfield — their three QA rounds
  are de-facto flow evidence; formalize at close), Phase-C holistic + C7
  reviews were superseded by this session's full-feature audit (documented in
  #1082).

## R2 debugging tooling (established this session — REUSE IT)

- SSH alias **`r2-poe`** (sudoless). Structured logs:
  `~/.local/state/tuxlink/logs/tuxlink.<date-hour>.jsonl` (frontend errors are
  forwarded there; grep `react-error-boundary` / `"level":"error"`).
- **Screenshot**: `ssh r2-poe 'DISPLAY=:1 xwd -root -silent | gzip' > x.gz`,
  then PIL parse (struct.unpack('>25I') header; offset hsize+ncolors*12;
  'RGBX'/'BGRX' bpl) — pattern in this session's scratchpad scripts.
- **Synthetic clicks**: `PYTHONPATH=/tmp python3 /tmp/xclick.py <x> <y>` on
  the R2 (pure-python Xlib shipped to /tmp/Xlib; venv/pip broken there —
  ship wheels from the Pi). Native res 2160x1440.
- Crop screenshots to find exact control coords BEFORE clicking (a bad guess
  clicked into the mailbox once).

## Working discipline the operator enforced this session (KEEP)

- **Render-first**: no product-surface change without a WebKitGTK render of
  the final result approved by the operator (mocks: `docs/design/mockups/
  2026-07-11-station-intel-l3/`, incl. the approved reconciliation +
  peer-selected renders; harness: `dev/render-harness/` + `?view=ft8&state=`
  fixtures + `style-probe.py` on PR #1083).
- Reachability is the recurring failure class: grep mounts/callers/listeners
  before claiming anything works (`StationFinderPanel.ft8mount.test.tsx` is
  the pattern).
- Evidence over theory: logs/screenshots/live-CMS fetches settled every
  disputed point this session.
