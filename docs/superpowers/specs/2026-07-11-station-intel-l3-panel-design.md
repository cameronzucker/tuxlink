# Design: Station Intelligence L3 — panel integration

- **Issue:** tuxlink-b026z.4 (epic tuxlink-b026z) · **Status:** DRAFT v3 (post adrev R1–R3; R4–R5 pending)
- **Agent:** owl-kestrel-lichen · **Date:** 2026-07-11
- **Upstream contracts:** L2 spec `docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md`
  (§Snapshot, §Commands, §Events, §Device selection, §Band provenance, §Sweep) —
  consumed as shipped in PR #1072. **L3's additive changes on the L2 surface
  (enumerated; the L2 state machine is untouched):** (1) the
  `ft8_set_sweep_bands` command + dwell-boundary re-read semantics; (2) two
  additive `Ft8Snapshot` fields — `sweepConfig { enabled, bands, dwellSlots }`
  (config truth, distinct from the runtime `sweep` status) and
  `configuredDeviceName: Option<String>` (for the "using <device>" arm); (3)
  an additive `AudioDeviceChoice.alsaHw` field (`"hw:1"`) — the shipped DTO
  carries only `humanName`+`stableId`, and identical C-Media-class cards are
  otherwise indistinguishable; (4) `ft8_set_sweep` / `ft8_set_sweep_bands` /
  `ft8_set_device` emit `ft8-listening:change` after persisting so the popover
  and setup surfaces refresh from one source. All serde-additive,
  completeness-tested like the existing fields.
- **Epic design:** `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-ant8s-ardop-connect-fixes-design-20260705-034957-passive-ft8-listener.md` (APPROVED 2026-07-05)
- **Approved mocks (operator, this session):** `docs/design/mockups/2026-07-11-station-intel-l3/`
  — `station-intel-l3-mock-v4.html` (layout baseline, "final shape — approved as
  mocked"), `station-intel-l3-rail-variants.html` (A/B/C deliberation; B chosen),
  `station-intel-l3-states-v1.html` (states + popover + ribbon approved),
  `station-intel-l3-firstrun-v2.html` (first-run as revised; supersedes states-v1
  card 6). Render with `dev/render-harness/snapshot.py` under
  `WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe`.
  The mocks illustrate healthy/sampled conditions (dots on all bands, post-L5
  heat layer); the honest default render is mostly no-data dots — §Openness wins
  where they differ.

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
2. **Band-chip openness dots** with honest no-data semantics (§Openness).
3. **Rail tab shell** (none exists today): *Station* | *Live decodes* (§Rail).
4. **Band-matrix rail** (approved variant B) (§Rail).
5. **Aim hero + magnetic declination** (§Declination).
6. **Collapsible "Live band · FT-8" strip**: Canvas2D waterfall + live decode
   feed + provenance chips + stats + sweep/band-subset popover (§Strip, §Waterfall).
7. **Setup / degraded surfaces** for the full uiState set (§States, §FirstRun).
8. **DashboardRibbon FT-8 badge** (§Ribbon).
9. **Map layer-control component** (Gateways entry only — see L5 boundary).
10. Menu + modem-pane label changes (§Renames).
11. New backend commands: `ft8_device_meter`, `ft8_cat_probe`,
    `magnetic_declination`, `ft8_set_sweep_bands` (§NewCommands).

**Out:**

- The FT-8 heat **layer itself** — that is L5 (tuxlink-b026z.6). L3 ships the
  map layer-control housing with the Gateways entry; **the "FT-8 heat" toggle
  entry appears only when L5 lands** — no dead control ships. The approved v4
  mock illustrates the post-L5 end state.
- MCP tools (L4). VOACAP/FT-8 fusion (never). Any TX path (never — RX-only;
  RADIO-1 trivially satisfied). Multi-receiver. Wizard changes.

## Architecture

### Frontend data layer — `useFt8Listener` (new hook, `src/ft8ui/useFt8Listener.ts`)

FT-8 state must survive panel remounts (slot phase computes from ring recency
and NEVER resets on reopen — L2 delta pin), so this hook introduces the
snapshot-hydrate + event-follow pattern. **Ordering is pinned to close the
mount race** (a change event landing between snapshot resolution and listener
registration would otherwise leave uiState stale indefinitely, since
`ft8-listening:change` fires only on change):

1. Register BOTH listeners first (`ft8-listening:change`, `ft8-decodes:slot`)
   via the `useAprsChat.ts` `subscribe<T>` idiom (mounted guard, unlisten
   cleanup, `.catch(() => {})` for jsdom); buffer events that arrive early.
2. Then `invoke('ft8_listener_snapshot')` → full `Ft8Snapshot`.
3. Replay the buffer over the snapshot: dedupe `ft8-decodes:slot` against
   `ring_tail` by `slotUtcMs`; apply any buffered `:change` received after the
   invoke resolved (last-writer-wins on the summary fields).
4. **Generation-gated commits:** each hydrate effect owns a generation token;
   the snapshot resolution and buffer replay commit ONLY if the generation is
   still current (unmount or a newer hydrate bumps it). The buffer is cleared
   after replay. This closes the residual races: unmount-during-replay, and an
   older `ft8_listener_snapshot` resolving after a newer one.
5. Hook tests: emit between mocked invoke resolution and listener
   registration (no lost slot, no stale axis); unmount-before-replay commits
   nothing; older-snapshot-resolves-last is discarded.

Exposes `{ snapshot, decodesRing, uiState, bandActivity }`. The frontend
`decodesRing` is bounded to 240 records (mirror of the backend ring — evict
oldest). **Wire shapes, not Rust names**, are what the hook and all mapping
tests consume: fields camelCase (`clockUnsynced`, `slotPhase`,
`availableDevices`), enum tags kebab-case (`"waiting-first-slot"`,
`"wsjtx-absent"`, `{"axis":"blocked","reason":"unsupported-sample-rate"}`).

**Provider placement:** the `useFt8Listener` context provider mounts
unconditionally in AppShell, OUTSIDE the lazy panel boundary (the ribbon badge
needs it whether or not the panel chunk ever loads; exactly one subscription
per window). Additional windows hydrate independently — read-only, no
host/client split needed (unlike `useEnvStations`).

Note: `ft8_listener_snapshot` may perform device-enumeration I/O inline when
devices are wanted (L2 `service.rs:1192-1196`); acceptable on mount, but do
not call it on a render-hot path.

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
| Setup / degraded surfaces | `src/ft8ui/Ft8SetupSurface.tsx` | New |
| Sweep/band-subset popover | `src/ft8ui/BandSubsetPopover.tsx` | New |
| Map layer control housing | `src/catalog/StationFinderMap.tsx` | Mod |
| Ribbon badge | `src/shell/DashboardRibbon.tsx` (+AppShell wiring per APRS pattern ~701-733, 1735-1741) | Mod |
| Menu label | `src/shell/chrome/menuModel.ts:97` | Mod |
| Modem-pane entry labels | `src/radio/RadioPanel.tsx` (label prop from mode panels; source = session `intent`, §Renames) | Mod |
| Shared rig control third render site | `src/radio/modes/RigControlSection.tsx` (self-contained: needs only `storageKeyPrefix`; already has `variant` prop) consumed by `Ft8SetupSurface` | Reuse |
| Snapshot/event hook | `src/ft8ui/useFt8Listener.ts` | New |

All dropdowns in every new/modified surface render through the shared
`Select` control (`src/controls/Select.tsx`, `.tux-select`) — **no native
`<select>` chrome anywhere** (operator requirement 2026-07-11). Same for
buttons/fields: `src/controls` first, panel-idiom CSS second, never bare
defaults.

### State mapping (§States)

`uiState` derives from the wire snapshot in one pure function, **total over
the axis** (every `ServiceAxis` value maps; nothing falls through to phase
rows except `listening`). First match wins:

| # | uiState | Derivation | Treatment |
|---|---|---|---|
| 0a | `off` | `axis == "stopped"` | Grey dot; strip body = "Not listening" + `Start listening on <band> →` CTA. Persisted slot phase is IGNORED for rendering (a stopped service must never look live). |
| 0b | `transitional` | `axis == "starting" \| "stopping"` | Amber dot + "Starting…"/"Stopping…" spinner row (start takes seconds: prewarm + CAT label session). Controls disabled. |
| 6 | `needs-setup` | `axis == "blocked"` with `reason ∈ needs-device-selection \| device-absent* \| wsjtx-absent \| unsupported-sample-rate` (*see 6b) | Strip body = setup surface (§FirstRun), arm chosen by reason; grey dot; NEEDS SETUP chip. |
| 6b | `device-lost` | `reason == "device-absent"` AND `config.ft8.device` is set (mid-run loss, self-healing) | COMPACT state, not the full setup surface: amber dot, "Device disconnected — reconnecting… (or pick another input)" with a link that opens the full setup surface. The supervisor retries every 5 s; this state usually self-clears (FT-710 C-Media class is routine). |
| — | `wedged` | `reason == "capture-wedged"` | Red dot; error banner "Audio capture is wedged — restart Tuxlink" (L2: restart-required; set_device/start return errors from it). |
| 5 | `yielded` | `axis == "yielded"` | Amber dot; YIELDED chip; waterfall frozen+dimmed under overlay ("<mode> session active — resumes automatically"); feed keeps pre-yield rows. |
| 2 | `waiting-first-slot` | `axis == "listening"` AND `slotPhase == "waiting-first-slot"` | Green dot; waterfall filling from top; countdown copy. |
| 3 | `band-dead` | `axis == "listening"` AND `slotPhase == "band-dead"` | Green dot (capture healthy); quiet-not-broken copy w/ live RMS + hot-band suggestion. |
| 1 | `decoding` | `axis == "listening"` AND `slotPhase == "decoded"` | v4 baseline. |

**Flags are a separate layer, never a body-replacing state:**

- `clockUnsynced` → amber banner + amber dot OVER whatever the table selected;
  the strip body still renders per the phase/axis row beneath it (listening
  with an unsynced clock keeps its live waterfall + feed).
- `catFixedBand` → strip-header chip (provenance family).
- `jt9Degraded` → amber dot + chip whose tooltip/banner renders
  `snapshot.lastFailure` (L2 put the diagnostic in the snapshot precisely so
  L3 can name the cause — surface it).

**Setup-surface arms by blocked reason (§FirstRun carries the layouts):**

- `needs-device-selection` / `device-absent`-with-no-device-configured → the
  approved firstrun-v2 two-step surface.
- `wsjtx-absent` → package-guidance arm FIRST ("FT-8 decoding needs the
  `wsjt-x` package — install via apt/your package manager, then Retry"),
  device picker beneath it when `availableDevices` present (the L2
  dual-blocker guarantee applies only while `config.ft8.device == None`; when
  a device IS configured, render "using <persisted device name>" instead of
  an empty or misleading zero-device arm).
- `unsupported-sample-rate` → "this input can't capture 48 kHz — choose a
  different card" + the device list. The snapshot does NOT carry
  `availableDevices` in this state (L2 presence rule); the surface calls
  `ft8_list_devices` (part of `ft8_device_meter`'s module — §NewCommands) to
  populate the picker. Never render plug-in-a-device guidance here.
- Zero devices enumerated → plug-in guidance + Refresh (this arm renders ONLY
  when enumeration returned empty, never as a fallback for a missing list).

### Openness dots — honest-recency semantics (§Openness)

One receiver samples one band at a time. A dot NEVER claims knowledge it
doesn't have:

- **Evidence slots only.** The derivation (tier, sampled/no-data split, dwell
  tooltip count) consumes ONLY ring records with outcome `decoded` or
  `band-dead`. `failed`, `dropped-*`, and `discarded` records are invisible to
  it (mirrors L2's own phase rule: drops/discards are not evidence). A
  qsy-transition discard must never manufacture a "quiet" claim.
- **Provenance-gated attribution.** Slots with `bandSource ==
  "default-unconfirmed"` are excluded from dot attribution entirely (nobody
  asserted the radio was on that band). `cat-confirmed` and
  `operator-asserted` slots attribute normally; an operator-asserted dot
  carries the provenance in its tooltip.
- **Rate math (pinned):** `rate = Σ decodes.length over evidence slots ÷
  (evidence-slot-count × 15 s)`, expressed per minute — i.e. per SAMPLED
  minute, never per window-minute (sweep dwells must not dilute). Tiers:
  ≥ 8/min hot · ≥ 1/min warm · sampled-but-below quiet.
- **no-data** (hollow dot + tooltip "not sampled in the last 10 min"): no
  evidence slot for that band inside the window. This is the DEFAULT for
  every band except the held band unless sweep is on.
- **Never-sampleable bands render NO dot at all** (not hollow): the FT-8 band
  table is 160–10 m; the finder's `60m` and `VHF/UHF` chips are structurally
  outside it — "not sampled recently" would be a lie of the other kind.
- Staleness: dot opacity fades linearly over the window with a **40 % floor**
  (an old sampled dot must stay visually distinct from a hollow no-data dot);
  tooltip carries "sampled Nm ago · dwell k slots".
- Window = 10 min = 40 slots — exactly the snapshot's `ring_tail` cap, so a
  fresh hydrate can compute dots without waiting for events.
- Dots appear on the top band chips AND in the band-matrix rows (approved:
  deliberate redundancy — pre-selection vs per-station context).

### Rail (§Rail)

- Tab shell (new): **Station** | **Live decodes** (+ live rate caption).
- **Station tab** = existing header + aim hero (§Declination) + **BandMatrix**:
  one row per band in the finder's `HF_BANDS` list + a VHF row (the plan names
  the exact row list; FT-8 dots render only on rows inside the FT-8 band
  table — §Openness). Row = `band label · openness dot · VOACAP bar+% · dial
  chips`. Chips preserve the shipped Use semantics exactly: `rankedDialsFor`/
  `channelToDial`, clicked dial always `candidates[0]`. **The favorite ☆
  remains a SIBLING interactive element adjacent to (never nested inside) the
  Use-chip** — existing `save-${mode}-${khz}` testids and `aria-pressed`
  semantics preserved; visual grouping is CSS only. Rows with 3+ chips: show
  best 2 (by that band's reliability) + a `+N` overflow chip that expands the
  row. Bands with no channel: dimmed "no channel". Best-band row highlighted.
  VHF row: no VOACAP bar (never ranked), LoS caption, no openness dot.
- **Live decodes tab** = station-centric aggregation over the EVIDENCE slots
  in the 10-min window, all bands (each row carries a small band tag):
  columns call · grid · best SNR · count · mi·brg, sorted by recency. Rows
  whose callsign has no grid heard yet render "—" for grid/mi/brg and are
  non-interactive; a later CQ carrying the grid upgrades the row in place and
  enables click-to-pan. Chronological raw feed stays in the strip — two
  questions, two surfaces.

### Aim hero + magnetic declination (§Declination)

- Display: `281° M` primary ("aim antenna · compass"), `291° T` secondary,
  distance unchanged; provenance line beneath: `declination +9.7° E · WMM
  2025 · from <operator grid> · updates with your location`. Compass needle
  stays TRUE-referenced (matches the map); labels carry the translation.
- Math: `magnetic = true − declination` (declination east-positive),
  **normalized to [0°, 360°)**; display rounds to whole degrees; 0° renders
  as `360° M` (compass convention).
- Backend: `magnetic_declination(grid) -> { declDeg, modelEpoch, validUntil }`
  — pure-Rust WMM evaluation, bundled public-domain coefficients, fully
  offline. Acceptance: NOAA-published WMM test vectors ±0.1°. Crate vs
  from-coefficients is a plan-time call.
- Frontend re-invokes whenever `useStatusData().grid` changes (the hero copy
  promises "updates with your location" — this is the wire).
- `validUntil` in the past → keep rendering the value, append "· model
  expired — declination may drift ~0.1°/yr". Never blank the hero.
- No operator grid → hero shows `—` (existing behavior), no declination line.

### Live strip (§Strip)

- Header: state dot (backend truth) · "Live band · FT-8" · provenance/health
  chips (`CAT CONFIRMED` / `OPERATOR-ASSERTED` / dashed `UNCONFIRMED — tune
  your dial to <dial>` / `CLOCK UNSYNCED` / `YIELDED` / `NEEDS SETUP` /
  **`SWEEP PAUSED — radio not responding`** for `sweep.mode ==
  "fallback-hold"`) · stats · `holding <band> ⌄` popover trigger · collapse.
- **Band-subset popover** (Flow 1: "limit FT-8 decode to a subset of bands or
  only one band"):
  - **Read contract:** the popover renders from `snapshot.sweepConfig`
    (config truth — the new additive field), NOT from the runtime `sweep`
    status: a saved one-band subset must reopen as saved while stopped, and
    the Sweep radio must stay checked (config) during `fallback-hold`
    (runtime). The set commands emit `ft8-listening:change`, so the one
    hydration source refreshes it.
  - Multi-select band chips edit `config.ft8.sweep.bands` via the NEW
    `ft8_set_sweep_bands` command (§NewCommands) — never a raw config write;
    there is no generic config-write command and the field path is
    `sweep.bands`, not `bands`.
  - Mode radio: *Hold one band* (default) / *Sweep selected*. In Hold-one
    mode the multi-select is disabled (greyed, dwell caption hidden) — the
    held band is the strip's existing band selection (`ft8_set_band`); the
    chips govern sweep only.
  - Sweep-enable is gated on a fresh `ft8_cat_probe` result when toggled
    (config-presence alone doesn't prove the radio is answering); probe
    failure renders "radio not responding — check CAT" and leaves sweep off.
  - `sweep.mode == "fallback-hold"` renders inside the popover too: radio
    stays checked (config truth) + inline warning chip.
  - While not listening, edits are persist-only (consent framing); caption
    switches to "saved — applies at next start (will tune your radio)".
  - The popover re-derives its enabled/disabled/reason lines from live
    snapshot changes while open.
- Collapse: persisted under a NEW ft8ui-owned key `tuxlink:ft8:strip`
  (extending the catalog-owned `PersistedFinderView` writer is needless
  coupling); default expanded; auto-collapse below a window-height threshold
  (plan-time constant). **Force-expand override:** `needs-setup`, `wedged`,
  and `device-lost` force the strip expanded, overriding both auto-collapse
  and the persisted bit (the ~700 px first-run window must never hide the
  setup surface behind a chip — the exact lost-user failure the operator
  rejected). The operator may re-collapse during those states;
  that choice is not persisted.
- Collapsed strip keeps the header row (state remains visible).
- **Small-height layout contract (~700 px windows — the project's canonical
  main size):** when the strip is force-expanded by a setup-class state, the
  panel body's 540 px min-height relaxes (the map/rail region shrinks first;
  the setup surface itself scrolls internally if still short) so the device
  picker and CTA are ALWAYS on screen — force-expanded must mean visible,
  not clipped below the panel edge. Render-harness smoke at 1024×700 with
  `needs-setup` is an exit-gate row.

### Waterfall (§Waterfall)

- **Lifecycle (token-counted, pinned):** the backend FFT consumer thread
  exists ONLY while ≥1 live subscription token exists.
  `ft8_waterfall_subscribe() -> subscriptionId` /
  `ft8_waterfall_unsubscribe(subscriptionId)`, both idempotent — a stale
  unsubscribe from a remounted React effect must never decrement another
  window's live subscription (plain counters break under remount; tokens
  don't). Tokens are tracked per window and reaped on window close. Frontend
  subscribes when the strip is expanded AND the panel is mounted;
  unsubscribes on collapse, unmount, and window close (Tauri listener-drop is
  a backstop, not the mechanism). Zero live tokens ⇒ zero FFT work — the L2
  tap's zero-cost-when-unsubscribed design is preserved 24/7. This assertion
  is PART of the perf exit gate.
- Backend: consumer thread reads the L2 `WaterfallTap` (`state.tap()` →
  `subscribe`/`take_blocks`/`unsubscribe`; 12 kHz i16, 1200-frame blocks,
  32-deep lossy ring), computes magnitude columns, emits batched
  `ft8-waterfall:columns` (u8 magnitudes; palette is frontend). Tap loss ⇒
  visual gap only, never decode impact.
- Frontend: Canvas 2D `putImageData` column write + self-copy `drawImage`
  scroll. **Probe-validated 2026-07-11** in the exact engine (WebKitGTK
  605.1.15 aarch64, software GL): getContext/putImageData/self-copy/readback
  all pass (`dev/scratch/canvas2d-waterfall-probe.html`). The station map's
  SVG decision was specific to Leaflet's `preferCanvas` path.
- **Gap rendering:** after yield/resume or unsubscribe/resubscribe, the canvas
  does NOT scroll-join discontinuous time — it draws an explicit gap marker
  row (or clears) so adjacent columns are never minutes apart on a
  continuous-looking scroll.
- **Exit gate (unchanged from the bd issue):** a STATED budget — FFT size,
  column cadence, event batch — under Pi software-GL, with a headroom
  measurement proving paint never starves decode, PLUS the zero-subscriber ⇒
  zero-FFT assertion. Initial numbers to validate: FFT 2048 @ 12 kHz, 4
  columns/s, batch 4 (one event/s), ≤ 5 % CPU paint budget on the Pi.
- Waterfall pauses with the service; slot boundaries as faint dashed lines;
  freq axis 0–3000 Hz fixed.

### Setup / degraded surfaces (§FirstRun — approved firstrun-v2 + §States arms)

Replaces the strip body in `needs-setup`; compact variant for `device-lost`;
also reachable later via the strip header chip when blocked.

- **Step 1 · Audio input · REQUIRED**: device rows from
  `snapshot.availableDevices` when present, else `ft8_list_devices`
  (§NewCommands — the `unsupported-sample-rate` arm needs it). Each row:
  friendly name, the device's `alsaHw` (the new additive DTO field — the
  shipped `AudioDeviceChoice` has only name+stableId, and two identical
  DRA-class cards are otherwise indistinguishable), **live level meter +
  dBFS** (`ft8_device_meter` poll ~2 Hz while the picker is visible), and a
  "used by your ARDOP/VARA setup" badge when the modem audio config matches.
  Meter `state ∈ live | silent | in-use | error` — `in-use` renders "in use
  by <VARA/ARDOP>" in place of the meter. **Meter/start handover:** on device
  select or Start, the frontend stops polling and awaits the in-flight meter
  call before invoking `ft8_set_device`/`ft8_listener_start`; the backend
  device-reservation (§NewCommands) is the enforcement — the frontend
  sequencing just avoids a user-visible refusal blip. Picker always asks,
  even with one device. Zero devices enumerated ⇒ plug-in guidance + Refresh
  (only then).
- **Step 2 · Rig control (CAT) · OPTIONAL·RECOMMENDED**: the SAME shared
  `RigControlSection` (third render site; self-contained — needs only
  `storageKeyPrefix`; writes the one `Config.rig`) with model/profile
  pre-fill, serial, baud, and **Test CAT** → `ft8_cat_probe`; success prints
  `✓ radio responds — dial reads 14.074.00 MHz (20 m)`. Copy: "One radio, one
  config … set them here and they're set everywhere." **Edit-flush contract:**
  `RigControlSection` persists fields on blur/async; Test CAT must not read
  stale `Config.rig`. The section gains a `commitNow(): Promise<void>` (flush
  pending field writes); the setup surface awaits it before probing and
  disables Test CAT while a write is pending — a typed-but-unblurred serial
  path must never produce a false "radio not responding".
- CTA `Start listening on <band> →` — disabled-with-reason covers EVERY
  blocker, not just device-unselected: no device selected ("select an audio
  input"), `wsjtx-absent` ("install wsjt-x first"), `wedged` ("restart
  Tuxlink"). Clicking Start must never silently re-render the same surface.
  Caption: "starts the decoder on the selected card · nothing ever transmits".

### New backend commands (§NewCommands — additive; L2 state machine untouched)

**Execution + error discipline (applies to every row):** commands touching
ALSA, serial, or sysfs (`ft8_device_meter`, `ft8_list_devices`,
`ft8_cat_probe`) run under `spawn_blocking` with bounded timeouts (meter read
≤ 250 ms, enumeration ≤ 1 s, CAT probe ≤ 3 s) — never on the invoke worker.
Every refusal is a machine-readable typed error (kebab-case `kind` tag +
human `detail`): `device-reserved | device-in-use | modem-busy |
rig-not-configured | probe-timeout | invalid-grid | invalid-band` — the UI
copy branches on `kind`, never parses strings.

| Command | Contract | Notes |
|---|---|---|
| `ft8_device_meter(stable_id) -> { rmsDbfs, state }` | Poll: one short nonblocking ALSA read per call, no session held between calls | `state ∈ live \| silent \| in-use \| error`. **Reservation rule (backend-enforced): a per-device reservation keyed by `stable_id` is shared between the meter and the listener's open path. The listener's open takes the reservation with priority; a meter call finding the device reserved returns `device-reserved` immediately; a meter mid-read when the listener wants the device finishes its ≤250 ms read first (the open awaits the reservation, it never EBUSY-fails).** This covers BOTH the persisted-device case (supervisor 5 s reopen retry) and the first-run candidate case (user clicks Start while a meter read is in flight) — no phantom `yielded` from metering, ever. Metering unreserved devices is always safe. |
| `ft8_list_devices() -> Vec<AudioDeviceChoice>` | Same enumeration the snapshot embeds, exposed directly | Needed by the `unsupported-sample-rate` arm (snapshot omits the list there) and the Refresh action. DTO gains the additive `alsaHw` field (header note). |
| `ft8_cat_probe() -> { dialHz, band } \| typed error` | One `Ft8Platform::rig_read_dial` spawn-read-drop, taken under the FT8 rig lock AND routed through `rig_session` exactly like the listener-start dial-read (`start_rig_labeling` path) | **REFUSES (`modem-busy`) while any modem session is active** (same `ModemState` positive-set the L2 resume poll uses: proceed only in `Stopped \| Error \| SocketLost`) — a second serial opener during a live session is the documented FT-710 C-Media reset class. Surface renders "radio busy with <mode> session — disconnect first". |
| `magnetic_declination(grid) -> { declDeg, modelEpoch, validUntil }` | Pure function, offline WMM | §Declination. `invalid-grid` for unparseable locators. |
| `ft8_set_sweep_bands(bands: Vec<String>) -> Result` | Validates every entry against the FT-8 band table BEFORE persisting (`invalid-band`; rejects empty); serializes through the ft8 writer mutex to `config.ft8.sweep.bands`; emits `ft8-listening:change` after persisting | **Live-sweep semantics (pinned):** the sweep scheduler re-reads the list at each dwell boundary; `band_idx` clamps/wraps against the new length; if the current band was deselected, the next boundary QSYs to `list[0]`. Mid-dwell the old band finishes its dwell — no immediate QSY. |
| `ft8_waterfall_subscribe() -> { subscriptionId }` / `ft8_waterfall_unsubscribe(subscriptionId)` | Idempotent, token-counted (§Waterfall); tokens reaped on window close | FFT thread lives while ≥1 live token. |

### Ribbon badge (§Ribbon)

`ft8` prop on `DashboardRibbonProps` mirroring the `aprs` shape
(`{listening, onOpen, onToggleListening?, toggleBusy?}` + band/rate caption);
rendered after the APRS block; wired in AppShell to `ft8_listener_start/stop`
with `toggleBusy` during `transitional`. Four states (approved): off / starting
(amber) / listening (green + band + rate) / blocked (amber "needs setup" —
**click opens the Station Intelligence panel**, not a bare toggle retry). Dot
is backend truth from `useFt8Listener`, never optimistic.

### Renames (§Renames)

User-facing strings only; internal identifiers stay (`menu:tools:find_gateway`
action id — parity test keys on IDs only, so the label rename is
contract-safe; `station-finder__*` CSS namespace; component filenames):

- Panel title + `aria-label` → "Station Intelligence"
  (`StationFinderPanel.tsx:369,374`).
- `menuModel.ts:97` label → "Station Intelligence…".
- Modem-pane entry (`RadioPanel.tsx:65-75`, currently hardcoded "Find a
  gateway"): label becomes a prop; each mode panel supplies it from its
  session **`intent`** (`'cms'` → "Find a Gateway"; `'p2p' | 'radio-only'` →
  "Find a Station"). `PacketRadioPanel` receives `intent` directly;
  Ardop/Vara read `mode.intent` (`radioPanelVisibility.ts:35-55`). NOT
  `catalogPrefillMode` — that is AppShell-local, panel-agnostic, and carries
  no CMS/P2P information.
- **Complete string inventory** (grep `Find a Station|Find a gateway` over
  `src/` at plan time is the completeness check): `CatalogReplyView.tsx:148`
  ("Added N gateways to Find a Station.") and `:161` ("Add to Find a
  Station"), `FavoritesPanel.tsx:126` (empty-state copy) — all become
  "Station Intelligence".
- **Known-breaking test assertions (update, don't weaken):**
  `StationFinderPanel.test.tsx:51,85,87,121,136` (`findByRole('dialog',
  { name: /find a station/i })`) and `AppShell.test.tsx:442-447` (menu label
  + dialog name) — all move to `/station intelligence/i` with zero
  selector/semantic changes beyond the name string.

## Testing strategy

- **Pure logic**: uiState mapping — TOTALITY test (every `ServiceAxis` × every
  blocked reason maps; `stopped`-with-stale-`decoded`-phase renders `off`, not
  green) + precedence rows; openness derivation (evidence-only, provenance
  gating, per-sampled-minute rate, no-data vs never-sampleable, fade floor);
  magnetic math vs NOAA WMM vectors + wraparound normalization; sweep-bands
  validation.
- **Hook**: `useFt8Listener` — listeners-before-snapshot race test (emit
  between invoke resolution and registration), ring dedupe by `slotUtcMs`,
  remount keeps phase, bounded frontend ring, unlisten cleanup (invoke-mock
  no-args teardown discipline), wire-shape (camelCase/kebab) fixtures.
- **Components** (vitest + mocked invoke): every uiState in the table renders
  distinctly (assert copy + `data-state`), flags layer over states
  (clock-unsynced banner + live body), setup arms per blocked reason
  (wsjtx-absent shows package copy with a configured device present —
  never plug-in guidance), CTA disable reasons, force-expand override beats
  persisted collapse, popover (hold-mode disables chips, fallback-hold
  warning, persist-only caption), BandMatrix (sibling ☆ testids preserved,
  +N overflow), Live decodes grid-less rows, ribbon four states, menu parity.
- **Map**: layer-control housing with real Leaflet in jsdom.
- **Waterfall**: column-paint unit; subscribe/unsubscribe lifecycle unit
  (collapse ⇒ unsubscribe); gap-marker on resume; perf-budget measurement on
  the Pi (exit gate, not CI).
- **Render harness**: smoke every uiState + firstrun arms + popover +
  fallback-hold chip in real WebKitGTK. Note: the committed mocks show
  sampled/healthy conditions; the honest default (hollow dots) is the
  expected real render — §Openness wins.
- CI is the gate (both arches); no local cold cargo builds on the Pi.

## Exit gates

1. Every uiState in the §States table (including `off`, `transitional`,
   `device-lost`, and each setup arm) renders distinctly in the real engine —
   including the `needs-setup` force-expanded layout at 1024×700 (picker +
   CTA fully visible, nothing clipped).
2. Waterfall perf budget stated + measured on Pi software-GL; paint never
   starves decode (headroom recorded in the PR); zero subscribers ⇒ zero FFT
   work demonstrated.
3. **Wire-walk of Flow 1 verbatim** on the shipped UI (runs when L3/L4 land;
   Flow 2 traces at L4).
4. Every dropdown is `.tux-select`-styled; zero native chrome.
5. Existing panel/AppShell tests green with only name-string updates
   (enumerated in §Renames); zero selector or semantic weakening.

## Non-goals / invariants

Two lenses, never blended (VOACAP bars and FT-8 dots share rows but never a
scale). Gateway finder untouched. RX-only forever — no L3 code path keys the
radio beyond CAT frequency/mode set. ECT support target unaffected (no new
system deps; WMM is bundled data). AGENTS.md parity check at PR time.

## Adversarial-review disposition log

- **R1 (contract grounding) + R2 (state machine/UX), 2026-07-11:** 14 P1 /
  16 P2 / 9 P3 across both. All P1/P2 dispositioned into this v2: state table
  made total over the axis with listening-guards on phase rows (R1-F2,
  R2-F1/F2); flags demoted to an overlay layer (R2-F5/F6); wsjtx-absent /
  yielded / unsupported-sample-rate names corrected + arms specced
  (R1-F3/F4, R2-F3); device-lost compact state + meter ownership rule
  (R2-F4); listeners-before-snapshot hydration order (R2-F8); eager provider
  (R2-F9); evidence-only + provenance-gated + per-sampled-minute openness
  (R2-F10/F11/F12); `ft8_set_sweep_bands` command replaces the nonexistent
  raw config write, field path corrected (R1-F1, R2-F14); fallback-hold
  surfaced (R2-F15); probe/meter refusal semantics (R2-F18, R2-F4);
  force-expand override (R2-F22); waterfall subscriber lifecycle + gap
  rendering (R2-F23); sibling-☆ pin (R2-F24); grid-less decode rows (R2-F25);
  intent-sourced labels (R1-F5); rename inventory + breaking-assertion list +
  gate-5 reword (R1-F6, R2-F27); wraparound + validUntil (R2-F20/F21);
  separate strip localStorage key (R1-F8); never-sampleable bands (R1-F7,
  R2-F26); wire-shape note (R1-F9); `ft8_list_devices` added (R2-F2 dead-end).
  P3s folded where one sentence sufficed; remainder to plan time (BandMatrix
  row list source, ring_tail=40 note, snapshot sync I/O note — all noted
  inline above).
- **R3 (Codex, 2026-07-11, verdict READY AFTER FIXES):** 4 P1 / 4 P2, all
  dispositioned into this v3: `sweepConfig` + `configuredDeviceName` snapshot
  read-contract (popover was rendering config truth it had no way to read);
  tokenized idempotent waterfall subscriptions (stale unsubscribe from a
  remount could kill the FFT thread under another window);
  `AudioDeviceChoice.alsaHw` additive field (spec displayed `hw:N` the DTO
  doesn't carry); per-device reservation shared by meter + listener open with
  listener priority (the v2 refusal rule missed the first-run candidate-device
  race); generation-gated hydration commits; `spawn_blocking` + bounded
  timeouts + typed kebab-case error kinds for all hardware-touching commands;
  `RigControlSection.commitNow()` flush before Test CAT (blur-persist would
  false-fail a just-typed serial path); ~700 px force-expanded layout contract
  + harness gate. Transcript:
  `dev/adversarial/2026-07-11-station-intel-l3-spec-codex.md` (local,
  gitignored).
