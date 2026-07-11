# Station Intelligence L3 — Panel Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fold the shipped passive-FT-8 listening service (L2, PR #1072) into the renamed "Station Intelligence" panel so an operator sees a live waterfall, live decodes with grid/SNR/distance, band-openness, and magnetic-aimed stations off the already-configured rig — with honest degraded states and a first-run setup surface that never dead-ends.

**Architecture:** L3 is additive over the shipped L2 service. Phase A extends the Rust surface (snapshot fields, 6 new Tauri commands, the waterfall FFT thread) additively — the pure `ListenerMachine` is untouched; two edits touch the supervisor's start sequence and are flagged. Phase B builds the `useFt8Listener` hook + two pure derivations (uiState mapping, openness). Phase C builds the 11 frontend components against those. Phase D wires them into AppShell/StationFinderPanel and runs the exit gates. Every datum the UI shows already exists in the L2 ring/snapshot or is derived by a pure, unit-tested function.

**Tech Stack:** Tauri 2.x, Rust (`src-tauri/`, MSRV 1.75, no workspace root), React 18 + TypeScript (`src/`, Vite), vitest + `@testing-library/react`, Leaflet, WebKitGTK 4.1 (software GL on the Pi).

**Canonical spec:** `docs/superpowers/specs/2026-07-11-station-intel-l3-panel-design.md` (APPROVED, post 5-round adrev). Every task cites its spec section; when this plan and the spec disagree, the spec wins and the task is a plan bug — stop and reconcile.

## Global Constraints

- **MSRV 1.75.** No API stabilized in 1.76+ (`Result::inspect_err`, etc.). Clippy `incompatible_msrv` is denied. Rust lint: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings`.
- **No workspace root.** Rust commands need `--manifest-path src-tauri/Cargo.toml`. Per-crate: `tuxlink-jt9`, `tuxlink-capture` are sub-crates under `src-tauri/`.
- **This Pi cannot cold-compile Rust.** Write the Rust + tests, push, let CI (both arches) compile/run. `pnpm vitest run <file>` is fast enough locally on a single file. Never claim a Rust test passed locally — say "written, CI-pending".
- **RX-only forever.** No L3 code path keys the radio beyond CAT frequency/mode set. RADIO-1 (ADR 0018) gates operator run-time TX, not this code; but transmit-adjacent correctness (there is none here) still needs abort/no-runaway. `ft8_cat_probe` and band-QSY set the dial — that is not transmission.
- **Two lenses never blended.** VOACAP reliability bars and FT-8 openness dots may share a row but never a color scale or a merged number.
- **Every dropdown uses the shared `Select` (`src/controls/Select.tsx`, `.tux-select`).** Every transparent/borderless button must pass the WebKit2GTK computed-style check (Task D3) — a bare `<button>` renders native GTK chrome invisibly to Chromium.
- **Wire shapes are the contract.** Frontend consumes camelCase fields + kebab-case enum tags (`"waiting-first-slot"`, `"wsjtx-absent"`, `{"axis":"blocked","reason":"unsupported-sample-rate"}`), NOT Rust identifiers.
- **Commit discipline:** conventional commits, `Agent: owl-kestrel-lichen` + `Co-Authored-By:` trailers on every commit. Subagents in worktrees CODE + STOP; the parent commits (a subagent cannot commit in this worktree — see the memory `subagents-cannot-commit-in-worktrees`). If executing inline, commit per task.
- **Openness invariant:** a dot NEVER claims knowledge it lacks. Only `decoded`/`band-dead` ring outcomes are evidence; `default-unconfirmed` band-source slots are excluded; never-sampleable bands (60m, VHF) render no dot.
- **uiState table is TOTAL over `ServiceAxis`.** Nothing falls through to a phase row except `axis == "listening"`. A stopped service must never render green.

---

## Phase A — Backend surface (Rust; cargo-testable; the pure state machine untouched)

### Task A1: Additive snapshot fields + resolve-time device name

**Spec:** header additive-changes (1)(2); §NewCommands L2-surface scope note.

**Files:**
- Modify: `src-tauri/src/ft8/service.rs` (the `Ft8Snapshot` struct + its builder ~`service.rs:1454-1473`; `Inner` resolve step ~`service.rs:575-583`)
- Modify: `src-tauri/src/ft8/records.rs` (`AudioDeviceChoice` DTO; `Ft8ListeningChange` unchanged — do NOT add config fields here, see Task B1)
- Test: `src-tauri/src/ft8/service.rs` `#[cfg(test)]` (extend the snapshot completeness test)

**Interfaces:**
- Produces: `Ft8Snapshot.sweepConfig: SweepConfigDto { enabled: bool, bands: Vec<String>, dwellSlots: u8 }` (camelCase serde), `Ft8Snapshot.configuredDeviceName: Option<String>`, `AudioDeviceChoice.alsaHw: String` (`"hw:1"`). Consumed by Task B1's hook + Task C6/C8/C9.

- [ ] **Step 1: Write the failing test.** In the snapshot completeness test, assert the serialized snapshot JSON contains keys `sweepConfig` (object with `enabled`/`bands`/`dwellSlots`), `configuredDeviceName`, and that an `AudioDeviceChoice` serializes with `alsaHw`. Assert `sweepConfig` mirrors `config.ft8.sweep` and `configuredDeviceName` is the resolved human name when a device is configured, `None` otherwise.

```rust
#[test]
fn snapshot_carries_sweep_config_and_device_name() {
    let snap = /* build snapshot with a configured device + sweep.bands=["20m","40m"] */;
    let v = serde_json::to_value(&snap).unwrap();
    assert!(v["sweepConfig"]["bands"].is_array());
    assert_eq!(v["sweepConfig"]["dwellSlots"], 8);
    assert_eq!(v["configuredDeviceName"], "USB Audio CODEC");
    // AudioDeviceChoice
    let dev = &v["availableDevices"][0];
    assert_eq!(dev["alsaHw"], "hw:1");
}
```

- [ ] **Step 2: Run to verify it fails.** `cargo test --manifest-path src-tauri/Cargo.toml ft8::service -- snapshot_carries` → FAIL (field absent). (CI if the Pi can't build.)

- [ ] **Step 3: Implement.** Add `SweepConfigDto` (derive `Serialize`, `#[serde(rename_all="camelCase")]`) built from `ft8_cfg.sweep`. Add `configured_device_name: Option<String>` to `Ft8Snapshot` (camelCase via the struct's existing `rename_all`). Store the resolved human name in `Inner` at resolve time (extend `Inner.resolved` or add a sibling `resolved_name: Option<String>` set in the resolve step; the name comes from `enumerate_capture_devices`' `human_name`, captured once at resolve — do NOT enumerate in `snapshot()`). Add `alsa_hw: String` to `AudioDeviceChoice`, populated from the resolved `alsa_hw`/`card_index`.

- [ ] **Step 4: Run to verify it passes.** Same command → PASS. Then `cargo clippy … --all-targets --locked -- -D warnings`.

- [ ] **Step 5: Commit.** `feat(ft8): additive snapshot fields sweepConfig + configuredDeviceName + alsaHw for L3`

### Task A2: `ft8_set_sweep_bands` command

**Spec:** §NewCommands `ft8_set_sweep_bands` row; §Strip band-subset popover.

**Files:**
- Modify: `src-tauri/src/ft8/commands.rs` (new command beside `ft8_set_sweep`, ~`commands.rs:120`)
- Modify: `src-tauri/src/lib.rs` (register in the `tauri::generate_handler!` list ~`lib.rs:2272-2277`)
- Test: `src-tauri/src/ft8/commands.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `ft8_set_sweep_bands(bands: Vec<String>) -> Result<(), Ft8CmdError>` (Tauri command). Emits `ft8-listening:change` after persisting. Consumed by Task C10 (popover).

- [ ] **Step 1: Write the failing tests.** (a) valid bands persist to `config.ft8.sweep.bands` through the ft8 writer mutex; (b) an out-of-table band (`"60m"`) returns `Err(Ft8CmdError { kind: "invalid-band", .. })` and persists nothing; (c) empty list → `Err` `invalid-band` (or a dedicated empty guard); (d) a set omitting other config fields does NOT wipe `config.ft8.device` (the hoi1 two-face guard — seed device, call set_sweep_bands, assert device survives).

```rust
#[test]
fn set_sweep_bands_persists_and_preserves_device() {
    let cfg = /* config with ft8.device = Some(id), sweep.bands = ["20m"] */;
    with_ft8_config_writer(|c| set_sweep_bands_inner(c, vec!["40m".into(), "80m".into()])).unwrap();
    let after = read_config();
    assert_eq!(after.ft8.sweep.bands, vec!["40m", "80m"]);
    assert_eq!(after.ft8.device, Some(id));  // hoi1: not wiped
}
#[test]
fn set_sweep_bands_rejects_out_of_table() {
    let e = set_sweep_bands_inner(&mut cfg, vec!["60m".into()]).unwrap_err();
    assert_eq!(e.kind, "invalid-band");
}
```

- [ ] **Step 2: Run to verify it fails.** `cargo test … ft8::commands -- set_sweep_bands` → FAIL.

- [ ] **Step 3: Implement.** Validate every band against `tuxlink_capture::bands` (the same table `ft8_set_band` uses, `commands.rs:87-89`); reject empty. RMW through `with_ft8_config_writer` (`commands.rs:19-23`) → `write_config_atomic`. After a successful persist, emit `ft8-listening:change` via the same `EventSink` path the service uses. Define `Ft8CmdError { kind: String, detail: String }` if not already present (Serialize, kebab-case kinds per Global Constraints).

- [ ] **Step 4: Run to verify it passes + clippy.**

- [ ] **Step 5: Commit.** `feat(ft8): ft8_set_sweep_bands command (validated, RMW, emits change)`

### Task A3: `ft8_device_meter` + device reservation + `ft8_list_devices`

**Spec:** §NewCommands meter + list rows; §FirstRun Step 1; the reservation rule.

**Files:**
- Modify: `src-tauri/src/ft8/commands.rs` (two commands)
- Modify: `src-tauri/src/ft8/service.rs` (the reservation structure on `Ft8ListenerState`; the step-7 `open_source` acquire, ~`service.rs:671-690`)
- Modify: `src-tauri/tuxlink-capture/src/alsa_source.rs` (a metered-read helper: open → wait ≥1 period → RMS over ~150 ms → close) OR a new `src-tauri/src/ft8/meter.rs`
- Modify: `src-tauri/src/lib.rs` (register both)
- Test: `src-tauri/src/ft8/service.rs` `#[cfg(test)]` (reservation race), `commands.rs` (meter states, list)

**Interfaces:**
- Produces: `ft8_device_meter(stable_id: String) -> Result<MeterDto, Ft8CmdError>` where `MeterDto { rms_dbfs: f64, state: String /* live|silent|in-use|error */ }`; `ft8_list_devices() -> Result<Vec<AudioDeviceChoice>, Ft8CmdError>`. A `DeviceReservation` on `Ft8ListenerState` with `acquire_priority(id)` (listener) and `try_meter(id)` (meter). Consumed by Task C9 (setup surface).

- [ ] **Step 1: Write the failing tests.** (a) meter on a `silent` fake source → `state:"silent"`, finite `rms_dbfs`; (b) meter with NO real read (single post-start nonblocking) would give `NaN` — assert the impl waits a period and returns finite; (c) **reservation race (barrier-synchronized, not `Promise.all`/`join` naively):** a meter read in flight when the listener open acquires the same id → the open WINS (proceeds) and the meter returns `device-reserved`/`in-use`, never an EBUSY that flips the service to `yielded`; (d) stale `stable_id` → `device-not-found`; (e) `ft8_list_devices` returns the same shape as the snapshot's `availableDevices` incl. `alsaHw`.

```rust
#[test]
fn listener_open_wins_reservation_over_meter() {
    let resv = DeviceReservation::default();
    let barrier = Arc::new(Barrier::new(2));
    // thread 1: meter holds a short read
    // thread 2: listener acquire_priority(id)
    // assert: listener acquires; concurrent meter returns Err(kind="device-reserved" | "device-in-use")
    // assert: no path returns an EBUSY that would map to yielded
}
```

- [ ] **Step 2: Run to verify it fails.**

- [ ] **Step 3: Implement.** `DeviceReservation` = `Mutex<HashMap<StableAudioId, ()>>` (or a per-id `tokio`/`std` lock) on `Ft8ListenerState`. Meter path: resolve `stable_id` → device (`device-not-found` on None); `try_meter` checks the reservation (`device-reserved` if held); open S16_LE 48 kHz matching `alsa_source`, discard until ≥1 full 100 ms period, RMS over ~150 ms, close; map ALSA-busy to `state:"in-use"`. Listener open (`execute_start_sequence` step 7): call `reservation.acquire_priority(id)` bounded-await (≤250 ms) BEFORE `open_source`, so an in-flight meter finishes rather than EBUSY-ing the open. `spawn_blocking` for the command; timeouts per spec (meter ≤250 ms, enum ≤1 s). `ft8_list_devices` calls the existing enumeration directly.

- [ ] **Step 4: Run + clippy.**

- [ ] **Step 5: Commit.** `feat(ft8): ft8_device_meter + device reservation + ft8_list_devices`

### Task A4: `ft8_cat_probe` (read-only)

**Spec:** §NewCommands cat_probe row.

**Files:**
- Modify: `src-tauri/src/ft8/commands.rs`; a read-only helper in `src-tauri/src/ft8/service.rs` or `traits.rs` usage
- Modify: `src-tauri/src/lib.rs`
- Test: `commands.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `ft8_cat_probe() -> Result<CatProbeDto, Ft8CmdError>` where `CatProbeDto { dial_hz: u64, band: String }`. Consumed by Task C9 (Test CAT) + Task C10 (sweep-enable gate).

- [ ] **Step 1: Write the failing tests.** (a) with a fake platform whose `rig_read_dial` returns 14.074 MHz and NO modem session → `Ok(CatProbeDto{dial_hz, band:"20m"})`; (b) with an active modem session (fake `ModemState` in the positive set = anything NOT `Stopped|Error|SocketLost`) → `Err(kind:"modem-busy")`; (c) no `Config.rig` → `Err(kind:"rig-not-configured")`; (d) assert the probe does NOT mutate `Inner.band`/`dial_hz`/`band_source` (read `Inner` before and after — unchanged).

- [ ] **Step 2: Run to verify it fails.**

- [ ] **Step 3: Implement.** New read-only method: acquire the FT8 rig lock + route through `rig_session`, call ONLY `platform.rig_read_dial()` (its own spawn-read-drop `ManagedRig`, works from any axis incl. never-started), map dial→band via the band table, touch no `Inner` state. Refuse `modem-busy` by reading the same modem-state source the L2 resume poll uses (`traits.rs:210-215` positive set: proceed only in `Stopped|Error|SocketLost`). `rig-not-configured` when `Config.rig` absent. `spawn_blocking`, ≤3 s timeout → `probe-timeout`.

- [ ] **Step 4: Run + clippy.**

- [ ] **Step 5: Commit.** `feat(ft8): ft8_cat_probe read-only dial probe (refuses during modem sessions)`

### Task A5: `magnetic_declination` (offline WMM)

**Spec:** §Declination; §NewCommands.

**Files:**
- Create: `src-tauri/src/geomag/mod.rs` (or a `tuxlink-wmm` leaf crate if from-coefficients) + bundled `WMM.COF` if that route
- Modify: `src-tauri/src/lib.rs` (command + register)
- Modify: `src-tauri/Cargo.toml` (dependency IF the crate route clears MSRV 1.75)
- Test: `src-tauri/src/geomag/mod.rs` `#[cfg(test)]` (NOAA vectors)

**Interfaces:**
- Produces: `magnetic_declination(grid: String) -> Result<DeclDto, Ft8CmdError>` where `DeclDto { decl_deg: f64, model_epoch: String, valid_until: String }`. Consumed by Task C7 (aim hero).

- [ ] **Step 1: PLAN-TIME GATE (do this first).** Check whether a maintained pure-Rust WMM crate (`world-magnetic-model`, `wmm`) declares `rust-version <= 1.75` and builds on both arches. If yes → crate route. If no → from-coefficients (bundle `WMM.COF`, implement Schmidt semi-normalized Legendre + secular variation). Record the choice in the task's commit body.

- [ ] **Step 2: Write the failing test.** NOAA-published WMM test vectors: for a fixed (lat, lon, decimal-year) assert `decl_deg` within ±0.1° of the published value; for a grid input assert it parses via `position::maidenhead::grid_to_lat_lon` and matches.

```rust
#[test]
fn declination_matches_noaa_vectors() {
    // NOAA WMM2025 test value, e.g. (lat, lon, 2025.0) -> known decl
    let d = declination_at(lat, lon, 2025.0);
    assert!((d - EXPECTED).abs() < 0.1, "got {d}");
}
```

- [ ] **Step 3: Run to verify it fails.**

- [ ] **Step 4: Implement** the chosen route. Grid→lat/lon reuses `position::maidenhead::grid_to_lat_lon` (`invalid-grid` on parse failure). Date from system clock decimal-year. Return `model_epoch` (e.g. `"WMM2025"`) + `valid_until` (epoch end).

- [ ] **Step 5: Run + clippy. Commit.** `feat(geomag): offline magnetic_declination command (WMM, NOAA-vector tested)`

### Task A6: Waterfall backend — FFT consumer thread + token subscriptions + event

**Spec:** §Waterfall (backend, lifecycle, FFT window/hop, gap signal); §NewCommands waterfall rows.

**Files:**
- Create: `src-tauri/src/ft8/waterfall.rs` (the consumer thread + FFT + token registry)
- Modify: `src-tauri/src/ft8/service.rs` (own the token registry; expose `state.tap()` consumer)
- Modify: `src-tauri/src/ft8/events.rs` (`ft8-waterfall:columns` constant)
- Modify: `src-tauri/src/lib.rs` (register subscribe/unsubscribe)
- Modify: `src-tauri/Cargo.toml` (`realfft` if not already present)
- Test: `src-tauri/src/ft8/waterfall.rs` `#[cfg(test)]`

**Interfaces:**
- Produces: `ft8_waterfall_subscribe() -> Result<SubDto{subscription_id:String}, Ft8CmdError>`, `ft8_waterfall_unsubscribe(subscription_id: String) -> Result<(), Ft8CmdError>` (both idempotent). Event `ft8-waterfall:columns` payload `WaterfallBatch { seq: u64, first_col_utc_ms: u64, cols: Vec<Vec<u8>> /* 512 bins each */ }`. Consumed by Task C11 (Waterfall.tsx).

- [ ] **Step 1: Write the failing tests.** (a) subscribe returns a fresh id; two subscribes → two live ids; unsubscribe of one keeps the thread alive; unsubscribe of the last stops the thread; (b) **idempotent:** a stale unsubscribe (already-removed id) is a no-op, does NOT decrement another live id; (c) **zero-subscriber ⇒ zero-FFT:** instrument a `take_blocks`/FFT-invocation counter; after the last token is released, the counter stops advancing; (d) FFT column length = 512 u8 (0–3000 Hz crop of a 2048-pt real FFT); (e) each emitted batch carries a monotonic `seq` and `first_col_utc_ms`.

- [ ] **Step 2: Run to verify it fails.**

- [ ] **Step 3: Implement.** Token registry (`Mutex<HashMap<String, ()>>` per-process; ids reaped on window close — expose a reap hook). A SINGLE consumer thread spawned when the registry goes 0→1, joined when it goes 1→0. It calls `state.tap().take_blocks()` (destructive drain — the ONLY drainer), forms 2048-sample columns at 4 Hz (hop ≈ 3000 samples), `realfft` → magnitude → crop 0–3000 Hz → 512 u8, batches 4 cols, emits `ft8-waterfall:columns` with `seq`+timestamp. Counter for the zero-FFT test behind `#[cfg(test)]` or an `AtomicU64`.

- [ ] **Step 4: Run + clippy. Commit.** `feat(ft8): waterfall FFT consumer thread + token-counted subscriptions`

### Task A7: Emit `ft8-listening:change` on set_device (parity)

**Spec:** header additive (5).

**Files:**
- Modify: `src-tauri/src/ft8/commands.rs` (`ft8_set_device` emits after persist; verify `ft8_set_sweep` already does — if not, add)
- Test: `commands.rs`

- [ ] **Step 1: Write the failing test.** After `ft8_set_device(id)` persists, an `EventSink` fake records one `ft8-listening:change` emission.

- [ ] **Step 2–4: Run-fail → implement (emit after persist, mirroring A2) → run-pass + clippy.**

- [ ] **Step 5: Commit.** `feat(ft8): emit listening:change on set_device for L3 re-hydrate`

**Phase A review gate:** after A1–A7, review the batch from multiple perspectives (≥3 rounds): serde shape correctness (camelCase/kebab), the reservation not deadlocking the start sequence, the waterfall thread lifecycle, MSRV. Push; let CI compile both arches. Update the private journal.

---

## Phase B — Frontend foundation (hook + pure derivations; vitest)

### Task B1: `useFt8Listener` hook — hydration, generation-gating, re-hydrate on change

**Spec:** §Frontend data layer (all 5 steps + provider placement).

**Files:**
- Create: `src/ft8ui/useFt8Listener.ts`, `src/ft8ui/ft8Types.ts` (wire-shape TS types)
- Create: `src/ft8ui/useFt8Listener.test.ts`

**Interfaces:**
- Produces: `useFt8Listener(): { snapshot, decodesRing, uiState, bandActivity }`; a context `Ft8ListenerProvider`. Consumes Task B2 (`deriveUiState`) + B3 (`deriveBandActivity`). `ft8Types.ts` exports `Ft8Snapshot`, `SlotRecord`, `Ft8ListeningChange`, `AudioDeviceChoice`, `MeterDto`, etc. (camelCase). Consumed by every Phase C component.

- [ ] **Step 1: Write the failing tests.** (a) **listeners-before-snapshot race:** mock `listen` + `invoke`; emit a `ft8-decodes:slot` BETWEEN the invoke resolving and the listener registering → the decode is NOT lost (dedupe by `slotUtcMs` against `ring_tail`); (b) a `ft8-listening:change` in the gap is applied; (c) **re-hydrate:** any `:change` triggers a coalesced (debounced ~150 ms) `ft8_listener_snapshot` re-invoke; the popover-relevant `sweepConfig` updates from it; (d) **generation-gating:** unmount before replay commits nothing; an older snapshot resolving after a newer one is discarded; (e) bounded ring at 240 (evict oldest); (f) unlisten called on unmount (invoke-mock no-args teardown discipline — the mock is called with no args at cleanup).

```ts
it('does not lose a slot emitted between snapshot-resolve and listen-register', async () => {
  // arrange mocks so listen registration is deferred one tick past invoke resolution
  // emit ft8-decodes:slot with slotUtcMs=T in that gap
  // assert decodesRing contains T exactly once
});
```

- [ ] **Step 2: Run to verify it fails.** `pnpm vitest run src/ft8ui/useFt8Listener.test.ts` → FAIL.

- [ ] **Step 3: Implement** per §Frontend data layer: register both listeners first (buffer early events), then `invoke('ft8_listener_snapshot')`, replay+dedupe, generation token gates every commit, coalesced re-hydrate on `:change`, 240-bounded ring, `.catch(()=>{})` for jsdom. Provider is a plain context; the hook reads it.

- [ ] **Step 4: Run to verify it passes.**

- [ ] **Step 5: Commit.** `feat(ft8ui): useFt8Listener hook (race-safe hydration, generation-gated, re-hydrate on change)`

### Task B2: `deriveUiState` — total mapping

**Spec:** §States table (rows 0a/0b/6/6b/wedged/5/2/3/1 + flags overlay).

**Files:**
- Create: `src/ft8ui/deriveUiState.ts`, `src/ft8ui/deriveUiState.test.ts`

**Interfaces:**
- Produces: `deriveUiState(snapshot: Ft8Snapshot): { state: Ft8UiState; flags: Ft8Flags }` where `Ft8UiState` is a union of the 9 states + `'device-lost'`. Consumed by B1 + C-components.

- [ ] **Step 1: Write the failing tests.** A TOTALITY test: for every `axis` value × every blocked `reason` × representative `slotPhase`, assert a defined state and that the phase rows (2/3/1) are reached ONLY when `axis=="listening"`. Named rows: `stopped`+stale `slotPhase:"decoded"` → `'off'` (NOT green); `blocked/unsupported-sample-rate` → `'needs-setup'` (unsupported arm), never a phase row; `blocked/device-absent` with device set → `'device-lost'`; without → `'needs-setup'`; `yielded` → `'yielded'`; flags (`clockUnsynced`) returned SEPARATELY (overlay), not replacing the state.

```ts
it('stopped service with stale decoded phase renders off, never decoding', () => {
  expect(deriveUiState({ service:{axis:'stopped'}, slotPhase:'decoded', /*…*/ }).state).toBe('off');
});
```

- [ ] **Step 2–4: Run-fail → implement first-match-wins with an axis guard on the phase rows + flags computed independently → run-pass.**

- [ ] **Step 5: Commit.** `feat(ft8ui): deriveUiState total mapping over ServiceAxis`

### Task B3: `deriveBandActivity` — openness dots

**Spec:** §Openness (evidence-only, provenance-gated, per-sampled-minute, no-data vs never-sampleable, fade floor).

**Files:**
- Create: `src/ft8ui/deriveBandActivity.ts`, `src/ft8ui/deriveBandActivity.test.ts`

**Interfaces:**
- Produces: `deriveBandActivity(ring: SlotRecord[], nowMs: number): Map<string, BandDot>` where `BandDot { tier: 'hot'|'warm'|'quiet'|'no-data'; opacity: number; sampledAgoMs: number|null; dwellSlots: number }`; and a `stripStats(ring, band, nowMs): { decodesPerMin, gridsHeard }`. Consumed by C4/C6 (chips + matrix) + C8 (strip stats).

- [ ] **Step 1: Write the failing tests.** (a) only `decoded`/`band-dead` outcomes count — a `discarded`(qsy-transition) or `dropped-*` slot on a band never yields a `quiet` dot (stays `no-data`); (b) `bandSource:"default-unconfirmed"` slots excluded from attribution; (c) rate = Σ decodes.length ÷ (evidence-slot-count × 15 s) per SAMPLED minute — 30 decodes over 8 evidence slots (2 min sampled) = 15/min = hot, NOT diluted by the 10-min window; (d) tiers ≥8 hot / ≥1 warm / sampled-but-below quiet; (e) fade opacity floors at 0.4; (f) `gridsHeard` = distinct 4+char grids in the window.

- [ ] **Step 2–4: Run-fail → implement → run-pass.**

- [ ] **Step 5: Commit.** `feat(ft8ui): deriveBandActivity + stripStats (evidence-only, provenance-gated)`

**Phase B review gate (≥3 rounds):** totality holes, race-test genuinely exercising the gap, openness math against the spec's worked examples. Commit + push.

---

## Phase C — Components (vitest + mocked invoke; against B)

Each component task follows the same TDD rhythm (failing render/behavior test → implement → pass → commit) and uses the shared `Select`/controls + panel CSS idiom. Interfaces below are the contract; the spec section is the behavior source of truth.

### Task C1: Renames (strings + tests)
**Spec:** §Renames (full inventory + breaking assertions).
- Modify: `StationFinderPanel.tsx:369,374` (title + aria-label → "Station Intelligence"); `menuModel.ts:97`; `RadioPanel.tsx:65-75` (label via new prop, sourced from session `intent`); `CatalogReplyView.tsx:148,161`; `FavoritesPanel.tsx:126`. Update the 6 breaking assertions (`StationFinderPanel.test.tsx:51,85,87,121,136`; `AppShell.test.tsx:442-447`) to `/station intelligence/i`. Verify via `grep -rn "Find a Station\|Find a gateway" src/` = only intended.
- [ ] TDD: update the assertion tests FIRST (they now expect the new string → fail on old code), rename, pass. Commit `refactor(catalog): rename Find-a-Station → Station Intelligence (user strings)`.

### Task C2: `Ft8ListenerProvider` mount + ribbon badge
**Spec:** §Ribbon; provider placement.
- Modify: `src/shell/DashboardRibbon.tsx` (new `ft8` prop mirroring `aprs` shape; render after APRS block); `src/shell/AppShell.tsx` (mount provider OUTSIDE the lazy boundary; wire `ft8_listener_start/stop`, `toggleBusy` during `transitional`). Four states off/starting/listening/blocked; blocked click opens the panel.
- [ ] TDD: `DashboardRibbon.test.tsx` — four states render distinct `data-state` + copy; blocked click calls the open handler not a toggle. Commit.

### Task C3: `BandMatrix`
**Spec:** §Rail Station tab.
- Create: `src/catalog/BandMatrix.tsx` + test. Rows = finder HF bands + VHF; row = label · openness dot (from B3; none on 60m/VHF) · VOACAP bar+% · dial chips. Preserve `rankedDialsFor`/`channelToDial`, clicked dial = `candidates[0]`. **☆ is a SIBLING element** (keep `save-${mode}-${khz}` testids + `aria-pressed`), never nested in the Use-chip. 3+ chips → best 2 + `+N` overflow.
- [ ] TDD: sibling-☆ testids preserved; `+N` overflow expands; VHF row no dot/no bar; clicked chip is candidates[0]. Commit.

### Task C4: Openness dots on band chips
**Spec:** §Openness (chip redundancy).
- Modify: `src/catalog/StationFinderControls.tsx` (dot on each HF chip from B3; none on 60m/VHF).
- [ ] TDD: hot/warm/quiet/no-data classes; never-sampleable bands render no dot. Commit.

### Task C5: Rail tab shell + Live decodes tab
**Spec:** §Rail (tab shell + Live decodes aggregation + untrusted-input hardening).
- Modify: `src/catalog/StationRail.tsx` (introduce `Station | Live decodes` tabs — none today). Create: `src/catalog/LiveDecodesTab.tsx` + test. Station-centric aggregation over evidence slots; grid-less rows non-interactive + "—"; a later CQ upgrades in place; row click pans via NULL-GUARDED `gridToLatLon` (skip on null).
- [ ] TDD: tab switch; grid-less row non-clickable; malformed grid never pans/throws. Commit.

### Task C6: Aim hero + declination
**Spec:** §Declination.
- Modify: `src/catalog/StationRail.tsx` (aim hero: `281° M` primary / `291° T` / distance; provenance line). Wire `magnetic_declination` (A5) on `useStatusData().grid` change; `magnetic = true − decl` normalized [0,360), 0→360° M; `validUntil` past → append drift note; no grid → `—`.
- [ ] TDD: M/T display; wraparound; grid-change re-invoke; expired-model note. Commit.

### Task C7: `LiveBandStrip` (header, stats, collapse, force-expand)
**Spec:** §Strip (header chips, enumerated stats, collapse + force-expand + small-height contract).
- Create: `src/ft8ui/LiveBandStrip.tsx` + test. Header dot (backend truth) + provenance/health chips (incl. `SWEEP PAUSED` for `fallback-hold`) + stats (`holding`/`dial`/`decodes/min`/`grids heard` from B3) + `holding <band> ⌄` trigger + collapse. Collapse persists under `tuxlink:ft8:strip`; auto-collapse below height threshold; **force-expand on `needs-setup`/`wedged`/`device-lost`** overriding both.
- [ ] TDD: force-expand beats persisted-collapse; fallback-hold chip; stats values. Commit.

### Task C8: `Waterfall.tsx`
**Spec:** §Waterfall (frontend paint + gap rendering).
- Create: `src/ft8ui/Waterfall.tsx` + test. Subscribe (A6) when strip expanded + panel mounted; unsubscribe on collapse/unmount. Canvas2D `putImageData` column + self-copy `drawImage` scroll (probe-validated). Gap-marker row when inter-batch `seq`/wall-gap exceeds cadence (never scroll-join).
- [ ] TDD: subscribe-on-expand / unsubscribe-on-collapse; gap-marker on discontinuity; column paint unit. Commit.

### Task C9: `Ft8SetupSurface` (device picker + meter + rig control + Test CAT)
**Spec:** §FirstRun + §States setup arms.
- Create: `src/ft8ui/Ft8SetupSurface.tsx` + test. Arm by blocked reason (needs-device-selection / wsjtx-absent / unsupported-sample-rate / zero-devices). Device rows: name · `alsaHw` · live meter (A3, ~2 Hz, `in-use` badge) · "used by ARDOP/VARA" badge; meter/start handover (stop+await meter before set_device/start). Step 2: shared `RigControlSection` (add `commitNow()`; await before Test CAT → A4). CTA disabled-with-reason for EVERY blocker.
- [ ] TDD: wsjtx-absent shows package copy WITH a configured device present (never plug-in guidance); CTA disable reasons; commitNow awaited before probe. Commit.

### Task C10: `BandSubsetPopover`
**Spec:** §Strip popover.
- Create: `src/ft8ui/BandSubsetPopover.tsx` + test. Renders from `snapshot.sweepConfig` (config truth, refreshed via B1 re-hydrate). Multi-select chips → `ft8_set_sweep_bands` (A2). Hold-one (default, chips disabled) / Sweep-selected (gated on fresh `ft8_cat_probe` A4; disabled+reason without CAT). Persist-only caption while not listening. Fallback-hold inline warning.
- [ ] TDD: reads sweepConfig; hold-mode disables chips; sweep-enable gated on probe; persist-only caption. Commit.

### Task C11: `DecodeFeed` (strip feed) + map layer-control housing
**Spec:** §Strip feed (cap/virtualize); §Scope map layer-control (Gateways entry only).
- Create: `src/ft8ui/DecodeFeed.tsx` + test (chronological, capped ~200 rows/virtualized; callsign/grid text via React escaping; sanitized keys). Modify: `src/catalog/StationFinderMap.tsx` (layer-control housing, Gateways entry only — NO FT-8 heat entry, that's L5).
- [ ] TDD: feed capped; layer control renders Gateways only (no dead heat toggle). Commit.

**Phase C review gate (≥3 rounds):** each component against its spec section; the sibling-☆ contract; force-expand; untrusted-input guards; no dead L5 control. Commit + push.

---

## Phase D — Integration + exit gates

### Task D1: Wire the panel body
**Spec:** §Architecture inventory; §States.
- Modify: `StationFinderPanel.tsx` (mount `LiveBandStrip` below body; `BandMatrix`/tabs in the rail; setup surface as strip body in setup states via `deriveUiState`). `AppShell.tsx` wiring (provider, ribbon `ft8` prop, `catalogPrefill`).
- [ ] TDD: App-level mount test (production mount path, not just units) — panel opens, strip renders the right uiState from a mocked snapshot. Commit.

### Task D2: State totality render smoke (harness)
- [ ] Render every uiState + firstrun arms + popover + fallback-hold in real WebKitGTK via `dev/render-harness/` (extend `harness.tsx` with FT-8 routes). Include `needs-setup` at 1024×700 (picker + CTA visible, nothing clipped). Save PNGs; eyeball distinctness. Commit harness additions.

### Task D3: WebKit transparent-button + `.tux-select` computed-style gate
- [ ] A WebKit2GTK computed-style check (getComputedStyle in the real engine) over rail tabs / `si-collapse` / `chip-use` / `rf-test` (appearance/border/border-radius ≠ native GTK) and every dropdown = `.tux-select`. Document the check in the harness README. Commit.

### Task D4: Waterfall perf exit gate (Pi, converged build)
- [ ] Against the REAL mounted `Waterfall.tsx` + `LiveBandStrip` in the converged build under software GL: record (a) paint-side CPU headroom; (b) decode-side non-starvation (zero missed slots vs unsubscribed baseline, decode within 15 s, no L2 ring overflow); (c) backend zero-subscriber⇒zero-FFT via the A6 counter. Record numbers in the PR body. (Operator-run if it needs the real rig; otherwise loopback/fake source.)

### Task D5: Wire-walk gate
- [ ] Invoke the `wire-walk` skill. Trace Flow 1's non-heatmap clauses (open → connect/CAT → dial-through-bands → waterfall → decodes → band-subset) verbatim to `file:line`. Heatmap clause defers to L5; Flow 2 to L4. Any broken primary clause = NOT shipped.

### Task D6: Docs + ship
- [ ] AGENTS.md parity check (confirmed no-op). `dev/implementation-log.md` entry. README maturity-matrix note if warranted. Open the PR (draft while CI compiles), let CI gate both arches, mark ready + merge per ADR 0010 (merge-commit, no squash).

**Phase D review gate (≥3 rounds):** integration seams, the wire-walk result, perf numbers real. Then the full-branch review before merge.

---

## Self-review notes (author)

- **Spec coverage:** every §section maps to a task — Frontend data layer→B1; States→B2+D1+D2; Openness→B3+C4+C3; Rail→C3+C5; Declination→A5+C6; Strip→C7+C10+C11; Waterfall→A6+C8+D4; FirstRun→C9; NewCommands→A2–A5+A3; Ribbon→C2; Renames→C1; map housing→C11; exit gates→D2–D5.
- **Cross-task types:** `Ft8Snapshot`/`SlotRecord`/`MeterDto`/`CatProbeDto`/`DeclDto`/`WaterfallBatch` defined in Phase A, mirrored in `ft8Types.ts` (B1), consumed by C. `deriveUiState`/`deriveBandActivity` signatures fixed in B2/B3.
- **Risk-front-loaded:** the adrev P1s (state totality, hydration race, reservation, openness evidence rule, waterfall lifecycle, emit-to-refresh) are all in Phase A/B with dedicated tests before any component consumes them.
