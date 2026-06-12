# Handoff — Find-a-Station real-app fixes (tuxlink-s0r1) — sparrow-bison-tamarack

Date: 2026-06-12 · Agent: sparrow-bison-tamarack · bd: **tuxlink-s0r1** (P1, in_progress)
Branch: **`bd-tuxlink-s0r1/findstation-realapp-fixes`** (worktree `worktrees/bd-tuxlink-s0r1-findstation-realapp-fixes`)

## Operator instruction (resume target)

Operator smoked the installed **0.53.0 .deb** (real WebKitGTK) and found 3 real bugs the
Chromium/vitest gates miss. Decisions made; **ship all three as ONE PR**. #1 + #2 are
done + committed; **#3 is the remaining work** — continue it straight through, then a
**real WebKitGTK verification** before the PR merges. Do NOT ship partial; do NOT trust
green automated gates for these (they are provably blind to all three — see below).

## The three bugs (root-caused against the running 0.53.0 app)

1. **Tier pins render as oblong black blobs.** The divIcon sized the dot with an inline
   `style="width:Npx;height:Npx"`. Tauri rewrites the packaged CSP (replaces
   `style-src 'unsafe-inline'` with per-stylesheet nonces), so WebKitGTK drops the
   runtime-injected inline `style` attribute; a `display:block` span with no width
   collapses to a full-width zero-height oblong showing only its dark border. The
   operator pin was fine because it's sized by a CSS class. Chromium dev keeps
   'unsafe-inline' → false green. **React `style={}` is fine** (applied via CSSOM, not a
   parsed attribute) — only the Leaflet HTML-string `style=` was affected.
2. **Mode "Use →" buttons greyed.** They were enabled only when a radio panel of that
   exact mode was already open (`catalogPrefillMode` from `radioPanelMode.kind` in
   AppShell). Opening Find-a-Station standalone left every Use greyed.
3. **HF predictions wildly pessimistic** (80m @ 0% on a guaranteed night NVIS path).
   `deck.rs` models the **far/gateway end as a shortwave whip** (`swwhip.voa`) — a
   vertical with a zenith null, the worst antenna for NVIS — and uses `req_snr_db = 73`
   (a voice/broadcast value). Operator's NVIS intuition was correct.

## DONE + COMMITTED

- **#1 pin render** — commit `f54c6803`. `src/catalog/StationFinderMap.tsx` (drop inline
  style; dot fills wrapper, size from `iconSize` via CSSOM) + `StationFinderPanel.css`
  (`width:100%;height:100%;box-sizing:border-box` on `.station-finder__pindot`) +
  regression test asserting the icon HTML has no `style=`.
- **#2 Use → arms modem** — commit `6c61e4cf`. `Use →` enabled for any dialable channel;
  on click AppShell `handleStationUse` opens the matching modem panel (`onSelectConnection({sessionType:'cms', protocol: dial.mode})`),
  emits the prefill, closes the finder. `src/favorites/prefillEvent.ts` now **retains**
  the last dial (4s TTL) so the just-mounted panel consumes it (the live event fires
  before the panel subscribes). Files: prefillEvent.ts(+test), StationRail.tsx(+test),
  StationFinderPanel.tsx, AppShell.tsx. Opening a panel is UI not TX → RADIO-1 honored
  by the panel's own Connect.
- **tsc green**; targeted vitest green (StationFinderMap 9, StationRail 9, prefillEvent 4,
  StationFinderPanel 5).

## #3 CONFIRMED SPEC (operator decisions 2026-06-12) — BUILD TO THIS

- **REQ.SNR default ≈ 22 dB-Hz** (range 20–25), operator-editable in settings. (73 was
  voice; VOACAP REQ.SNR is dB-Hz = SNR + 10·log₁₀(BW). Anchors: SSB 44, CW 19, FT8 13.)
- **Operator's OWN antenna = selectable preset**, default **NVIS low horizontal dipole**.
  Generic set: NVIS dipole / general-DX dipole / vertical-whip / isotropic. Plus ~10
  **commercial** presets (EFHW, Chameleon, Buddipole, Wolf River, Hustler, Hamstick,
  Comet, G5RV, Hexbeam/Yagi) mapped to VOACAP models — **full table + sources in
  `dev/scratch/antenna-presets-research.md`** (gitignored; lives in this worktree).
- **Gateway (far/RX) antenna = PARSE the listing's `B`/`D`/`V` "Antenna being used"
  code** (legend in every listing header; confirmed present in real fixture data). Map
  Beam→beam/Type-13, Dipole→horizontal dipole, Vertical→vertical/whip. **Fallback to
  isotropic (`CCIR.000`, 0 dBi) only when a gateway reports none** — never assume a whip.
- VOACAP antenna refs: `itshfbc/antennas/default/` (26 `CCIR.???`, `CCIR.000`=isotrope);
  type codes 0/30=const-gain, 13=directional, 14=omni/vertical, 22=HFANT vertical. v1 may
  reference stock `.voa` files. Deck ANTENNA card format is in `deck.rs` (lines ~81–112).

## #3 STATUS + REMAINING PLAN

- **#3a COMMITTED `1ce8accd`** — `src-tauri/src/catalog/stations.rs`: added
  `GatewayAntenna {Beam,Dipole,Vertical}` enum (serde lowercase) + `Gateway.antenna:
  Option<GatewayAntenna>`, parse `B`/`D`/`V` in `apply_subline`, `antenna: None` in the
  two constructors (stations.rs parse_header_line + stations_disk.rs literal), + 2 tests
  (`no_antenna_line_yields_none`, `parses_gateway_antenna_code`). **NOT YET CARGO-VERIFIED**
  — the Pi cold build was still compiling at commit time (stalled ~45min on printpdf/proptest
  dev-deps). **FIRST ACTION next session:** `cargo test --manifest-path src-tauri/Cargo.toml
  catalog::stations` to confirm green (low risk: both Gateway constructors updated, test mod
  uses `use super::*`, simple match arms). Old build log: `/tmp/cargo-s0r1-parser.log`.
- **#3b** — `deck.rs`: TX antenna = operator preset's VOACAP model; RX antenna = parsed
  gateway antenna (isotropic fallback); REQ.SNR + tx_power from config not the 73/100
  constants. `run_prediction`/`PredictionInputs` (commands.rs) take the antenna + snr +
  power; `propagation_predict_path` command signature gains rx-antenna + reads operator
  prefs (config/state).
- **#3c** — config read/write (Rust `config_*` + TS) for operator antenna preset + REQ.SNR
  + power; **settings UI** (antenna preset dropdown + SNR + power) — inline per
  `inline_ui_no_window_clutter`.
- **#3d** — thread the gateway antenna through the DTO → frontend `Gateway`/`Station`
  model → `useStationPrediction`/`propagationApi.predictPath` so each prediction passes
  that station's gateway antenna. (TS types currently ignore the new `antenna` field;
  extra serialized field is harmless until wired.)
- **Tests** at each layer (parser ✓, deck antenna-selection, config round-trip, a
  prediction test that a gateway `Vertical` vs `Dipole` changes the modeled RX).

## CRITICAL GATE before merge — REAL WebKitGTK, not Chromium

All three bugs are **invisible to vitest + the Chromium harness** (they were green while
0.53.0 shipped broken). Verify in actual WebKitGTK before the PR merges: `pnpm tauri dev`
in this worktree + grim (per `grim_realapp_validation_pandora`), or build a .deb the
operator installs. Confirm: pins are coloured circles (not blobs); Use → opens+prefills a
modem with no panel pre-opened; an NVIS path on 80m no longer reads 0%. Chromium CANNOT
reproduce #1 (CSP) — do not accept a Chromium-only pass.

## Branch / worktree / evidence state

- Branch has commits `f54c6803` (#1), `6c61e4cf` (#2), + (pending) #3a parser. **Pushed to
  origin** at handoff (so nothing is stranded). No PR opened yet (one PR when #3 done).
- Worktree gitignored-on-disk (NOT in git, stays in this worktree): `dev/scratch/antenna-presets-research.md`
  (the #3 spec source), `dev/scratch/findstation-realapp-*.png` + `fs-*.png` (operator-app
  evidence crops), `node_modules/`, any `target/`.
- Main checkout (`recover-handoffs`) remains git-blocked by another live session all
  session — consistent with prior handoffs. Several older untracked handoffs sit there for
  the operator to commit.
- bd `tuxlink-s0r1` notes carry the same confirmed spec (durable in Dolt).

## Watch-outs

- `git commit --amend` is hook-banned even for local unpushed commits — make NEW commits.
- Don't reintroduce inline `style=` in any Leaflet divIcon HTML (CSP strips it). Regression
  test guards `stationPinIcon`.
- Pin `cargo --manifest-path src-tauri/Cargo.toml` / `pnpm -C <worktree>`; bash cwd can
  revert from the worktree to the main checkout mid-session.
- Codex adversarial round on the #3 RF/deck change is appropriate before merge (quota
  resets ~2026-06-13).
