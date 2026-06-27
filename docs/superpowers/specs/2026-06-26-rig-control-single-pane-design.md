# CAT rig control + single-pane connect — design spec

- **bd issue:** tuxlink-8fkkk
- **Session:** butte-crag-marten · 2026-06-26
- **Status:** design approved (shape locked in visual-companion brainstorm); spec under operator review
- **Supersedes/implements:** ADR 0015 (resolves the deferred "Hamlib backend form"); folds in tuxlink-5jb (station↔frequency plane) and tuxlink-gdwh (gateway auto-dial ranking), both of which are largely already satisfied by the shipped Find a Station subsystem (see §3).

## 1. Context

Tuxlink can dial ARDOP HF today, but it cannot set the radio's **frequency or
mode** — the operator tunes the rig by hand. That is the WLE-parity gap. Winlink
Express's connect flow tunes the radio for you; Tuxlink presses Connect and
leaves the VFO wherever it was.

Two hardware facts shape the design:

- **FT-710 internal-codec defect** (measured, memory `project_ft710_internal_codec_tx_reset`):
  holding a CP2105 serial port open *concurrent with audio streaming* resets the
  radio's internal C-Media codec off the USB bus — even in RX, latest firmware,
  both units. The fix is **close-serial sequencing**: set freq/mode over CAT,
  **release the serial**, then stream audio. CAT PTT persists after the serial
  closes.
- **DRA-100 data-port path** puts audio on a *separate* USB device, so the rig's
  internal codec sits idle and CAT can hold the serial open freely — no
  contention. This is the operator's path going forward and the common case.

The feature must serve both: hold-serial-freely on DRA-100, close-serial on
internal-codec radios.

## 2. Decisions

1. **Backend = managed `rigctld` subprocess** (operator-confirmed 2026-06-26).
   Resolves ADR 0015's deferred choice over libhamlib-FFI / own-CAT. Rationale:
   reuses ADR 0015's existing spawn / supervise / SIGINT-clean-stop machinery
   (rigctld becomes the third managed external process alongside ardopcf and
   Dire Wolf); close-serial sequencing reduces to "stop the subprocess to release
   the serial," a process boundary tuxlink already owns. FT-710 = hamlib model
   **1049**, data mode **PKTUSB**.

2. **UI shape = enhance the existing ARDOP modem pane** (`ArdopRadioPanel`). No
   new surface; no duplicated ranking. Find a Station keeps prediction + ranking +
   selection; the modem pane owns running the radio (device config + dialing). The
   feature adds exactly two things to the pane (§4).

3. **`Connect` button keeps its plain label** — not "Connect & tune." Tuning is
   part of what Connect does on an HF modem (WLE-equivalent), not a presumptive
   advertised sub-step.

4. **QSY-on-fail is operator-selectable** (§5). Default: dial the selected
   channel only. Opt-in: auto-QSY walks the ranked list from Find a Station.

5. **No tuxlink-added safeguards** (memory `feedback_no_tuxlink_added_safeguards`):
   no bounded-airtime caps, TOT timers, or extra confirmation modals. Mirror WLE
   behavior. Transparent operator commands.

## 3. What already exists (reused, not rebuilt)

The local checkout this session began on was 2330 commits behind `origin/main`;
the following all ship on `origin/main` and are **reused as-is**:

- **Station↔frequency plane (tuxlink-5jb):** `catalog/stations.rs` `Gateway {
  frequencies_khz: Vec<f64> }` → per-mode listings → `stationModel.ts`
  `Channel { mode, frequencyKhz, ssid, band }` aggregated into station pins.
  ARDOP gateways and their dial frequencies are already modeled (`ListingMode::ArdopHf`).
- **Gateway ranking (tuxlink-gdwh):** `channelReliability(channel, prediction,
  utcHour) → { rel, tier }` backed by the full VOACAP propagation engine
  (`src-tauri/src/propagation/`, antenna patterns, solar/SSN forecast). Ranking a
  destination's ARDOP channels by predicted path quality is a sort over existing
  data.
- **Finder → connect handoff:** `StationFinderPanel` already exposes `onUse(station,
  mode)`, which prefills the ARDOP target today.
- **Favorites / recents:** `FavoriteDial { gateway, freq, transport, band, grid }`
  + `connectDispatch`. Note `Favorite.freq` is **record-only metadata today**
  ("never read back into a form", H8) — this feature is what turns the recorded
  frequency into a real CAT dial.
- **ARDOP transport + connect flow:** `modem_commands.rs`
  `modem_ardop_connect_post_consume_with_factory` → `transport.init()` (≈:343) →
  `transport.connect_arq()` (≈:388).
- **Close-serial CAT PTT bridge:** the existing `catptt_bridge.py` close-serial
  shim stays for the internal-codec **PTT** case.

So this feature is **closing the loop** between Find a Station's ranked channels
and the ARDOP connect mechanics, with a CAT tune step in the middle — not a
greenfield build of ranking or a frequency database.

## 4. UI additions to `ArdopRadioPanel`

> The visual-companion mocks (`.superpowers/brainstorm/.../03-modem-pane.html`)
> are **illustrative only** — bespoke CSS to communicate layout/behavior. The
> implementation uses the pane's **existing styling vocabulary**, so the new
> controls render native to the pane, not as a transplant:
> `radio-panel-sec`, `radio-panel-input-row` / `radio-panel-input`,
> `radio-panel-btn-primary`, `radio-panel-mono`, and the pane's existing
> `expander` / `expander-summary` / `expander-count` pattern.

### 4a. Frequency & mode element (connect area)

- Always present (manual frequency entry is always available — `H8`'s record-only
  `freq` becomes a live dial).
- Shows the freq + mode the rig **will be set to** on Connect. Sourced from a Find
  a Station handoff (which carries freq/band/mode) when present, otherwise typed.
- A **Tune…** affordance sets freq + mode on the rig *now*, without dialing — pure
  manual rig control. On internal-codec radios this is close-serial-safe (set,
  release serial).
- A live rig line: "FT-710 follows on connect" (idle) / live VFO when the opt-in
  poll is enabled (§4b).
- Built from `radio-panel-input` + `radio-panel-mono`.

### 4b. Rig control expander (its own `expander`, separate from Audio)

Collapsible section holding the settings "to run the radio itself":

- **PTT keying:** RTS · CAT · CM108 · VOX (existing PTT picker, extended).
- **CAT backend:** Managed rigctld (the only backend; field present for clarity +
  future).
- **Rig model:** Yaesu FT-710 (hamlib 1049) — model picker, generic rigctld
  underneath.
- **CAT port:** serial device (e.g. ttyUSB0).
- **Close-serial sequencing** toggle — for internal-codec radios; releases CAT
  before audio. Leave off on the DRA-100 data-port path.
- **Live VFO poll** toggle — opt-in (default off). Polls + displays actual VFO
  while connected. **Mutually exclusive with close-serial sequencing** (polling
  holds the serial open), so it is disabled when close-serial is on and only
  available on the DRA-100 path.

### Resolved detail decisions

- Rig control and Audio remain **separate expanders** (scannability; the pane
  already has section structure).
- Frequency element **always shown**; handoff pre-fills.
- Live VFO readout **opt-in** (default off) for the close-serial-incompatibility
  reason above.

## 5. Connect orchestration + QSY-on-fail

The CAT tune step inserts in `modem_commands.rs` **between `transport.init()` and
`transport.connect_arq()`** (the pre-audio window):

```
resolve dial (target + freq + mode [+ ordered ranked list if auto-QSY])
  → tux-rig: SetFreq + SetMode   (the tune step; pre-audio)
  → [internal-codec path] release CAT serial before audio
  → connect_arq (dial)
  → on success: install transport, ConnectedIrs, log attempt (reached)
  → on failure:
       if QSY-on-fail enabled and list not exhausted:
            advance to next ranked channel → tune → connect_arq (loop)
       else: terminate + log attempt (failed)
```

- **QSY-on-fail is operator-selectable** (decision §2.4). Default **off** (dial
  selected channel only; on fail, terminate + log; operator re-selects in Find a
  Station). When **on**, the Find a Station → modem handoff carries an **ordered
  list** of top-N ranked channels and the loop walks them (tune → dial) until one
  connects or the list is exhausted. The setting lives in the Rig control expander
  (connect behavior).
- The handoff is wired to carry the ordered list **regardless** of the setting, so
  enabling auto-QSY needs no data-flow rework.
- Outcomes record into the existing favorites/attempt log
  (`favorite_record_attempt`, `reached` / `failed`).
- Existing abort/disconnect path (tuxlink-o3f2 side-channel abort) bounds any
  in-flight `connect_arq`; the QSY loop respects it.

## 6. `tux-rig` crate (per ADR 0015)

- Trait surface: `Ptt(bool)`, `SetFreq(hz)`, `SetMode(mode)`, `ReadStatus() →
  { freq, mode, ptt }`.
- **Backend:** managed `rigctld` subprocess — drive it over its TCP protocol
  (default 127.0.0.1:4532); lifecycle (spawn / supervise / SIGINT-stop /
  confirm-serial-released) reuses ADR 0015's managed-process machinery.
- **Close-serial sequencing** is a crate-level capability: on the internal-codec
  path, `SetFreq`/`SetMode` then **stop rigctld** (release the serial) before audio
  starts; PTT for that path stays on the existing `catptt_bridge.py` close-serial
  shim. On the DRA-100 path, rigctld stays up and owns CAT freq+mode+PTT.
- Consumed by the ARDOP connect flow now; structured so a future first-party modem
  daemon can link it too (ADR 0015 build-once).
- **Compiles in CI, not on the Pi** (memory `feedback_no_cold_cargo_on_contended_pi`):
  TDD against the trait + a rigctld protocol fake; draft PR early so GitHub CI
  compiles. Arm any backend subagents with the clippy `-D warnings` trap list
  (memory `feedback_no_local_compile_sdd_clippy_arming`).

## 7. Config additions (`config.rs` `ArdopUiConfig`)

New fields (snake_case, no `rename_all` — mirror the TS DTO in the same PR):

- `rig_hamlib_model: Option<u32>` (FT-710 = 1049)
- `rigctld_host: String` (default `127.0.0.1`), `rigctld_port: u16` (default `4532`)
- `cat_serial_path: Option<String>`
- `ptt_keying: enum { Rts, Cat, Cm108, Vox }` (consolidates existing PTT choice)
- `close_serial_sequencing: bool` (default false — DRA-100 is the common path)
- `live_vfo_poll: bool` (default false; forced false when `close_serial_sequencing`)
- `qsy_on_fail: bool` (default false)

## 8. Data flow

```
Find a Station (prediction + ranking + selection)
   └─ onUse(station, mode)  → target + freq + mode  [+ ordered top-N list]
        └─ ArdopRadioPanel: prefill Frequency&mode element + target
             └─ operator may override any field (manual-first)
                  └─ Connect
                       └─ tux-rig SetFreq/SetMode (pre-audio)
                            └─ [internal-codec] release serial
                                 └─ connect_arq → exchange / fail
                                      └─ QSY-next if enabled, else terminate+log
```

Manual-only path (no Find a Station): operator types target + freq + mode directly;
identical Connect orchestration.

## 9. Scope boundaries

**In scope:** `tux-rig` crate (rigctld backend + close-serial capability); the
pre-audio CAT tune step in the ARDOP connect flow; the QSY-on-fail loop (gated);
the modem-pane UI additions (frequency element + Rig control expander); config
fields + TS DTO mirror; the Find a Station → modem handoff carrying freq/mode
(+ ordered list); attempt logging via the existing favorites log.

**Out of scope / reused:** the propagation engine and `channelReliability` ranking
(exists); the station/channel data model (exists); Find a Station's map/rail/
selection UI (unchanged); VOX (never — memory `feedback_never_vox_keying`; the VOX
PTT option is display-only parity, never built or tested as a keying path here);
VFO memories / band-scan (not WLE-parity, deferred).

## 10. Testing posture

- `tux-rig`: TDD against the trait with a rigctld-protocol fake (freq/mode/PTT
  round-trips; close-serial release ordering; subprocess lifecycle). Rust compiles
  in CI.
- Connect orchestration: unit-test the tune-then-dial ordering and the QSY loop
  (tune → fail → next → connect) against a transport fake; assert the pre-audio
  ordering and the internal-codec serial-release ordering.
- UI: component tests for the frequency element (handoff prefill + manual
  override) and the Rig control expander (close-serial ⊻ live-VFO mutual
  exclusion).
- On-air validation is **operator-run only** (RADIO-1 / memory
  `feedback_rf_validation_onair_only`); agent work makes it runnable + observable.

## 11. Process (per ADR 0015 + project gates)

TDD the crate → draft PR (CI compiles) → **mandatory cross-provider Codex
adrev** (memory `feedback_no_carveout_on_cross_provider_adrev`) → merge. Do not
skip the adrev.
