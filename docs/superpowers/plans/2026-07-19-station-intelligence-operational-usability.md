# Station Intelligence Operational Usability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the already-built FT-8 decode data into the operator-facing Station Intelligence panel (map layers, evidence filter, contained feeds, in-panel controls) and adopt the Winlink channels JSON API so frequency and bandwidth become first-class, per the approved spec at `docs/superpowers/specs/2026-07-19-station-intelligence-operational-usability-design.md`.

**Architecture:** Frontend-first: replace the setup body-takeover with an in-strip setup, add three FT-8 layers to the existing Leaflet map via the project's `useLeafletMap`/`useLeafletLayerGroup` hooks, and compute evidence corroboration in a pure TS module. Backend second: a `channels_api.rs` module joins per-channel bandwidth/frequency from `api.winlink.org/gateway/status.json` into the existing `StationsCache` flow and adds `ListingMode::VaraFm`. MCP parity mirrors the evidence math in Rust with shared JSON fixtures.

**Tech Stack:** React 18 + TypeScript + Leaflet 1.9 (raw `L`, SVG renderer), vitest + jsdom, Rust (Tauri 2, reqwest, keyring, rmcp), WebKitGTK render harness for visual gates.

## Global Constraints

- Working tree: `/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-6i0ie-si-operational-usability`, branch `bd-tuxlink-6i0ie/si-operational-usability`. All paths below are relative to that root.
- **NO `cargo` on this Pi, ever** (IDE lockup). Rust compiles/tests run in CI: open a **draft PR** after the first pushed commit so CI compiles every push. Optional local Rust check: `ssh r2-poe` and use the rustup toolchain with `--workspace` (distro 1.75 gives false greens).
- Frontend tests locally: `pnpm vitest run <file>` per file is fine; CI runs the FULL vitest suite + `clippy --all-targets` and is stricter than local. After vitest runs, `pkill -9 -f vitest` ONLY if workers zombie (match your own PIDs, never broad pkill).
- **Subagents implement but do NOT commit** (worktree hook denies them). Each task's commit step is executed by the parent session. Every commit carries both trailers:
  `Agent: harrier-sandbar-cardinal` and `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.
- **No em-dashes** in any authored doc, code comment, or UI copy.
- RADIO-1: nothing in this plan transmits. FT-8 is receive-only; the channels API is an internet fetch.
- Spec constants, verbatim (define once in Task 6 / Task 12, never inline magic numbers): evidence recency **30 min**; SNR threshold default **−24 dB**; radius **15%** of operator→heard distance clamped **[50 mi, 750 mi]**; ghost opacity **0.2**.
- No new npm dependencies (no `leaflet.heat`; the heat layer is grid-square choropleth on the existing SVG renderer). No new Rust dependencies (reqwest/keyring/serde already present), so `Cargo.lock` is untouched; if a dep IS added, regenerate the lockfile in the same commit.
- vitest invoke-mocks are called with NO arguments during teardown: every `invoke` mock implementation must tolerate `invoke()` (guard `cmd` before switching on it).
- serde enums that cross the IPC/JSON boundary get an explicit rename + a wire-shape test (project convention).
- **MSRV 1.75** (`src-tauri/Cargo.toml` `rust-version`; clippy denies `incompatible_msrv`): no APIs stabilized in 1.76+ (e.g. `Result::inspect_err`).
- The `.superpowers/` directory is gitignored; never commit anything from it.

---

### Task 1: `Ft8StripSetup` component (compact in-strip setup form)

The OS-convention replacement for the row-per-device `DeviceList`. Built standalone first; mounted in Task 2; the old surface is deleted in Task 3.

**Files:**
- Create: `src/ft8ui/useDeviceMeterPoll.ts` (extract from `Ft8SetupSurface.tsx:173-249`, verbatim logic)
- Create: `src/ft8ui/Ft8StripSetup.tsx`
- Create: `src/ft8ui/Ft8StripSetup.css`
- Test: `src/ft8ui/Ft8StripSetup.test.tsx`

**Interfaces:**
- Consumes: `Ft8Snapshot`, `AudioDeviceChoice`, `StableAudioId`, `MeterDto`, `Ft8CmdError` from `./ft8Types`; `RigControlSection` (already used by `Ft8SetupSurface.tsx` with `storageKeyPrefix="ft8"`); invokes `ft8_list_devices`, `ft8_set_device` (`{ stableId }`), `ft8_device_meter` (`{ stableId }`), `ft8_listener_start`, `ft8_cat_probe`.
- Produces: `export function Ft8StripSetup(props: Ft8StripSetupProps): JSX.Element` with
  ```typescript
  export interface Ft8StripSetupProps {
    snapshot: Ft8Snapshot;
    onStarted?: () => void;   // fired after ft8_listener_start resolves
    onRetry?: () => void;     // wsjtx-absent Retry
  }
  ```
  and `export function useDeviceMeterPoll(stableId: StableAudioId, enabled: boolean): DeviceMeterState` re-homed in its own module with
  `export interface DeviceMeterState { meter: MeterDto | null; error: Ft8CmdError | null; stopAndAwait: () => Promise<void> }`.

- [ ] **Step 1: Extract `useDeviceMeterPoll`**

Move the hook + `DeviceMeterState` from `Ft8SetupSurface.tsx:173-249` into `src/ft8ui/useDeviceMeterPoll.ts` unchanged (same `METER_POLL_MS = 500`, same enabled-dependency resume contract, same `stopAndAwait`). Export both. In `Ft8SetupSurface.tsx`, replace the local definition with `import { useDeviceMeterPoll } from './useDeviceMeterPoll';` so the old surface keeps compiling until Task 3 deletes it.

- [ ] **Step 2: Write the failing test**

```typescript
// src/ft8ui/Ft8StripSetup.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { Ft8StripSetup } from './Ft8StripSetup';
import type { Ft8Snapshot } from './ft8Types';

const DEVICES = [
  { humanName: 'Digirig Mobile', stableId: { kind: 'usbVidPidSerial', value: 'a' }, alsaHw: 'hw:1,0' },
  { humanName: 'Loopback: Analog', stableId: { kind: 'cardIdHash', value: 'b' }, alsaHw: 'hw:2,0' },
];

function snap(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'blocked', reason: 'needs-device-selection' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'waiting-first-slot', band: '20m', dialHz: 14_074_000,
    bandSource: 'default-unconfirmed', bandLabelConfirmedUtcMs: null,
    sweep: { mode: 'inactive', bandIdx: 0, dwellProgress: 0 },
    engineVersion: null, nConsecutive: 0, kConsecutive: 0,
    lastSlotUtcMs: null, lastFailure: null,
    availableDevices: DEVICES, ringTail: [],
    sweepConfig: { enabled: false, bands: [], dwellSlots: 0 },
    configuredDeviceName: null,
    ...over,
  } as Ft8Snapshot;
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd?: string) => {
    if (!cmd) return undefined;                       // teardown no-arg calls
    if (cmd === 'ft8_list_devices') return DEVICES;
    if (cmd === 'ft8_device_meter') return { rmsDbfs: -32, state: 'live' };
    return undefined;
  });
});

describe('Ft8StripSetup', () => {
  it('renders one <select> with an option per device, not row-per-device buttons', async () => {
    render(<Ft8StripSetup snapshot={snap()} />);
    const select = await screen.findByTestId('ft8-setup-device-select');
    expect(select.tagName).toBe('SELECT');
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual(
      expect.arrayContaining(['Digirig Mobile', 'Loopback: Analog']),
    );
    expect(screen.queryByText('Use this device')).toBeNull();
  });

  it('selecting a device persists it via ft8_set_device with the stableId', async () => {
    render(<Ft8StripSetup snapshot={snap()} />);
    const select = await screen.findByTestId('ft8-setup-device-select');
    fireEvent.change(select, { target: { value: 'Loopback: Analog' } });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('ft8_set_device', { stableId: DEVICES[1].stableId }),
    );
  });

  it('Start listening invokes ft8_listener_start and fires onStarted', async () => {
    const onStarted = vi.fn();
    render(
      <Ft8StripSetup snapshot={snap({ configuredDeviceName: 'Digirig Mobile' })} onStarted={onStarted} />,
    );
    fireEvent.click(await screen.findByTestId('ft8-setup-start'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('ft8_listener_start'));
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
  });

  it('zero devices renders the plug-in notice with a Refresh button', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd?: string) =>
      cmd === 'ft8_list_devices' ? [] : undefined,
    );
    render(<Ft8StripSetup snapshot={snap({ availableDevices: [] })} />);
    expect(await screen.findByTestId('ft8-setup-zero-devices')).toBeTruthy();
    expect(screen.getByTestId('ft8-setup-refresh')).toBeTruthy();
  });

  it('wsjtx-absent renders install copy with Retry wired to onRetry', async () => {
    const onRetry = vi.fn();
    render(
      <Ft8StripSetup
        snapshot={snap({ service: { axis: 'blocked', reason: 'wsjtx-absent' } })}
        onRetry={onRetry}
      />,
    );
    fireEvent.click(await screen.findByTestId('ft8-setup-retry'));
    expect(onRetry).toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run to verify failure**

Run: `pnpm vitest run src/ft8ui/Ft8StripSetup.test.tsx`
Expected: FAIL, cannot resolve `./Ft8StripSetup`.

- [ ] **Step 4: Implement `Ft8StripSetup`**

Structure (borrow arm-selection logic and copy verbatim from `Ft8SetupSurface.tsx:588-728`, compacted; keep all its data-testids that tests above use):

```tsx
// src/ft8ui/Ft8StripSetup.tsx
// Compact in-strip FT-8 setup: one dropdown + live meter + Start. Replaces the
// full-body Ft8SetupSurface (deleted in the same change series). The blocked
// arms (zero devices / wsjtx-absent / unsupported-sample-rate) render compact
// single-line notices with their existing testids and actions.
import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../controls';
import { RigControlSection } from '../rig/RigControlSection';
import { useDeviceMeterPoll } from './useDeviceMeterPoll';
import type { AudioDeviceChoice, Ft8Snapshot } from './ft8Types';
import './Ft8StripSetup.css';

export interface Ft8StripSetupProps {
  snapshot: Ft8Snapshot;
  onStarted?: () => void;
  onRetry?: () => void;
}

export function Ft8StripSetup({ snapshot, onStarted, onRetry }: Ft8StripSetupProps) {
  const [devices, setDevices] = useState<AudioDeviceChoice[] | null>(snapshot.availableDevices);
  const [busy, setBusy] = useState(false);
  const [catOpen, setCatOpen] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);

  const loadDevices = useCallback(async () => {
    try { setDevices(await invoke<AudioDeviceChoice[]>('ft8_list_devices')); }
    catch { /* keep the last list; the meter arm surfaces device errors */ }
  }, []);
  useEffect(() => { void loadDevices(); }, [loadDevices]);

  const selected = useMemo(
    () => (devices ?? []).find((d) => d.humanName === snapshot.configuredDeviceName) ?? null,
    [devices, snapshot.configuredDeviceName],
  );
  // Meter follows the persisted pick; paused while a handover is in flight.
  const { meter, error: meterError, stopAndAwait } = useDeviceMeterPoll(
    selected?.stableId ?? { kind: 'cardIdHash', value: '' },
    !busy && selected != null,
  );

  const pick = useCallback(async (humanName: string) => {
    const device = (devices ?? []).find((d) => d.humanName === humanName);
    if (!device) return;
    setBusy(true);
    try {
      await stopAndAwait();                       // release the meter handle first
      await invoke('ft8_set_device', { stableId: device.stableId });
    } finally { setBusy(false); }
  }, [devices, stopAndAwait]);

  const start = useCallback(async () => {
    setBusy(true); setStartError(null);
    try { await invoke('ft8_listener_start'); onStarted?.(); }
    catch (e) { setStartError(String(e)); }
    finally { setBusy(false); }
  }, [onStarted]);

  const reason = snapshot.service.axis === 'blocked' ? snapshot.service.reason : null;
  if (reason === 'wsjtx-absent') {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-wsjtx-absent">
        FT-8 decoder missing: install the wsjtx package, then
        <Button data-testid="ft8-setup-retry" onClick={() => onRetry?.()}>Retry</Button>
      </div>
    );
  }
  if (reason === 'unsupported-sample-rate') {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-bad-rate">
        This input cannot capture at 12 kHz: pick a different device below.
      </div>
    );
  }
  if ((devices ?? []).length === 0) {
    return (
      <div className="ft8ss ft8ss--notice" data-testid="ft8-setup-zero-devices">
        No capture-capable soundcard found: plug in the rig interface, then
        <Button data-testid="ft8-setup-refresh" onClick={loadDevices}>Refresh</Button>
      </div>
    );
  }

  return (
    <div className="ft8ss" data-testid="ft8-strip-setup">
      <span className="ft8ss__label">audio input</span>
      <select
        className="ft8ss__select"
        data-testid="ft8-setup-device-select"
        value={selected?.humanName ?? ''}
        disabled={busy}
        onChange={(e) => void pick(e.target.value)}
      >
        {selected == null && <option value="" disabled>Choose input…</option>}
        {(devices ?? []).map((d) => (
          <option key={d.humanName} value={d.humanName}>{d.humanName}</option>
        ))}
      </select>
      <MeterBar rmsDbfs={meter?.rmsDbfs ?? null} state={meter?.state ?? null} error={meterError} />
      <Button
        data-testid="ft8-setup-start"
        disabled={busy || selected == null}
        onClick={() => void start()}
      >
        ▶ Start listening on {snapshot.band}
      </Button>
      <button type="button" className="ft8ss__cat-toggle" data-testid="ft8-setup-cat-toggle"
        onClick={() => setCatOpen((v) => !v)}>
        rig control (CAT) · optional {catOpen ? '⌃' : '⌄'}
      </button>
      {startError && <span className="ft8ss__err">{startError}</span>}
      {catOpen && (
        <div className="ft8ss__cat" data-testid="ft8-setup-cat">
          <RigControlSection storageKeyPrefix="ft8" />
        </div>
      )}
    </div>
  );
}
```

Plus a small `MeterBar` presentational component in the same file (a div-scaled bar; `state === 'in-use'` renders "in use by another app", `error` renders `error.detail ?? error.kind`) and `Ft8StripSetup.css` (single flex row, `flex-wrap: wrap`, `gap: 12px`; `.ft8ss__cat { flex-basis: 100%; }` so the optional CAT panel wraps to its own line inside the strip without moving the row above).

Check `RigControlSection`'s real import path and props first (grep its use in `Ft8SetupSurface.tsx:646-680`); mirror exactly what the old surface passed, including the `ref`/`commitNow()` + `ft8_cat_probe` Test CAT button if the section does not embed one itself.

- [ ] **Step 5: Run to verify pass**

Run: `pnpm vitest run src/ft8ui/Ft8StripSetup.test.tsx`: Expected: PASS.
Also run: `pnpm vitest run src/ft8ui/Ft8SetupSurface.test.tsx`: Expected: still PASS (hook extraction is behavior-neutral).

- [ ] **Step 6: Commit (parent session)**

```bash
git add src/ft8ui/useDeviceMeterPoll.ts src/ft8ui/Ft8StripSetup.tsx src/ft8ui/Ft8StripSetup.css src/ft8ui/Ft8StripSetup.test.tsx src/ft8ui/Ft8SetupSurface.tsx
git commit -m "feat(ft8ui): compact in-strip setup form with OS-convention device select"
```

---

### Task 2: Mount setup in the strip; strip header gains ▶/■

**Files:**
- Modify: `src/ft8ui/LiveBandStrip.tsx` (NonLiveBody arms at lines 488-592; header at 269-388)
- Test: `src/ft8ui/LiveBandStrip.test.tsx` (extend)

**Interfaces:**
- Consumes: `Ft8StripSetup` from Task 1; existing `LiveBandStripProps` (`snapshot`, `uiState`, `decodesRing`, `operatorGrid`, `blockingSessionMode`, `onOpenFullSetup`, `nowMs`).
- Produces: `LiveBandStripProps` gains `onRehydrate?: () => void` (threaded to `Ft8StripSetup.onStarted`/`onRetry`; the panel passes `ft8.rehydrate`). `onOpenFullSetup` is REMOVED from the interface (no full setup exists to open). Header exposes `data-testid="ft8-strip-startstop"`.

- [ ] **Step 1: Write failing tests**: extend `LiveBandStrip.test.tsx` with: (a) `needs-setup` state renders `ft8-strip-setup` (the Task 1 form) inside the strip body instead of the "Open setup →" notice; (b) `device-lost` state renders the same form (replacing the "pick another input" dead-end link); (c) in a live state (`decoding` fixture) the header renders `ft8-strip-startstop` with label "■ Stop" and clicking it invokes `ft8_listener_stop`; (d) in `off` state with a configured device the header button reads "▶ Start" and invokes `ft8_listener_start`. Reuse the file's existing snapshot fixtures; mock `invoke` with the no-arg-tolerant pattern from Task 1.
- [ ] **Step 2: Run to verify failure**: `pnpm vitest run src/ft8ui/LiveBandStrip.test.tsx`.
- [ ] **Step 3: Implement**: In `NonLiveBody`: replace the `needs-setup` arm (lines 546-567) and the `device-lost` arm's "pick another input" button (line ~578) with `<Ft8StripSetup snapshot={snapshot} onStarted={onRehydrate} onRetry={onRehydrate} />`; replace the `off`-state no-device CTA (line ~517) the same way. Delete the header `setup` button (line ~371). Add the header start/stop button next to the collapse control: derive `running` from `LIVE_BODY_STATES.has(state) || state === 'waiting-first-slot'` and call `ft8_listener_start`/`ft8_listener_stop` with a local `toggling` guard (mirror `AppShell.tsx:1087-1108`). Remove `onOpenFullSetup` from `LiveBandStripProps` and all four internal call sites (lines 371, 517, 562, 578).
- [ ] **Step 4: Run to verify pass**: `pnpm vitest run src/ft8ui/LiveBandStrip.test.tsx`.
- [ ] **Step 5: Commit (parent)**: `feat(ft8ui): setup lives in the strip; header start/stop control`

---

### Task 3: Delete the panel takeover; map + rail always mounted

**Files:**
- Modify: `src/catalog/StationFinderPanel.tsx` (states at 205-217; ternary at 505-594)
- Delete: `src/ft8ui/Ft8SetupSurface.tsx`, `src/ft8ui/Ft8SetupSurface.css`, `src/ft8ui/Ft8SetupSurface.test.tsx`
- Modify: `src/catalog/StationFinderPanel.css` (remove `.station-finder__setupbody*` rules)
- Test: `src/catalog/StationFinderPanel.ft8mount.test.tsx` (extend)

**Interfaces:**
- Consumes: Task 2's `LiveBandStrip` (no `onOpenFullSetup`; new `onRehydrate`).
- Produces: `StationFinderPanel` renders `station-finder__body` (map + rail) + `LiveBandStrip` UNCONDITIONALLY. States `forceSetup`, `setupDismissed`, `needsSetup`, `setupActive` no longer exist.

- [ ] **Step 1: Write the failing test**: add to `ft8mount.test.tsx` a needs-setup case: override the `ft8_listener_snapshot` stub with `service: { axis: 'blocked', reason: 'needs-device-selection' }, configuredDeviceName: null`, then assert ALL THREE mount simultaneously: `screen.getByTestId('map-layer-control')` (map), `screen.getByTestId('rail-tab-station')` (rail), `screen.getByTestId('ft8-strip')` (strip), and `screen.queryByTestId('station-finder-setup-body')` is null.
- [ ] **Step 2: Run to verify failure**: `pnpm vitest run src/catalog/StationFinderPanel.ft8mount.test.tsx`.
- [ ] **Step 3: Implement**: In `StationFinderPanel.tsx`: delete the four setup states and their effect (lines ~205-217), the entire `setupActive ? (...) : (...)` ternary keeping only the else-branch content (map + rail + strip, un-nested from the fragment), the `Ft8SetupSurface` import, and pass `onRehydrate={ft8.rehydrate}` to `LiveBandStrip`. Delete the three `Ft8SetupSurface` files. Remove `.station-finder__setupbody`, `.station-finder__setupbody-scroll`, `.station-finder__setup-back` rules from the CSS. Grep for stragglers: `grep -rn "Ft8SetupSurface\|setupbody\|onOpenFullSetup\|forceSetup" src/` must return nothing.
- [ ] **Step 4: Run to verify pass**: `pnpm vitest run src/catalog/StationFinderPanel.ft8mount.test.tsx src/catalog/StationFinderPanel.test.tsx` (the second file may assert old behavior: update any test that asserted the takeover).
- [ ] **Step 5: Commit (parent)**: `fix(catalog): map+rail always mounted; FT-8 setup takeover deleted (tuxlink-6i0ie)`

After this commit, push and **open the draft PR** (`gh pr create --draft --base main`) so CI compiles all subsequent Rust work.

---

### Task 4: FT-8 heard layer on the map

**Files:**
- Create: `src/map/Ft8HeardLayer.tsx`
- Modify: `src/catalog/StationFinderMap.tsx` (props 39-57; layer housing 338-424; children 378-397)
- Modify: `src/catalog/StationFinderPanel.tsx` (thread `decodesRing`)
- Modify: `src/catalog/StationFinderPanel.css` (marker/z-index additions if any keep the one-z-index-per-rule invariant)
- Test: `src/map/Ft8HeardLayer.test.tsx`, extend `src/catalog/StationFinderMap.test.tsx`

**Interfaces:**
- Consumes: `aggregateLiveDecodes(ring, nowMs): LiveDecodeRow[]` exported from `src/catalog/LiveDecodesTab.tsx:62` (`LiveDecodeRow = { call; grid: string | null; bestSnrDb; count; band; lastSlotUtcMs }`); `gridToLatLon` from `src/forms/position/maidenhead.ts`; `useLeafletMap()` + `useLeafletLayerGroup(map)` from `src/map/`.
- Produces:
  ```typescript
  // src/map/Ft8HeardLayer.tsx
  export const SNR_HOT_DB = -10;   // openness ramp thresholds for per-station SNR
  export const SNR_WARM_DB = -17;
  export interface Ft8HeardLayerProps {
    rows: LiveDecodeRow[];         // pre-aggregated by the caller
    enabled: boolean;
    renderer?: L.Renderer;         // the panel map's shared SVG renderer
  }
  export function Ft8HeardLayer(props: Ft8HeardLayerProps): null
  ```
  `StationFinderMapProps` gains `decodesRing?: SlotRecord[]`. The layer control gains a button `data-testid="map-layer-ft8"` labeled `FT-8 heard`, default ON.

- [ ] **Step 1: Write the failing test**: `Ft8HeardLayer.test.tsx` follows the real-Leaflet-in-jsdom pattern from `StationFinderMap.test.tsx` (clientWidth/Height shim, `L.map` spy, walk `captured.eachLayer`): with two rows (one grid `DN26` SNR −4, one grid `PM74` SNR −19) assert two `L.CircleMarker`s exist at the grid centroids with `fillColor` equal to the hot ramp for −4 (`#ff5470`) and quiet for −19 (`#5c92b3`), and that a gridless row plots nothing. With `enabled: false` assert zero markers. In `StationFinderMap.test.tsx`, assert the `map-layer-ft8` toggle button renders and clicking it removes the heard markers.
- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement**: `Ft8HeardLayer` mirrors `StationLayers` (`StationFinderMap.tsx:189-282`): `useLeafletLayerGroup`, diff-less full rebuild is fine at ≤ 240 ring entries (clear + re-add on `rows` change; rows are already aggregated per call). Marker: `L.circleMarker([ll.lat, ll.lon], { renderer, radius: 4, stroke: false, fillOpacity: 0.9, fillColor: rampFor(row.bestSnrDb) })` with `bindTooltip(\`${row.call} · ${row.bestSnrDb} dB\`)`. Color ramp uses the CSS custom-property VALUES already defined in `StationFinderPanel.css` (`--open-hot: #ff5470`, `--open-warm: #ffcf5c`, `--open-quiet: #5c92b3`); Leaflet needs literals, so export a `rampFor(snrDb)` helper mapping via `SNR_HOT_DB`/`SNR_WARM_DB`. In `StationFinderMap`: add `ft8Visible` state (default true), the layer button beside Gateways/Peers in `station-finder__layers` (housing comments at lines 338-343 anticipate exactly this), compute `rows = useMemo(() => aggregateLiveDecodes(decodesRing ?? [], nowMs), [...])`, and mount `<Ft8HeardLayer rows={rows} enabled={ft8Visible} renderer={rendererRef.current}/>` beside `<StationLayers>`. In the panel, pass `decodesRing={ft8.decodesRing}`.
- [ ] **Step 4: Run to verify pass**: both test files.
- [ ] **Step 5: Commit (parent)**: `feat(catalog): FT-8 heard stations plotted on the finder map (spec L3 traffic map)`

---

### Task 5: FT-8 heat layer (grid-square choropleth, default off)

**Files:**
- Create: `src/map/Ft8HeatLayer.tsx`
- Modify: `src/catalog/StationFinderMap.tsx`
- Test: `src/map/Ft8HeatLayer.test.tsx`

**Interfaces:**
- Consumes: `LiveDecodeRow[]` (same rows as Task 4); `gridToLatLon`; `useLeafletLayerGroup`.
- Produces: `export function Ft8HeatLayer({ rows, enabled }: { rows: LiveDecodeRow[]; enabled: boolean }): null` and `export function gridSquareBounds(grid4: string): [[number, number], [number, number]] | null` (the 4-char square's SW/NE corners: 2° lon × 1° lat). Layer button `data-testid="map-layer-ft8heat"`, label `FT-8 heat`, default OFF.

- [ ] **Step 1: Write the failing test**: unit-test `gridSquareBounds('DN26')` returns the exact rectangle (lon [-112,-110], lat [46,47]: derive from maidenhead math and assert literally); component test (jsdom Leaflet pattern) with rows in two squares (3 stations in DN26, 1 in PM74) asserts two `L.Rectangle` layers whose `fillOpacity` for DN26 > PM74 (density-scaled `0.15 + 0.55 * count/maxCount`) and zero layers when `enabled: false`.
- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement**: bucket rows by `row.grid.slice(0, 4).toUpperCase()` (skip null grids), `L.rectangle(gridSquareBounds(sq), { stroke: false, fillColor: '#ff5470', fillOpacity: scaled })` on the layer group. No canvas, no new dependency: rectangles render on the map's default SVG path renderer, safe under Pi software-GL.
- [ ] **Step 4: Run to verify pass.**
- [ ] **Step 5: Commit (parent)**: `feat(catalog): FT-8 heat layer as grid-square choropleth (spec L5)`

---

### Task 6: Evidence corroboration module (pure math, TDD)

**Files:**
- Create: `src/catalog/ft8Evidence.ts`
- Create: `src/catalog/__fixtures__/evidence/basic.json` (shared with Rust in Task 12: commit the SAME file content to both fixture dirs)
- Test: `src/catalog/ft8Evidence.test.ts`

**Interfaces:**
- Consumes: `Station` + `stationKey` (`src/catalog/useReachabilityMap.ts` exports `stationKey(s: Station): string`), `SlotRecord`/`DecodeDto` from `src/ft8ui/ft8Types.ts`, `distanceFromGrids` + `kmToMi` from `./distance`, `gridToLatLon` null-guard for validity.
- Produces:
  ```typescript
  export const EVIDENCE_RECENCY_MS = 30 * 60 * 1000;
  export const EVIDENCE_SNR_MIN_DB_DEFAULT = -24;
  export const EVIDENCE_RADIUS_FACTOR = 0.15;
  export const EVIDENCE_RADIUS_MIN_MI = 50;
  export const EVIDENCE_RADIUS_MAX_MI = 750;

  export interface EvidenceOptions {
    nowMs: number;
    snrMinDb: number;              // caller passes the UI threshold
    operatorGrid: string;
  }
  export interface EvidenceResult {
    corroborated: ReadonlySet<string>;   // stationKey set
    sampledBands: string[];              // bands with ≥1 qualifying decode in-window
    considered: number;                  // stations evaluated
  }
  export function corroborateStations(
    stations: Station[], ring: SlotRecord[], opts: EvidenceOptions,
  ): EvidenceResult
  export function evidenceRadiusMi(operatorToHeardMi: number): number  // clamp(0.15 * d, 50, 750)
  ```
  Semantics exactly per spec §2: decode qualifies iff it has a grid, `opts.nowMs - slotUtcMs <= EVIDENCE_RECENCY_MS`, `snrDb >= opts.snrMinDb`; a station is corroborated iff some qualifying decode D has `band(D)` matching a `channel.band` of the station AND `distanceFromGrids(D.grid, station.grid)` in miles `<= evidenceRadiusMi(distanceFromGrids(operatorGrid, D.grid) in miles)`. Note `SlotRecord.band` is the slot's band; decodes inherit it.

- [ ] **Step 1: Write the failing test**: table-driven from `__fixtures__/evidence/basic.json`. The fixture (author it now; it is the cross-language contract):

```json
{
  "operatorGrid": "DN17",
  "nowMs": 1000000000,
  "snrMinDb": -24,
  "stations": [
    { "key": "corroborated-near", "grid": "DN26", "bands": ["20m"] },
    { "key": "wrong-band", "grid": "DN26", "bands": ["40m"] },
    { "key": "too-far", "grid": "EM48", "bands": ["20m"] },
    { "key": "stale-only", "grid": "DN26", "bands": ["20m"] }
  ],
  "decodes": [
    { "grid": "DN36", "band": "20m", "snrDb": -8, "slotUtcMs": 999940000 },
    { "grid": "EM10", "band": "20m", "snrDb": -20, "slotUtcMs": 999940000 },
    { "grid": "DN27", "band": "20m", "snrDb": -12, "slotUtcMs": 998000000 }
  ],
  "expectCorroborated": ["corroborated-near"],
  "expectSampledBands": ["20m"]
}
```

  (Before finalizing the fixture, VERIFY the distances with `distanceFromGrids` in a scratch test: the decode at DN36 must be within `evidenceRadiusMi` of DN26 and outside it for EM48, and the third decode must be older than 30 min relative to `nowMs`; adjust grids until the four cases discriminate. The final numbers in the committed fixture are ground truth for Rust too.) Also unit-test `evidenceRadiusMi`: `evidenceRadiusMi(100) === 50` (floor), `evidenceRadiusMi(1500) === 225`, `evidenceRadiusMi(10000) === 750` (cap).
- [ ] **Step 2: Run to verify failure.** `pnpm vitest run src/catalog/ft8Evidence.test.ts`
- [ ] **Step 3: Implement**: straightforward double loop (≤ 240 slots × stations; memoize per-decode operator distance). Build minimal `Station` objects in the test from the fixture (only `grid`, `channels[].band`, and a key are consulted; construct with a helper).
- [ ] **Step 4: Run to verify pass.**
- [ ] **Step 5: Commit (parent)**: `feat(catalog): FT-8 evidence corroboration math (spec constants, fixture-backed)`

---

### Task 7: Evidence filter UI (toggle + threshold + ghosting + note chip)

**Files:**
- Modify: `src/catalog/StationFinderPanel.tsx` (state + persistence + wiring)
- Modify: `src/catalog/StationFinderMap.tsx` (ghost styling, layer-box controls, note chip)
- Modify: `src/catalog/StationFinderPanel.css`
- Test: extend `src/catalog/StationFinderMap.test.tsx` + `src/catalog/StationFinderPanel.test.tsx`

**Interfaces:**
- Consumes: Task 6's `corroborateStations` / constants; Task 4's `decodesRing` threading.
- Produces: `StationFinderMapProps` gains
  ```typescript
  evidence?: {
    enabled: boolean;
    onToggle: () => void;
    snrMinDb: number;
    onSnrMinChange: (db: number) => void;
    ghostedKeys: ReadonlySet<string>;   // empty when disabled
    note: string | null;                // "evidence: 2 of 5 gateways corroborated (20m) · SNR ≥ -18 · last 30 min"
  };
  ```
  `PersistedFinderView` gains `evidenceOn: boolean` and `evidenceSnrMinDb: number` (validated on read like the existing fields, `StationFinderPanel.tsx:118-160`). Ghost style: `pinStyle` result overridden with `fillOpacity: 0.2, opacity: 0.2` for ghosted keys; ghosted pins stay clickable.

- [ ] **Step 1: Write failing tests**: map test: with `evidence.enabled` and `ghostedKeys` containing one of two stations, that station's `CircleMarker.options.fillOpacity` is `0.2` and the other is unchanged; the layer box renders `map-evidence-toggle` and a `map-evidence-snr` input; the note chip text renders verbatim from `evidence.note`. Panel test: toggling evidence persists `evidenceOn` in `localStorage['tuxlink:station-finder:view']`.
- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement**: Panel: `const evidenceResult = useMemo(() => evidenceOn ? corroborateStations(visible, ft8.decodesRing, { nowMs: Date.now(), snrMinDb, operatorGrid }) : null, [...])`; `ghostedKeys = visible stations whose stationKey ∉ corroborated`; note string built from `corroborated.size`, `visible.length`, `sampledBands.join('/')`, `snrMinDb`, and the 30 min constant. Map: render toggle + a compact `<input type="range" min="-24" max="0" step="3">` in `station-finder__layers` (keep exactly one `z-index` in any touched overlay rule: the CSS test `StationFinderPanel.css.test.ts` guards this), note chip as `station-finder__evidence-note` positioned like the layer box. Threshold slider only visible when the toggle is on.
- [ ] **Step 4: Run to verify pass** (both files).
- [ ] **Step 5: Commit (parent)**: `feat(catalog): FT-8 evidence filter with ghosting and honest note chip`

---

### Task 8: Rust channels API (`ListingMode::VaraFm` + fetch/join/cache)

**Files:**
- Modify: `src-tauri/src/catalog/stations.rs` (enum 24-43, `listing_file` 45-53, `uses_history_hours` 57-59, `label`, `Gateway` struct 101-115)
- Create: `src-tauri/src/catalog/channels_api.rs`
- Modify: `src-tauri/src/catalog/commands.rs` (fetch flow 98-168), `src-tauri/src/winlink/credentials.rs` (new key fns), `src-tauri/src/lib.rs` (manage cache + register commands, near 1945-1952 / 3292-3301)
- Create: `src-tauri/tests/fixtures/catalog/channels-status-sample.json`
- Test: inline `#[cfg(test)]` in `channels_api.rs` + extend `src-tauri/tests/catalog_listing_parse.rs`

**Interfaces:**
- Consumes: `reqwest` client pattern from `commands.rs:84-135` (user-agent, 30 s timeout, https_only); `StationsCache` pattern (`stations_cache.rs`) and `stations_disk.rs` atomic-write; keyring `EntryLike`/factory seam (`credentials.rs:58-68`, service-codes fns at 353-415 as the template).
- Produces (exact, later tasks and TS depend on the wire shape):
  ```rust
  // stations.rs
  pub enum ListingMode { VaraHf, Packet, ArdopHf, Pactor, RobustPacket, VaraFm }  // serde kebab-case -> "vara-fm"
  // listing_file becomes Option<&'static str>; VaraFm => None (no text listing exists; API-only)

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct ChannelDetail {
      pub frequency_khz: f64,          // DIAL kHz (center Hz - 1500, / 1000)
      pub bandwidth_hz: Option<u32>,   // 500 / 2300 / 2750; None when unknown
      pub mode: ListingMode,
      pub operating_hours: Option<String>,
      pub grid: Option<String>,
  }
  // Gateway gains: #[serde(default)] pub channel_details: Vec<ChannelDetail>,

  // channels_api.rs
  pub const CHANNELS_API_URL: &str = "https://api.winlink.org/gateway/status.json";
  pub const DEFAULT_CHANNELS_API_KEY: &str = "1880278F11684B358F36845615BD039A"; // Pat's public WDT AccessKey (la5nta/pat internal/cmsapi)
  pub fn mode_code_to_detail(code: u32) -> Option<(ListingMode, Option<u32>)>;
  //   50 => (VaraHf, Some(2300)), 53 => (VaraHf, Some(500)), 54 => (VaraHf, Some(2750)),
  //   51 | 52 => (VaraFm, None), others => None (verify/extend from the captured fixture)
  pub fn parse_channels_feed(json: &str) -> Result<ChannelsFeed, UiError>;   // ChannelsFeed = HashMap<String /*CALLSIGN with SSID, uppercased*/, Vec<ChannelDetail>>
  pub fn join_channels(listing: &mut StationListing, feed: &ChannelsFeed);   // match on gateway.callsign uppercased
  pub fn synthesize_vara_fm_listing(feed: &ChannelsFeed, fetched_at_ms: u64) -> StationListing;
  pub async fn fetch_channels_feed(key: &str, service_codes: &str) -> Result<ChannelsFeed, UiError>;
  // credentials.rs
  pub fn channels_api_key_read() -> String;      // keyring account "winlink-channels-api-key"; falls back to DEFAULT_CHANNELS_API_KEY
  pub fn channels_api_key_write(key: &str) -> Result<(), CredError>;         // same *_with_factory test seam as service codes
  ```
  New Tauri commands `catalog_get_channels_api_key` / `catalog_set_channels_api_key` (mirror `catalog_get_service_codes` at `commands.rs:170-183`). Channels feed cached in a `ChannelsCache` modeled on `StationsCache` (TTL **60 min**, min-refetch 15 min, disk `channels-feed-cache.json`, stale-on-error), managed in `lib.rs` beside the stations cache.

- [ ] **Step 1: Capture the fixture**: one real POST (internet, no RF):
  `curl -s -X POST 'https://api.winlink.org/gateway/status.json' --data 'Mode=anyall&ServiceCodes=PUBLIC&key=1880278F11684B358F36845615BD039A&format=json' | head -c 200000 > src-tauri/tests/fixtures/catalog/channels-status-sample.json`
  then hand-trim to ~15 gateways that include: a VARA 2300 (code 50), a VARA 500 (53), a VARA 2750 (54), a VARA FM (51/52), an ARDOP entry, and one gateway present in the text-listing fixtures (matching callsign for the join test). RECORD in a fixture-header comment file (`channels-status-sample.README`) the capture date and the verbatim field names observed (`Callsign`, `BaseCallsign`, `Channels[].Frequency/Mode/OperatingHours/Gridsquare/ServiceCode`...); if the live shape differs from this plan's assumptions (center-Hz frequency, code table), THE FIXTURE WINS and the mapping fns adjust to it.
- [ ] **Step 2: Write failing Rust tests** (inline `mod tests` in `channels_api.rs`, compiled by CI):
  test `mode_code_to_detail` table; `parse_channels_feed` on the fixture (`include_str!`) asserts a known gateway's channel count, a dial conversion (assert one channel's `frequency_khz` equals the fixture's center Hz minus 1500, divided by 1000), and VARA FM channels landing under `ListingMode::VaraFm`; `join_channels` attaches `channel_details` to the matching text-listing gateway and leaves others empty; `synthesize_vara_fm_listing` yields `parsed_ok: true`, mode `VaraFm`, gateways only for callsigns with VaraFm channels; serde wire-shape test: `serde_json::to_value(ListingMode::VaraFm) == json!("vara-fm")` and a `ChannelDetail` round-trip in camelCase.
- [ ] **Step 3: Implement**: enum + `listing_file() -> Option<&str>` refactor (update the two call sites in `stations.rs`/`commands.rs` to skip text fetch when `None`); `channels_api.rs` per the interface block; `fetch_channels_feed` uses the `commands.rs:84-135` client pattern (POST form, 30 s, https_only, user-agent). In `catalog_fetch_stations` (`commands.rs:142-168`): after the per-mode text-listing loop, fetch the channels feed through `ChannelsCache::get_or_fetch` (never fails the whole call: on error, skip the join and serve text-only, logging once), `join_channels` into every fetched listing, and when `modes` contains `VaraFm` push `synthesize_vara_fm_listing`. Add key read/write fns + commands + `ChannelsCache` (copy `stations_cache.rs` structure with the listing type swapped; keep `MockClock` tests for TTL + stale-on-error).
- [ ] **Step 4: Verify via CI**: push; the draft PR runs `cargo test` + clippy on both arches. Do NOT run cargo locally. Check the run by head SHA: `gh run list --commit $(git rev-parse HEAD)` and confirm conclusion success.
- [ ] **Step 5: Commit (parent)**: `feat(catalog): Winlink channels JSON API with bandwidth + VARA FM (tuxlink-nkzng)` (commit precedes the CI check; fix-forward on red).

---

### Task 9: TS model + bandwidth filter chips (both locations, one state)

**Files:**
- Modify: `src/catalog/stationTypes.ts` (ListingMode union line 8, `Gateway` iface, new `BandwidthClass`), `src/catalog/stationModel.ts` (`Channel` 11-17, `aggregateStations` 77-79, `stationMatchesBandMode`), `src/catalog/StationFinderControls.tsx` (FilterMode line 12, FILTER_MODES 14-18, chip row insert point after line 225), `src/catalog/StationFinderPanel.tsx` (FILTER_MODES line ~90, state + persistence), `src/catalog/StationFinderMap.tsx` (layer-box mirror), `src/catalog/channelGrouping.ts` (`channelToDial` vara-fm arm), `src/catalog/StationFinderPanel.css` (`--m-vara-fm` swatch)
- Test: `src/catalog/stationModel.test.ts`, `src/catalog/StationFinderControls.test.tsx`, `src/catalog/StationFinderMap.test.tsx`

**Interfaces:**
- Consumes: Task 8's wire shapes (`Gateway.channelDetails?: ChannelDetail[]` camelCase, `ListingMode` now including `'vara-fm'`).
- Produces:
  ```typescript
  // stationTypes.ts
  export type ListingMode = 'vara-hf' | 'packet' | 'ardop-hf' | 'pactor' | 'robust-packet' | 'vara-fm';
  export type BandwidthClass = '500' | '2300' | '2750';
  export const BANDWIDTH_CLASSES: BandwidthClass[] = ['500', '2300', '2750'];
  export function bandwidthClass(hz: number | null | undefined): BandwidthClass | null;
  export interface ChannelDetail { frequencyKhz: number; bandwidthHz: number | null; mode: ListingMode; operatingHours: string | null; grid: string | null; }
  // Gateway gains channelDetails?: ChannelDetail[]
  // stationModel.ts
  export interface Channel { mode: ListingMode; frequencyKhz: number; ssid?: string; band: Band | null; bandwidthHz?: number | null; }
  export function stationMatchesFilters(station: Station, bands: Set<Band>, modes: Set<FilterMode>, bandwidths: Set<BandwidthClass>): boolean;
  // StationFinderControlsProps gains: enabledBandwidths: Set<BandwidthClass>; onToggleBandwidth: (bw: BandwidthClass) => void;
  // StationFinderMapProps gains: bandwidthMirror?: { enabled: Set<BandwidthClass>; onToggle: (bw: BandwidthClass) => void };
  ```
  **Unknown-bandwidth rule (load-bearing):** a channel with `bandwidthHz == null` (text-listing-only data) passes EVERY bandwidth filter; the chips only subtract channels with a known non-matching bandwidth. `FilterMode` (`'vara-hf' | 'ardop-hf' | 'packet'`) gains `'vara-fm'`; `PersistedFinderView` gains `bandwidths: BandwidthClass[]`.

- [ ] **Step 1: Write failing tests**: `stationModel`: `aggregateStations` prefers `channelDetails` when present (one Channel per detail, carrying `bandwidthHz` + per-channel mode) and falls back to `frequenciesKhz` expansion otherwise; `stationMatchesFilters` drops a station whose only 20m VARA channel is 500 Hz when bandwidths = {2300, 2750}, keeps a null-bandwidth channel under any subset, and keeps the station if ANY channel passes. Controls: BW chip group renders three chips between modes and searchgroup (`data-testid="bw-chip-500"` etc.), toggling calls `onToggleBandwidth`; a `vara-fm` mode chip renders. Map: `bandwidthMirror` renders the same three checkboxes in the layer box and toggling fires the SAME handler (assert one shared `vi.fn`).
- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement**: model changes per the interface block; `channelToDial` gains the `'vara-fm'` arm mapping to RadioMode `'vara-fm'` (RadioMode already includes it, `src/favorites/types.ts:11`; AppShell prefill routing already handles vara-fm panes at `AppShell.tsx:1279, 2089`); panel owns `enabledBandwidths` state seeded from persistence, passes it to Controls AND to the map's `bandwidthMirror`; replace the `stationMatchesBandMode` call site in the panel's `visible` memo with `stationMatchesFilters`; CSS adds `--m-vara-fm: #79b8ff` and a `station-finder__sw--vara-fm` swatch (triangle shape like VARA HF, lighter hue).
- [ ] **Step 4: Run to verify pass**: the three test files plus `pnpm vitest run src/catalog` (the directory: model changes ripple).
- [ ] **Step 5: Commit (parent)**: `feat(catalog): bandwidth filter chips in both locations, one shared state; VARA FM end-to-end`

---

### Task 10: Frequency hero + channel-row frequency/bandwidth badges

**Files:**
- Modify: `src/catalog/StationRail.tsx` (StationTabPane, insert hero after the aim hero ~line 420), `src/catalog/BandMatrix.tsx` (channel rows), `src/catalog/StationFinderPanel.css`
- Test: `src/catalog/StationRail.test.tsx`, `src/catalog/BandMatrix.test.tsx`

**Interfaces:**
- Consumes: `Channel.frequencyKhz` (kHz), `Channel.bandwidthHz`, `rankedDialsFor` from `src/catalog/ranking.ts` (BandMatrix already imports it; the hero shows the TOP-ranked dial for the current hour).
- Produces: `export function formatDialKhz(khz: number): string` in `src/catalog/stationModel.ts` rendering `7103.5` as `"7,103.5 kHz"`; hero block `data-testid="rail-freq-hero"`; badge element `station-finder__bw-badge station-finder__bw-badge--narrow|--wide` (narrow = 500, wide = 2300/2750, absent when bandwidth null).

- [ ] **Step 1: Write failing tests**: `formatDialKhz` unit cases (`7103.5 -> "7,103.5 kHz"`, `14108 -> "14,108.0 kHz"`); StationRail: selecting a station with channels renders `rail-freq-hero` whose text contains the top-ranked channel's formatted dial and its bandwidth badge; BandMatrix: every channel row shows its formatted `frequencyKhz` and a 500-bandwidth channel gets the `--narrow` badge class while a null-bandwidth channel gets none.
- [ ] **Step 2: Run to verify failure.**
- [ ] **Step 3: Implement**: hero: mono 19px block styled per the approved mock (largest datum in the rail; CSS class `station-finder__freq-hero`); BandMatrix rows append the dial + badge inline (rows already render per-channel: locate the row map inside BandMatrix and add the two spans).
- [ ] **Step 4: Run to verify pass.**
- [ ] **Step 5: Commit (parent)**: `feat(catalog): gateway dial frequency first-class in the rail (tuxlink-hcmfb)`

---

### Task 11: Containment: pin the growth mechanism, then fix panel + strip + WWV

**Files:**
- Modify: `dev/render-harness/harness.tsx` (add a finder view), `src/catalog/StationFinderPanel.css`, `src/ft8ui/LiveBandStrip.css`, `src/ft8ui/DecodeFeed.css` (if needed)
- Test: `src/catalog/StationFinderPanel.css.test.ts` (extend), `src/ft8ui/LiveBandStrip.css.test.ts` (create, same raw-CSS pattern)

**Interfaces:**
- Consumes: the harness IPC shim (`dev/render-harness/harness.tsx` RESPONSES map; supports function values for args-aware fixtures), `snapshot.py <url> <out.png> [w] [h] [wait_ms] [selector]`.
- Produces: harness route `?view=finder&ring=0|240` mounting `StationFinderPanel` inside the same provider stack the ft8mount test uses, with `ft8_listener_snapshot` returning a decoding snapshot whose `ringTail` length follows the `ring` param; PNG evidence pair in `dev/scratch/si-containment/{before,after}-ring{0,240}.png`.

- [ ] **Step 1: EVIDENCE FIRST (spec §4 requires pinning before touching geometry)**: add the finder view to the harness; then:
  ```bash
  pnpm dev &   # if not already serving :1420
  for r in 0 240; do
    WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    python3 dev/render-harness/snapshot.py \
      "http://localhost:1420/dev/render-harness/harness.html?view=finder&ring=$r" \
      dev/scratch/si-containment/before-ring$r.png 1366 800 3000
  done
  ```
  Read both PNGs. Measure `.station-finder`'s rendered height at ring=0 vs ring=240 (use `style-probe.py` or a `clip:` crop). WRITE the finding into the task commit message: which element grows (expected suspects: the strip's live body has no fixed height so `si-feed` content extends it; `.station-finder` is `max-height: 92vh` so it grows content-driven until the clamp, which reads as "the window resizing"). If the PNGs show something ELSE grows, fix THAT and update the steps below accordingly: the measurement is authoritative, not this plan's prediction.
- [ ] **Step 2: Write failing CSS tests**: extend the raw-CSS pattern (`StationFinderPanel.css.test.ts:14-29`): `.station-finder` declares a FIXED height (`height: min(760px, 92vh)`, exactly one `height` declaration, no bare `max-height`-only sizing); new `LiveBandStrip.css.test.ts` asserts the live-body rule (`.si-strip__live` or the actual class from the file) declares `height: 150px` (or the value the evidence supports) and `si-feed`'s rule keeps `overflow` (`auto` or `hidden`) + `min-height: 0`; the WWV rule `.station-finder__offair-nocopy` no longer declares `flex-basis: 100%` and declares a `max-width`.
- [ ] **Step 3: Run to verify failure**: `pnpm vitest run src/catalog/StationFinderPanel.css.test.ts src/ft8ui/LiveBandStrip.css.test.ts`.
- [ ] **Step 4: Implement**: `.station-finder { height: min(760px, 92vh); }` (body + strip then flex inside the fixed box; `.station-finder__body` keeps `flex: 1; min-height: 0` and drops `min-height: 540px` in favor of the small-height media query already at css line 363); strip live body gets the fixed height with the waterfall and feed as `min-height: 0` flex children; `.station-finder__offair-nocopy { flex: 1 1 260px; max-width: 420px; }` so the WWV manual-entry block wraps within the actions row without a full-width blank band (tuxlink-1w0d0).
- [ ] **Step 5: Re-snapshot as `after-ring{0,240}.png`; verify pass**: identical `.station-finder` height in both PNGs; CSS tests green; eyeball the after-PNGs for regressions (map still visible, strip feed scrolls with a clipped last row visible).
- [ ] **Step 6: Commit (parent)**: `fix(catalog): fixed-height panel; decode feeds scroll in place; WWV row contained (tuxlink-6i0ie, tuxlink-1w0d0)` with the measured mechanism stated in the body.

---

### Task 12: MCP parity (evidence + channels over `find_stations`) + docs

**Files:**
- Modify: `src-tauri/tuxlink-mcp-core/src/ports.rs` (StationFilterDto/GatewayDto/StationListDto near 615-651, StationPort 940-947), `src-tauri/tuxlink-mcp-core/src/router.rs` (find_stations 560-584, StationFilterParams 1870-1881), `src-tauri/src/mcp_ports.rs` (station port impl + new evidence fn near MonolithFt8Port 3542-3658), `docs/mcp-knowledge/agents-guide.md` (§Station intelligence, line 48)
- Create: `src-tauri/src/catalog/evidence.rs` + `src-tauri/tests/fixtures/evidence/basic.json` (BYTE-IDENTICAL copy of `src/catalog/__fixtures__/evidence/basic.json` from Task 6)
- Test: inline `mod tests` in `evidence.rs`; extend the mcp-core mock tests (`tuxlink-mcp-core/src/lib.rs`, find_stations mock at 749)

**Interfaces:**
- Consumes: Task 6's fixture as the cross-language contract; Task 8's `ChannelDetail`; the existing grid-distance helpers used by the e7z7d find_stations distance/bearing feature (grep `src-tauri` for the haversine/maidenhead util that feature added and reuse it; do not write a second one); `Ft8ListenerState` snapshot access pattern from `MonolithFt8Port::listener()` (`mcp_ports.rs:3617-3624`).
- Produces:
  ```rust
  // src-tauri/src/catalog/evidence.rs (constants MUST equal the TS module's)
  pub const EVIDENCE_RECENCY_MS: u64 = 30 * 60 * 1000;
  pub const EVIDENCE_SNR_MIN_DB_DEFAULT: i32 = -24;
  pub const EVIDENCE_RADIUS_FACTOR: f64 = 0.15;
  pub const EVIDENCE_RADIUS_MIN_MI: f64 = 50.0;
  pub const EVIDENCE_RADIUS_MAX_MI: f64 = 750.0;
  pub struct EvidenceInput<'a> { pub operator_grid: &'a str, pub now_ms: u64, pub snr_min_db: i32 }
  pub fn evidence_radius_mi(operator_to_heard_mi: f64) -> f64;
  pub fn corroborate(gateways: &[(String /*key*/, String /*grid*/, Vec<String> /*bands*/)],
                     decodes: &[(Option<String> /*grid*/, String /*band*/, i32 /*snr*/, u64 /*slot_ms*/)],
                     input: &EvidenceInput) -> std::collections::HashSet<String>;
  // ports.rs
  pub struct ChannelDto { pub frequency_khz: f64, pub bandwidth_hz: Option<u32>, pub mode: String, pub operating_hours: Option<String> }
  // GatewayDto gains: pub channels: Vec<ChannelDto>, pub ft8_corroborated: Option<bool>
  // StationFilterDto gains: pub bandwidths: Option<Vec<u32>>, pub ft8_evidence: Option<bool>, pub ft8_snr_min_db: Option<i32>
  // StationListDto gains: pub evidence: Option<EvidenceParamsDto>
  pub struct EvidenceParamsDto { pub snr_min_db: i32, pub recency_ms: u64, pub radius_factor: f64, pub radius_min_mi: f64, pub radius_max_mi: f64, pub sampled_bands: Vec<String> }
  ```
  `StationFilterParams` (router) mirrors the three new optional fields with schemars docs; the `find_stations` tool description is extended: "...optionally filtered by channel bandwidth (Hz) and corroborated against live FT-8 decodes (ft8_evidence: true) with an snr floor". Response DTO field additions are additive/optional: existing agent consumers keep working.
- [ ] **Step 1: Write failing Rust tests**: `evidence.rs` loads `tests/fixtures/evidence/basic.json` (serde structs matching the fixture keys) and asserts `expectCorroborated` exactly, plus the same three `evidence_radius_mi` boundary cases as TS. Add a fixture-sync guard test: hash both fixture copies and assert equality is NOT possible cross-crate at runtime, so instead add a repo-level check in the wire-walk task (Task 13 step) comparing the two files with `cmp`.
- [ ] **Step 2: Implement**: evidence fn; monolith station-port impl: when `ft8_evidence == Some(true)`, read the listener snapshot (same `spawn_blocking` pattern as `heard_stations`, `mcp_ports.rs:3652-3658`), map ring decodes + gateways into the tuple form, call `corroborate`, stamp `ft8_corroborated` per gateway and `evidence` params (with `sampled_bands`) on the list DTO; when the listener is unavailable, return `PortError::Unavailable` ONLY if evidence was requested, otherwise serve without it. Bandwidth filter applies AFTER the join (a gateway stays if any channel passes; null bandwidth passes, same rule as TS). Update the mcp-core mock (`lib.rs:749`) for the new DTO fields; update `agents-guide.md`: extend the Station-intelligence tier bullet for `find_stations` and ADD the six `ft8_*` tools (currently undocumented there).
- [ ] **Step 3: Verify via CI**: push; confirm `cargo test` green by head SHA (`gh run list --commit $(git rev-parse HEAD)`).
- [ ] **Step 4: Commit (parent)**: `feat(mcp): find_stations gains channels, bandwidth filter, FT-8 evidence corroboration (agent parity)`

---

### Task 13: Wire-walk, visual gates, suite, ship

**Files:**
- Create: `dev/scratch/si-wirewalk-20260719.md` (evidence log), render-harness PNGs under `dev/scratch/si-redesign/`
- Modify: `.beads/` via bd commands (close/annotate issues)

- [ ] **Step 1: Fixture parity guard**: `cmp src/catalog/__fixtures__/evidence/basic.json src-tauri/tests/fixtures/evidence/basic.json`: MUST be identical; if not, stop and reconcile.
- [ ] **Step 2: Full local frontend suite**: `pnpm vitest run` (full, not scoped; CI is stricter but catch what we can) + `pnpm lint:docs`. All green before the walk.
- [ ] **Step 3: Run the `wire-walk` skill (HARD GATE, `.claude/skills/wire-walk/`)**: the OPERATOR supplies the key user flows greenfield (do NOT draft them for him; anchoring launders blind spots), then trace each flow verbatim to `file:line`. The spec's §7 exit gates are the agent-side minimum checklist alongside whatever flows the operator names: (1) open from ribbon with FT-8 unconfigured: map+rail live, setup in strip, no takeover; (2) pick device in dropdown, meter live, Start from strip; (3) decodes reach map + rail tab + feed with NO panel resize (compare harness `?view=finder&ring=0` vs `ring=240` PNGs); (4) evidence toggle subtracts, note chip states parameters; heat layer toggles; (5) a VARA gateway shows dial + bandwidth hero/badges; BW chips filter from both locations; VARA FM chip lists API-sourced gateways and Use → prefills the VARA FM pane; (6) ■ Stop works from the strip, ribbon mirrors; (7) WWV Refresh row stays contained. Record each gate PASS/FAIL with PNG paths in the wirewalk doc. Anything FAIL: fix before proceeding (ADR 0022: completeness is an invariant; no deferral).
- [ ] **Step 3b: Codex adversarial round on the branch diff** (standing rule for subagent-built work; GPT-5.5, never 5.6 per ADR 0023): custom-prompt stdin pattern from CLAUDE.md, output tee'd to `dev/adversarial/2026-07-19-si-usability-codex.md` (gitignored). Verify a real review landed (`wc -l` roughly 1500+, not a 5-line argparse stub). Disposition every finding before ship; if Codex reports "usage limit...HH:MM", DEFER to that time rather than skip; if Codex is down entirely, run a self-adrev pass instead.
- [ ] **Step 4: Render-harness approval PNGs**: final `?view=finder` decoding-state snapshot at 1366×800 for the operator's visual approval (the standing rule: only WebKitGTK harness PNGs are approval artifacts).
- [ ] **Step 5: Ship**: mark the draft PR ready after CI is green on the final head SHA; merge per the repo's standing merge grant (bare `gh pr merge`, never chained, stated visibly first). Update bd: close `tuxlink-6i0ie`, `tuxlink-hcmfb`, `tuxlink-nkzng`, `tuxlink-1w0d0` with result notes; note on `tuxlink-9obx2` that its blocker has landed.

---

## Self-Review Notes (resolved inline)

- Spec §1-§8 coverage: §1 scope → Tasks 1-13; §2 panel structure → 1-3; §3 map intelligence → 4-7; §4 bandwidth/frequency → 8-10; §5 containment → 11; §6 states untouched → enforced by Tasks 2-3 leaving `deriveUiState`/arms intact; §7 agent parity → 12; §8 verification → 13.
- The evidence math exists twice (TS Task 6, Rust Task 12) by design; the byte-identical shared fixture + Task 13 Step 1 `cmp` guard is the drift control.
- Task 8's mode-code table and center-frequency rule defer to the captured fixture where reality disagrees; that is stated in the task, not left implicit.
