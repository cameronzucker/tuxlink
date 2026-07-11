# Design: Station Intelligence L3 — panel integration

- **Issue:** tuxlink-b026z.4 (epic tuxlink-b026z) · **Status:** DRAFT v1 (pre-adrev)
- **Agent:** owl-kestrel-lichen · **Date:** 2026-07-11
- **Upstream contracts:** L2 spec `docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md`
  (§Snapshot, §Commands, §Events, §Device selection, §Band provenance, §Sweep) —
  all consumed as shipped in PR #1072; nothing in L3 modifies the L2 service.
- **Epic design:** `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md` (APPROVED 2026-07-05)
- **Approved mocks (operator, this session):** `docs/design/mockups/2026-07-11-station-intel-l3/`
  — `station-intel-l3-mock-v4.html` (layout baseline, "final shape — approved as
  mocked"), `station-intel-l3-rail-variants.html` (A/B/C deliberation; B chosen),
  `station-intel-l3-states-v1.html` (states 1–5 + popover + ribbon approved),
  `station-intel-l3-firstrun-v2.html` (state 6 as revised; supersedes states-v1
  card 6). Render with `dev/render-harness/snapshot.py` under
  `WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe`.

## Operator user flows (VERBATIM — the wire-walk gate traces these as stated)

**Flow 1 — first run:**

> "a user on first run should be able to open spatial intelligence,
> program/connect their radio, the radio should be addressable via CAT (if
> supported) to dial through FT-8 stations, and that information should be
> displayed in an optional heatmap with live FT-8 waterfall and FT-8 stations
> decoding. They should also be able to limit FT-8 decode to a subset of bands
> or only one band if desired."

**Flow 2 — agent tool use** (lands in L4/tuxlink-b026z.5, recorded on that
issue's notes; repeated here because the gate covers L3+L4 together):

> "Agents should be able to derive band intelligence to select stations
> intelligently based on strong clusters of heard FT-8 stations, including
> radio CAT control, band switching, etc."

Flow capture is CLOSED (operator: additional flows "similar enough to the
first run flow that I think they're not worth litigating").

## Scope

**In (all additive; the gateway finder loses nothing):**

1. Rename the user-facing surface Find-a-Station → **Station Intelligence**
   (§Renames — strings only, internal identifiers stay).
2. **Band-chip openness dots** on the existing band filter chips, with honest
   no-data semantics (§Openness).
3. **Rail tab shell** (none exists today): *Station* | *Live decodes* (§Rail).
4. **Band-matrix rail** (approved variant B): the path-forecast section and the
   channel list merge into one row-per-band matrix (§Rail).
5. **Aim hero + magnetic declination** (operator-added requirement) (§Declination).
6. **Collapsible "Live band · FT-8" strip**: Canvas2D waterfall + live decode
   feed + provenance chip + stats + sweep/band-subset popover (§Strip, §Waterfall).
7. **First-run / degraded-state surfaces** for all six mandated states, incl.
   in-panel device picker with live meters and the embedded shared rig-control
   form with Test CAT (§States, §FirstRun).
8. **DashboardRibbon FT-8 badge** (APRS-badge pattern, four states) (§Ribbon).
9. **Map layer-control component** (Gateways entry only — see L5 boundary).
10. Menu + modem-pane label changes (§Renames).
11. New backend commands: `ft8_device_meter`, `ft8_cat_probe`,
    `magnetic_declination` (§NewCommands).

**Out:**

- The FT-8 heat **layer itself** — that is L5 (tuxlink-b026z.6). L3 ships the
  map layer-control housing with the Gateways entry; **the "FT-8 heat" toggle
  entry appears only when L5 lands** — no dead control ships
  (no-incomplete-refs discipline). The approved v4 mock illustrates the
  post-L5 end state.
- MCP tools (L4). VOACAP/FT-8 fusion (never). Any TX path (never — this
  feature is RX-only; RADIO-1 trivially satisfied).
- Multi-receiver support; L2 service changes; wizard changes.

## Architecture

### Frontend data layer — `useFt8Listener` (new hook, `src/ft8ui/useFt8Listener.ts`)

The APRS hooks are event-accumulating only; FT-8 state must survive panel
remounts (slot phase computes from ring recency and NEVER resets on reopen —
L2 delta pin), so this hook introduces the snapshot-hydrate + event-follow
pattern:

1. On mount: `invoke('ft8_listener_snapshot')` → full `Ft8Snapshot` (15
   camelCase fields, the L3 hydration contract).
2. Subscribe `ft8-listening:change` (axis/flags/phase/band/sweep deltas) and
   `ft8-decodes:slot` (one `SlotRecord` per slot boundary — including drops
   and discards) via the `useAprsChat.ts` `subscribe<T>` idiom (mounted guard,
   unlisten cleanup, `.catch(() => {})` for jsdom).
3. Exposes: `{ snapshot, decodesRing, uiState, bandActivity }` where `uiState`
   is the §States mapping output and `bandActivity` the §Openness derivation.
4. Singleton-per-window via context provider in AppShell (ribbon badge + panel
   + strip all read one subscription; no duplicate listeners).

### Component inventory

| Concern | File | New/Mod |
|---|---|---|
| Panel container, strip mount, view state | `src/catalog/StationFinderPanel.tsx` | Mod |
| Band-chip openness dots | `src/catalog/StationFinderControls.tsx` | Mod |
| Rail tab shell + tabs | `src/catalog/StationRail.tsx` | Mod |
| Band-matrix section (replaces prop-bars + channel list) | `src/catalog/BandMatrix.tsx` | New |
| Aim hero + declination | `src/catalog/StationRail.tsx` | Mod |
| Live decodes tab (station-centric aggregation) | `src/catalog/LiveDecodesTab.tsx` | New |
| Strip (header/stats/popover/collapse) | `src/ft8ui/LiveBandStrip.tsx` | New |
| Waterfall canvas | `src/ft8ui/Waterfall.tsx` | New |
| Decode feed table | `src/ft8ui/DecodeFeed.tsx` | New |
| First-run / blocked surfaces | `src/ft8ui/Ft8SetupSurface.tsx` | New |
| Sweep/band-subset popover | `src/ft8ui/BandSubsetPopover.tsx` | New |
| Map layer control housing | `src/catalog/StationFinderMap.tsx` | Mod |
| Ribbon badge | `src/shell/DashboardRibbon.tsx` (+AppShell wiring ~701-730, 1735-1741 pattern) | Mod |
| Menu label | `src/shell/chrome/menuModel.ts:97` | Mod |
| Modem-pane entry labels | `src/radio/RadioPanel.tsx` (label via prop from mode panels) | Mod |
| Shared rig control third render site | `src/radio/modes/RigControlSection.tsx` consumed by `Ft8SetupSurface` | Reuse |
| Snapshot/event hook | `src/ft8ui/useFt8Listener.ts` | New |

All dropdowns in every new/modified surface render through the shared
`Select` control (`src/controls/Select.tsx`, `.tux-select`) — **no native
`<select>` chrome anywhere** (operator requirement 2026-07-11). Same for
buttons/fields: `src/controls` first, panel-idiom CSS second, never bare
defaults.

### State mapping (§States)

`uiState` derives from `Ft8Snapshot` in one pure function (unit-tested per row):

| # | uiState | Derivation (first match wins) | Treatment (approved states-v1/firstrun-v2) |
|---|---|---|---|
| 6 | `needs-setup` | `service == blocked(needs-device-selection \| device-absent \| wsjtx-missing)` OR `available_devices` present pre-start | Strip body = setup surface (§FirstRun); grey dot; NEEDS SETUP chip |
| 5 | `yielded` | `service == paused(modem-hold)` | Amber dot; YIELDED chip; waterfall frozen+dimmed under overlay; feed keeps pre-yield rows |
| — | `wedged` | `service == blocked(capture-wedged)` | Red dot; error banner: "restart Tuxlink" (L2: restart-required; set_device/start return errors) |
| 4 | `clock-unsynced` | `flags.clock_unsynced` (listening continues) | Amber dot + amber banner naming chrony + consequence |
| 2 | `waiting-first-slot` | `slot_phase == WaitingFirstSlot` | Green dot; waterfall filling from top; feed countdown copy |
| 3 | `band-dead` | `slot_phase == BandDead` | Green dot (capture healthy); quiet-not-broken copy w/ live RMS + hot-band suggestion |
| 1 | `decoding` | `slot_phase == Decoded` | v4 baseline |

`wedged` is a seventh rendered state beyond the issue's six — it exists in the
L2 axis and must not fall through to a generic error. `cat_fixed_band` and
`jt9_degraded` flags render as strip-header chips, orthogonal to the states.

### Openness dots — honest-recency semantics (§Openness)

One receiver samples one band at a time. A dot NEVER claims knowledge it
doesn't have:

- **hot / warm / quiet**: computed from ring `SlotRecord`s for that band within
  the recency window (default 10 min): decodes/min ≥ 8 → hot; ≥ 1 → warm;
  sampled-but-0 → quiet.
- **no-data** (hollow/absent dot + tooltip "not sampled recently"): no slot for
  that band inside the window. This is the DEFAULT for every band except the
  held band unless sweep is on.
- Staleness: dot opacity fades linearly over the window; tooltip carries
  "sampled Nm ago · dwell k slots" (same recency+dwell honesty L4 requires).
- Dots appear on the top band chips AND in the band-matrix rows (approved:
  deliberate redundancy — pre-selection vs per-station context).

### Rail (§Rail)

- Tab shell (new): **Station** | **Live decodes** (+ live rate caption).
- **Station tab** = existing header + aim hero (§Declination) + **BandMatrix**:
  one row per HF band + VHF row. Row = `band label · openness dot · VOACAP
  bar+% · dial chips`. Chips = Use-action (mode swatch + kHz + ☆ favorite,
  exact `rankedDialsFor`/`channelToDial` semantics preserved from the current
  channel rows, incl. QSY candidate ordering — the clicked chip is always
  candidates[0]). Bands with no channel: dimmed "no channel". Best-band row
  highlighted. VHF row: no VOACAP bar (never ranked), LoS caption.
- **Live decodes tab** = station-centric aggregation (operator-ratified
  default): callsigns heard on the current band set, columns call · grid ·
  best SNR · count · mi·brg, sorted by recency; row click pans/zooms the map
  to the grid. Chronological raw feed stays in the strip — two questions, two
  surfaces.

### Aim hero + magnetic declination (§Declination)

Operator requirement (verbatim intent: compass users aim with magnetic).

- Display: `281° M` primary ("aim antenna · compass"), `291° T` secondary,
  distance unchanged; provenance line beneath: `declination +9.7° E · WMM
  2025 · from <operator grid> · updates with your location`. Compass needle
  stays TRUE-referenced (it matches the map); labels carry the translation.
- Math: `magnetic = true − declination` (declination east-positive).
- Backend: `magnetic_declination(grid: String) -> { declDeg: f64, modelEpoch:
  String, validUntil: String }`. Pure-Rust World Magnetic Model evaluation
  with **bundled coefficients** (WMM.COF is public domain) — fully offline, no
  C, no network. Crate vs from-coefficients implementation is a plan-time
  call; the acceptance test is fixed either way: NOAA-published test vectors
  for WMM epoch (lat/lon/date → declination ±0.1°). Grid → lat/lon uses the
  existing grid math. Date from system clock; if `clock_unsynced`, declination
  still computes (sub-year error is negligible against ±0.1°).
- Degrade: no operator grid → hero shows `—` for both bearings (existing
  behavior) and no declination line.

### Live strip (§Strip)

- Header: green/amber/red/grey dot (backend truth) · "Live band · FT-8" ·
  provenance chip (`CAT CONFIRMED` / `OPERATOR-ASSERTED` / dashed
  `UNCONFIRMED — tune your dial to <dial>` / `CLOCK UNSYNCED` / `YIELDED` /
  `NEEDS SETUP`) · stats (`holding <band> · dial <MHz> · N/min · M grids
  heard`) · **`holding <band> ⌄` popover trigger** · collapse control.
- **Band-subset popover** (Flow 1: "limit FT-8 decode to a subset of bands or
  only one band"): multi-select band chips (writes `config.ft8.bands` via the
  L2 crate-wide RMW gate `config::update_config`) + mode radio: *Hold one
  band* (default) / *Sweep selected* (`ft8_set_sweep`; disabled with reason
  when CAT absent) + dwell caption. Band pick while not listening =
  persist-only (L2 consent framing: only a running listener moves the dial).
- Collapse: persisted in the panel's existing localStorage view state
  (`tuxlink:station-finder:view`), default expanded, auto-collapse below a
  window-height threshold so the map never starves (threshold is a plan-time
  constant; behavior operator-ratified).
- Collapsed strip keeps the header row (state remains visible).

### Waterfall (§Waterfall)

- Backend FFT (the audio is already in Rust): a consumer thread reads the L2
  `WaterfallTap` (12 kHz i16 blocks, 32×1200 lossy ring), computes magnitude
  columns, and emits batched columns as a Tauri event `ft8-waterfall:columns`
  (u8 magnitudes, palette applied frontend-side). Tap loss ⇒ visual gap only —
  NEVER decode impact (tap is lossy by design).
- Frontend: Canvas 2D `putImageData` new-column write + self-copy `drawImage`
  scroll. **Probe-validated 2026-07-11** in the exact engine (WebKitGTK
  605.1.15 aarch64, software GL): `getContext('2d')`, 200-row `putImageData`,
  self-copy scroll, `getImageData` readback all pass
  (`dev/scratch/canvas2d-waterfall-probe.html`). The station map's SVG
  decision was specific to Leaflet's `preferCanvas` path — not a general
  canvas prohibition.
- **Exit gate (from the bd issue, unchanged):** a STATED budget — FFT size,
  column cadence, event batch size — under Pi software-GL, with a headroom
  measurement proving paint never starves decode. Initial numbers to validate,
  not tune open-endedly: FFT 2048 @ 12 kHz, 4 columns/s, batch 4 (one event/s),
  ≤ 5 % CPU paint budget on the Pi. Numbers land in the plan's perf task; the
  gate is the measurement, run via the render harness + `top` on the Pi.
- Waterfall pauses with the service (yielded/blocked); slot boundaries drawn
  as faint dashed lines; freq axis 0–3000 Hz fixed.

### First-run / setup surface (§FirstRun — approved firstrun-v2)

Replaces the strip body in `needs-setup`; also reachable later via the strip
header when blocked.

- **Step 1 · Audio input · REQUIRED**: device rows from
  `snapshot.available_devices` (the L2 dual-blocker rule guarantees presence
  while simultaneously blocked on wsjtx — one-visit fix). Each row: friendly
  name, `hw:N`, **live level meter + dBFS** (`ft8_device_meter`), and a
  "used by your ARDOP/VARA setup" badge when the modem audio config matches.
  Hint copy: the radio is the card with a moving level. Picker always asks,
  even with one device (L2 operator decision 2). Zero devices ⇒ plug-in
  guidance + refresh.
- **Step 2 · Rig control (CAT) · OPTIONAL·RECOMMENDED**: the SAME shared
  `RigControlSection` (third render site; writes the one `Config.rig`) with
  model/profile pre-fill, serial port, baud, and **Test CAT** →
  `ft8_cat_probe` reads the dial and prints `✓ radio responds — dial reads
  14.074.00 MHz (20 m)`. Copy: "One radio, one config … set them here and
  they're set everywhere." No cross-pane navigation is ever required.
- CTA `Start listening on <band> →` (disabled until a device is selected, with
  the reason) + caption "starts the decoder on the selected card · nothing
  ever transmits".

### New backend commands (§NewCommands — additive; L2 service untouched)

| Command | Contract | Notes |
|---|---|---|
| `ft8_device_meter(stable_id) -> { rmsDbfs, state }` | Poll model: the picker polls ~2 Hz while open; each call is one short nonblocking ALSA read on the candidate capture device (no session held between calls) | `state ∈ live \| silent \| in-use \| error` — "in-use" doubles as the VARA/ARDOP-holds-card detector; NEVER runs while the FT-8 service owns a device |
| `ft8_cat_probe() -> { dialHz, band } \| error` | One ManagedRig spawn-read-drop through the FT8_ARBITER (serial never held) | The L2 listener-start dial-read path exposed as a command |
| `magnetic_declination(grid) -> { declDeg, modelEpoch, validUntil }` | Pure function, offline WMM | §Declination |

### Ribbon badge (§Ribbon)

`ft8` prop on `DashboardRibbonProps` mirroring the `aprs` shape; rendered
after the APRS block; wired in AppShell to `ft8_listener_start/stop` with
`toggleBusy` during transitions. Four states (approved): off (faint dot) /
starting (amber) / listening (green dot + band + rate) / blocked (amber,
"needs setup", **click opens the Station Intelligence panel** — not a bare
toggle retry). Dot is backend truth from `useFt8Listener`, never optimistic.

### Renames (§Renames)

User-facing strings only; internal identifiers are load-bearing and stay
(`menu:tools:find_gateway` action id — contract-tested; `station-finder__*`
CSS namespace; component filenames):

- Panel title + `aria-label` → "Station Intelligence".
- `menuModel.ts:97` label → "Station Intelligence…".
- Modem-pane entry (`RadioPanel.tsx` button): label becomes contextual via
  prop — "Find a Gateway" when the pane's session target is CMS, "Find a
  Station" for P2P/radio-only (source: the pane's own mode — the panels
  already know `catalogPrefillMode`). Telnet/CMS-fixed panes keep omitting it.

## Testing strategy

- **Pure logic**: state-mapping table (every row + precedence), openness
  derivation (hot/warm/quiet/no-data/staleness), magnetic math vs NOAA WMM
  test vectors, band-subset config round-trip.
- **Hook**: `useFt8Listener` — snapshot hydration, event follow, remount
  keeps phase (the no-reset pin), unlisten cleanup (invoke-mock teardown
  no-args discipline).
- **Components** (vitest + mocked invoke, `StationFinderPanel.test.tsx`
  pattern): all seven states render distinctly (assert copy + `data-state`),
  first-run picker/meter/badge/CTA-disable, popover writes + disabled-sweep
  reason, BandMatrix chips preserve Use/☆ semantics incl. QSY candidate
  ordering, ribbon badge four states, menu parity test update.
- **Map**: layer-control housing with real Leaflet in jsdom
  (`StationFinderMap.test.tsx` pattern).
- **Waterfall**: column-paint unit (putImageData path) + the perf-budget
  measurement on the Pi (exit gate, not CI).
- **Render harness**: smoke the six states + first-run + popover in real
  WebKitGTK (the mocks double as the visual reference).
- CI is the gate (both arches); no local cold cargo builds on the Pi.

## Exit gates

1. All seven states render distinctly in the real engine (harness smokes).
2. Waterfall perf budget stated + measured on Pi software-GL; decode cadence
   unaffected while painting (headroom number recorded in the PR).
3. **Wire-walk of Flow 1 verbatim** on the shipped UI (runs when L3/L4 land;
   Flow 2 traces at L4).
4. Every dropdown is `.tux-select`-styled; zero native chrome.
5. Gateway-finder regression: existing panel tests green untouched semantics.

## Non-goals / invariants

Two lenses, never blended (VOACAP bars and FT-8 dots share rows but never a
scale). Gateway finder untouched. RX-only forever — no code path in L3 keys
the radio beyond CAT frequency/mode set. ECT support target unaffected (no
new system deps; WMM is bundled data). AGENTS.md parity check at PR time.
