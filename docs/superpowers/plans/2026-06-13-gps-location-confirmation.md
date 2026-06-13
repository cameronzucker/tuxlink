# GPS/Location Wizard — Position Confirmation + Unconditional Diagnostics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the first-run Location step (and Settings → Location) *show and confirm* the operator's position on the offline map, and surface the Linux GPS diagnostics unconditionally so they're reachable before a device enumerates.

**Architecture:** Three phases. (A) Plumb raw lat/lon from the gpsd fix through `Fix` → `PositionArbiter` → `PositionStatusDto` (local-display-only; the on-air broadcast path is untouched). (B) Rework `classifyGpsSources` so dialout/ModemManager triage emit independent of device-presence, gated by the component on "no working fix." (C) Compose `BaseMap` into a dedicated `LocationMap` (draggable manual pin + precise GPS-fix marker + grid square), add a live "acquiring → fixed" readout, and lay out the wizard step full-screen. The shared component means Settings → Location inherits all of it.

**Tech Stack:** Rust (Tauri commands, `serde_json`), TypeScript/React, `react-leaflet`/`leaflet` (existing offline map subsystem), Vitest + `@testing-library/react`, `cargo test`.

**Scope note:** This plan is `tuxlink-yy1m`. The one-click "Fix it for me" pkexec helper (`tuxlink-m9ej`) is a separate, already-specified subsystem (helper binary + PolicyKit policy + spawner); it lands as a stacked pair per the parent design and gets its own plan (`2026-06-13-gps-fix-it-pkexec.md`, written when that work starts). This plan ships the unconditional diagnostics with **copy-paste commands** (already working); the one-click buttons stay disabled until m9ej.

**Design source:** [docs/design/2026-06-13-gps-location-confirmation-design.md](../../design/2026-06-13-gps-location-confirmation-design.md) (amends [2026-06-05-gps-setup-ux-design.md](../../design/2026-06-05-gps-setup-ux-design.md)).

---

## File Structure

**Backend (Rust):**
- Modify `src-tauri/src/position/gpsd.rs` — `Fix` gains `lat`/`lon`; `parse_tpv` retains them; `Fix::test` sets zeros.
- Modify `src-tauri/src/position/arbiter.rs` — `fresh_fix_latlon()` accessor (the `Fix` struct lives in arbiter.rs per the source; see Task A1).
- Modify `src-tauri/src/ui_commands.rs` — `PositionStatusDto` gains `fix_lat`/`fix_lon`; `position_status` populates them.

**Frontend (TypeScript/React):**
- Modify `src/shell/useStatus.ts` — `PositionStatusDto` TS shape gains `fix_lat`/`fix_lon`.
- Modify `src/location/gpsProbes.ts` — `classifyGpsSources` unconditional triage + `noDevice` flag.
- Create `src/location/LocationMap.tsx` — offline map for location setup (draggable pin + GPS-fix marker + grid square), composing `BaseMap`.
- Create `src/location/LocationMap.test.tsx`.
- Modify `src/location/useLocationConfig.ts` — poll `position_status`; expose `gpsReady`, `fixLat`, `fixLon`, `uiGrid`.
- Modify `src/location/GpsSourcePicker.tsx` — embed `LocationMap`, live readout, diagnostics gating, `noDevice` card.
- Modify `src/location/GpsSourcePicker.test.tsx` / `gpsProbes.test.ts` — extend.
- Modify `src/wizard/StepLocation.tsx` + `src/wizard/wizard.css` — full-screen map+rail layout.
- Modify `src/location/GpsSourcePicker.css` — diagnostics/readout/map styles.

---

## Phase A — Backend lat/lon plumbing

### Task A1: `Fix` carries lat/lon; `parse_tpv` retains them

**Files:**
- Modify: `src-tauri/src/position/arbiter.rs` (the `Fix` struct + `Fix::test`)
- Modify: `src-tauri/src/position/gpsd.rs` (`parse_tpv`)
- Test: `src-tauri/src/position/gpsd.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test** (add to `gpsd.rs` test module)

```rust
#[test]
fn parse_tpv_retains_lat_lon() {
    let line = r#"{"class":"TPV","mode":3,"lat":48.143,"lon":11.608}"#;
    let fix = parse_tpv(line).unwrap();
    assert_eq!(fix.grid, "JN58td");
    assert!((fix.lat - 48.143).abs() < 1e-9, "lat retained");
    assert!((fix.lon - 11.608).abs() < 1e-9, "lon retained");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parse_tpv_retains_lat_lon`
Expected: FAIL — `no field 'lat' on type 'Fix'` (compile error).

- [ ] **Step 3: Add the fields** — in `arbiter.rs`, extend `Fix` and `Fix::test`:

```rust
#[derive(Debug, Clone)]
pub struct Fix {
    pub grid: String,
    pub lat: f64,
    pub lon: f64,
    pub received: std::time::Instant,
}
impl Fix {
    #[cfg(test)]
    pub fn test(grid: &str) -> Self {
        Self { grid: grid.into(), lat: 0.0, lon: 0.0, received: std::time::Instant::now() }
    }
    fn is_fresh(&self, window: std::time::Duration) -> bool { self.received.elapsed() < window }
}
```

In `gpsd.rs`, update `parse_tpv` to keep the coordinates it already parses:

```rust
    let lat = v.get("lat")?.as_f64()?;
    let lon = v.get("lon")?.as_f64()?;
    Some(Fix { grid: lat_lon_to_grid(lat, lon), lat, lon, received: std::time::Instant::now() })
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p tuxlink parse_tpv && cargo test --manifest-path src-tauri/Cargo.toml position::`
Expected: PASS (existing `parses_a_3d_tpv_into_a_grid`, arbiter tests using `Fix::test` still compile + pass).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/position/arbiter.rs src-tauri/src/position/gpsd.rs
git commit -m "feat(position): retain raw lat/lon on GPS Fix (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task A2: `PositionArbiter::fresh_fix_latlon()`

**Files:**
- Modify: `src-tauri/src/position/arbiter.rs`
- Test: `src-tauri/src/position/arbiter.rs` (test module)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn fresh_fix_latlon_some_when_fresh_none_when_absent() {
    let a = PositionArbiter::new(PositionSource::Gps, None, PositionPrecision::FourCharGrid);
    assert_eq!(a.fresh_fix_latlon(), None, "no fix yet");
    let mut f = Fix::test("DM33ab");
    f.lat = 33.5; f.lon = -112.1;
    a.apply_gps_fix(f);
    assert_eq!(a.fresh_fix_latlon(), Some((33.5, -112.1)), "fresh fix returns coords");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_fix_latlon`
Expected: FAIL — `no method named 'fresh_fix_latlon'`.

- [ ] **Step 3: Implement** — add to `impl PositionArbiter`:

```rust
    /// The newest fix's raw lat/lon, but only while it is fresh. Local display
    /// only (the precise pin on the operator's own map during setup) — NEVER
    /// broadcast. Returns `None` when there is no fix or it has gone stale.
    pub fn fresh_fix_latlon(&self) -> Option<(f64, f64)> {
        let i = self.inner.lock().unwrap();
        i.last_fix.as_ref().filter(|f| f.is_fresh(FIX_STALENESS)).map(|f| (f.lat, f.lon))
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fresh_fix_latlon`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/position/arbiter.rs
git commit -m "feat(position): arbiter exposes fresh_fix_latlon for local map pin (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task A3: `PositionStatusDto` exposes `fix_lat`/`fix_lon`

**Files:**
- Modify: `src-tauri/src/ui_commands.rs` (`PositionStatusDto` + `position_status`)
- Modify: `src/shell/useStatus.ts` (TS `PositionStatusDto`)
- Test: `src-tauri/src/ui_commands.rs` (add a focused test) — see note

- [ ] **Step 1: Write the failing test** — add near the position command tests in `ui_commands.rs`:

```rust
#[tokio::test]
async fn position_status_omits_fix_coords_without_fresh_fix() {
    // A Manual arbiter with no fix → fix_lat/fix_lon are None.
    let arbiter = std::sync::Arc::new(crate::position::PositionArbiter::new(
        crate::config::PositionSource::Manual,
        Some("EM75".into()),
        crate::config::PositionPrecision::FourCharGrid,
    ));
    let (lat, lon) = (arbiter.fresh_fix_latlon().map(|c| c.0), arbiter.fresh_fix_latlon().map(|c| c.1));
    assert_eq!(lat, None);
    assert_eq!(lon, None);
}
```

> Note: `position_status` reads `config::read_config()` from disk, so a full command-level test needs the existing config-test harness. If that harness isn't readily available in this module, keep the unit assertion above (covers the `None` contract via the arbiter) and rely on the component test (Task C2) for the populated-DTO path. Do not invent a config fixture.

- [ ] **Step 2: Run test to verify it fails / compiles**

Run: `cargo test --manifest-path src-tauri/Cargo.toml position_status_omits_fix_coords`
Expected: PASS for the arbiter assertion (it documents the contract); proceed to wire the DTO.

- [ ] **Step 3: Implement** — extend the struct + command in `ui_commands.rs`:

```rust
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct PositionStatusDto {
    pub gps_ready: bool,
    pub broadcast_grid: String,
    pub ui_grid: String,
    /// Raw latitude of the live GPS fix, present only when `gps_ready` and
    /// `gps_state != Off`. LOCAL DISPLAY ONLY (the precise setup-map pin) —
    /// never broadcast. `None` for Manual source / no fix / GPS off.
    pub fix_lat: Option<f64>,
    /// Raw longitude of the live GPS fix; see `fix_lat`.
    pub fix_lon: Option<f64>,
}
```

```rust
#[tauri::command]
pub async fn position_status(
    arbiter: tauri::State<'_, std::sync::Arc<crate::position::PositionArbiter>>,
) -> Result<PositionStatusDto, UiError> {
    let cfg = config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let gps_on = cfg.privacy.gps_state != crate::config::GpsState::Off;
    let gps_ready = arbiter.has_fresh_fix() && gps_on;
    let (fix_lat, fix_lon) = if gps_ready {
        match arbiter.fresh_fix_latlon() { Some((la, lo)) => (Some(la), Some(lo)), None => (None, None) }
    } else { (None, None) };
    Ok(PositionStatusDto {
        gps_ready,
        broadcast_grid: crate::position::effective_broadcast_locator(&cfg, Some(&arbiter)),
        ui_grid: crate::position::effective_ui_locator(&cfg, Some(&arbiter)),
        fix_lat,
        fix_lon,
    })
}
```

Update TS shape in `src/shell/useStatus.ts` (`PositionStatusDto` interface):

```typescript
  ui_grid: string;
  /** Live GPS fix latitude — LOCAL DISPLAY ONLY (setup map pin), never broadcast.
   *  null for Manual / no fix / GPS off. */
  fix_lat: number | null;
  /** Live GPS fix longitude; see fix_lat. */
  fix_lon: number | null;
```

- [ ] **Step 4: Run tests + typecheck**

Run: `cargo test --manifest-path src-tauri/Cargo.toml position_status && pnpm -C . exec tsc --noEmit`
Expected: PASS; no TS errors. (Existing `useStatus`/`status.test.ts` consumers compile because the new fields are required on a DTO they build from `invoke`; if any synthetic test DTO omits them, add `fix_lat: null, fix_lon: null` there.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ui_commands.rs src/shell/useStatus.ts
git commit -m "feat(position): surface live fix lat/lon on PositionStatusDto (local-only) (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase B — Unconditional diagnostics

### Task B1: `classifyGpsSources` — device-independent triage + `noDevice`

**Files:**
- Modify: `src/location/gpsProbes.ts`
- Test: `src/location/gpsProbes.test.ts`

- [ ] **Step 1: Write the failing tests** (add to `gpsProbes.test.ts`)

```typescript
import { classifyGpsSources, type GpsDetection } from './gpsProbes';

const base: GpsDetection = {
  gpsd: { reachable: false },
  serial: { devices: [] },
  dialout: { member: true, groupExists: true },
  modemManager: { active: false },
};

it('emits dialout triage even with NO serial device present', () => {
  const c = classifyGpsSources({ ...base, dialout: { member: false, groupExists: true } });
  expect(c.triage.some((t) => t.kind === 'dialout')).toBe(true);
  expect(c.sources).toHaveLength(0);
});

it('emits modemmanager triage even with NO serial device present', () => {
  const c = classifyGpsSources({ ...base, modemManager: { active: true } });
  expect(c.triage.some((t) => t.kind === 'modemmanager')).toBe(true);
});

it('reports noDevice when no serial device and gpsd unreachable', () => {
  expect(classifyGpsSources(base).noDevice).toBe(true);
});

it('does not report noDevice when gpsd is reachable', () => {
  expect(classifyGpsSources({ ...base, gpsd: { reachable: true } }).noDevice).toBe(false);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/location/gpsProbes.test.ts`
Expected: FAIL — `noDevice` undefined; dialout/MM triage absent without serial.

- [ ] **Step 3: Implement** — replace the `classifyGpsSources` body in `gpsProbes.ts` and extend `GpsClassification`:

```typescript
export interface GpsClassification {
  sources: GpsSourceCard[];
  triage: GpsTriageCard[];
  /** True when no serial GPS device is present AND gpsd is unreachable —
   *  i.e. nothing to read a position from yet. Drives the "plug in + Rescan"
   *  card. Device-independent diagnostics (dialout/ModemManager) still apply. */
  noDevice: boolean;
}

export function classifyGpsSources(d: GpsDetection): GpsClassification {
  const sources: GpsSourceCard[] = [];
  const triage: GpsTriageCard[] = [];
  const hasSerial = d.serial.devices.length > 0;

  if (d.gpsd.reachable) {
    sources.push({ id: 'gpsd', kind: 'gpsd', label: 'gpsd daemon', detail: '127.0.0.1:2947' });
  }
  if (hasSerial && d.dialout.member) {
    for (const dev of d.serial.devices) {
      sources.push({ id: `serial:${dev.path}`, kind: 'serial', label: serialDeviceLabel(dev), detail: dev.path });
    }
  }

  // Device-INDEPENDENT diagnostics: these block GPS the moment a device appears,
  // so surface them up front (the device often won't enumerate until they're fixed).
  if (!d.dialout.member) {
    triage.push({
      kind: 'dialout',
      title: 'GPS access blocked: not in the "dialout" group',
      problem: "Even once a GPS is plugged in, your user can't open its serial port without this. It's the #1 Linux GPS wall.",
      command: 'sudo usermod -aG dialout "$USER"   # then log out and back in',
      fixable: d.dialout.groupExists,
    });
  }
  if (d.modemManager.active) {
    triage.push({
      kind: 'modemmanager',
      title: 'ModemManager is running',
      problem: 'ModemManager probes serial devices on connect and frequently grabs the GPS port the moment you plug it in — making the device "never appear".',
      command: 'sudo systemctl mask ModemManager   # reversible: systemctl unmask',
      fixable: true,
    });
  }

  return { sources, triage, noDevice: !hasSerial && !d.gpsd.reachable };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/location/gpsProbes.test.ts`
Expected: PASS (new + existing classification tests).

- [ ] **Step 5: Commit**

```bash
git add src/location/gpsProbes.ts src/location/gpsProbes.test.ts
git commit -m "feat(location): run GPS diagnostics unconditionally, add noDevice state (tuxlink-yy1m)

dialout/ModemManager triage no longer gated on a device already being
enumerated — that gate is why the diagnostics were invisible exactly
when Linux was broken and the device wouldn't appear.

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task B2: GpsSourcePicker renders the `noDevice` card + always shows diagnostics when no working source

**Files:**
- Modify: `src/location/GpsSourcePicker.tsx`
- Test: `src/location/GpsSourcePicker.test.tsx`

- [ ] **Step 1: Write the failing test** (add to `GpsSourcePicker.test.tsx`)

```typescript
it('shows the dialout triage and no-device card when no GPS and not in dialout', async () => {
  mockProbes({ gpsd: { reachable: false }, serial: { devices: [] }, dialout: { member: false, groupExists: true } });
  renderPicker();
  expect(await screen.findByTestId('gps-triage-dialout')).toBeInTheDocument();
  expect(screen.getByTestId('gps-no-device')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/location/GpsSourcePicker.test.tsx`
Expected: FAIL — `gps-no-device` not found.

- [ ] **Step 3: Implement** — in `GpsSourcePicker.tsx`, render a no-device card when `classification.noDevice` (place it after the triage `.map(...)`, before the manual card):

```tsx
      {classification.noDevice && (
        <div className="gps-card gps-card--nodevice" data-testid="gps-no-device">
          <div className="gps-card__body">
            <span className="gps-card__label">No GPS receiver detected yet</span>
            <span className="gps-card__detail">
              Plug in your USB or serial GPS, then press Rescan. A phone sharing
              location over gpsd works too.
            </span>
          </div>
        </div>
      )}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/location/GpsSourcePicker.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/location/GpsSourcePicker.tsx src/location/GpsSourcePicker.test.tsx src/location/GpsSourcePicker.css
git commit -m "feat(location): no-device diagnostic card in the GPS picker (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Phase C — Location map, live readout, manual pin, chromes

### Task C1: `LocationMap` component (draggable pin + GPS-fix marker + grid square)

**Files:**
- Create: `src/location/LocationMap.tsx`
- Test: `src/location/LocationMap.test.tsx`

`react-leaflet`'s `Marker` accepts `draggable` + an `eventHandlers.dragend` that reads the new latlng. The GPS-fix marker (precise) and the manual marker (grid-center, draggable) are distinct.

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';

// Mock the heavy leaflet substrate so this is a wiring/shape test (per the
// map subsystem's C1 convention — real drag/render is grim-verified).
vi.mock('../map/BaseMap', () => ({
  BaseMap: ({ children }: { children?: React.ReactNode }) => <div data-testid="basemap">{children}</div>,
}));
vi.mock('../map/useTileSource', () => ({ useTileSource: () => null }));
vi.mock('react-leaflet', () => ({
  Marker: (p: { draggable?: boolean }) => <div data-testid={p.draggable ? 'manual-marker' : 'gps-marker'} />,
  Rectangle: () => <div data-testid="grid-square" />,
}));

import { LocationMap } from './LocationMap';

describe('LocationMap', () => {
  it('renders a precise GPS marker when a fix is present', () => {
    render(<LocationMap grid="EM75km" fixLatLon={{ lat: 36.1, lon: -86.8 }} onGridChange={vi.fn()} />);
    expect(screen.getByTestId('gps-marker')).toBeInTheDocument();
  });
  it('renders a draggable manual marker when no fix', () => {
    render(<LocationMap grid="EM75km" fixLatLon={null} onGridChange={vi.fn()} />);
    expect(screen.getByTestId('manual-marker')).toBeInTheDocument();
    expect(screen.getByTestId('grid-square')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/location/LocationMap.test.tsx`
Expected: FAIL — cannot resolve `./LocationMap`.

- [ ] **Step 3: Implement** `src/location/LocationMap.tsx`

```tsx
/**
 * LocationMap — the offline location-setup map (tuxlink-yy1m). Composes BaseMap
 * (offline substrate) like PositionMapWidget. Two markers, never both meaningful:
 *   - GPS fix present  → a precise marker at the raw fix lat/lon (confirmation).
 *   - no fix           → a DRAGGABLE marker at the grid-square center; click the
 *                        map OR drag the marker to set the grid by hand.
 * The grid-square rectangle always frames the current grid. onGridChange fires
 * the Manual-pinning path in the parent (config_set_grid).
 */
import { Marker, Rectangle } from 'react-leaflet';
import type { LeafletEventHandlerFnMap, Marker as LMarker } from 'leaflet';
import { BaseMap } from '../map/BaseMap';
import { useTileSource } from '../map/useTileSource';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';

export interface LocationMapProps {
  /** Current grid (square highlight + manual marker center). */
  grid: string;
  /** Raw live GPS fix coords for the precise marker, or null when no fresh fix. */
  fixLatLon: { lat: number; lon: number } | null;
  /** Fired with the new grid when the operator clicks the map or drags the pin. */
  onGridChange: (grid: string) => void;
}

const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;
const HALF_LON_6 = 2.5 / 60;
const HALF_LAT_6 = 1.25 / 60;

export function LocationMap({ grid, fixLatLon, onGridChange }: LocationMapProps) {
  const tileSource = useTileSource();
  const ll = grid ? gridToLatLon(grid) : null;
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? HALF_LAT_6 : HALF_LAT_4;
  const halfLon = is6 ? HALF_LON_6 : HALF_LON_4;
  const bounds: [[number, number], [number, number]] | null = ll
    ? [[ll.lat - halfLat, ll.lon - halfLon], [ll.lat + halfLat, ll.lon + halfLon]]
    : null;

  const dragHandlers: LeafletEventHandlerFnMap = {
    dragend(e) {
      const m = e.target as LMarker;
      const { lat, lng } = m.getLatLng();
      onGridChange(latLonToGrid(lat, lng));
    },
  };

  const center = fixLatLon ?? ll ?? undefined;

  // The wrapper div (.location-map) is the CSS target both chromes size (large
  // left pane in the wizard; min-height block in Settings) — Task C4.
  return (
    <div className="location-map">
      <BaseMap
        onMapClick={({ lat, lon }) => onGridChange(latLonToGrid(lat, lon))}
        initialCenter={center}
        initialZoom={center ? 3 : 1}
        tileSource={tileSource ?? undefined}
      >
        {bounds && (
          <Rectangle bounds={bounds} pathOptions={{ color: '#5fd39a', weight: 2, fillOpacity: 0.1 }} />
        )}
        {fixLatLon ? (
          <Marker position={[fixLatLon.lat, fixLatLon.lon]} />
        ) : (
          ll && <Marker position={[ll.lat, ll.lon]} draggable eventHandlers={dragHandlers} />
        )}
      </BaseMap>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/location/LocationMap.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/location/LocationMap.tsx src/location/LocationMap.test.tsx
git commit -m "feat(location): LocationMap — offline confirm map w/ GPS pin + draggable manual pin (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task C2: `useLocationConfig` polls live position

**Files:**
- Modify: `src/location/useLocationConfig.ts`
- Test: `src/location/useLocationConfig.test.ts` (create if absent)

- [ ] **Step 1: Write the failing test** (`src/location/useLocationConfig.test.ts`)

```tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useLocationConfig } from './useLocationConfig';

beforeEach(() => vi.mocked(invoke).mockReset());

it('exposes live gps_ready + fix coords from position_status', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'EM75', position_source: 'Gps' } as never;
    if (cmd === 'position_status') return { gps_ready: true, broadcast_grid: 'EM75', ui_grid: 'EM75km', fix_lat: 36.1, fix_lon: -86.8 } as never;
    return undefined as never;
  });
  const { result } = renderHook(() => useLocationConfig());
  await waitFor(() => expect(result.current.gpsReady).toBe(true));
  expect(result.current.fixLat).toBeCloseTo(36.1);
  expect(result.current.fixLon).toBeCloseTo(-86.8);
  expect(result.current.uiGrid).toBe('EM75km');
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/location/useLocationConfig.test.ts`
Expected: FAIL — `gpsReady` undefined.

- [ ] **Step 3: Implement** — extend `useLocationConfig.ts`. Add a 2 s poll of `position_status` and new return fields:

```typescript
export interface UseLocationConfig {
  grid: string;
  selectedSource: string;
  error: string | null;
  onGridChange: (grid: string) => void;
  onSelectSource: (id: string) => void;
  // live arbiter status (tuxlink-yy1m)
  gpsReady: boolean;
  fixLat: number | null;
  fixLon: number | null;
  uiGrid: string;
}
```

Inside the hook, add state + a polling effect (mirrors the `useStatus` 2 s cadence):

```typescript
  const [gpsReady, setGpsReady] = useState(false);
  const [fixLat, setFixLat] = useState<number | null>(null);
  const [fixLon, setFixLon] = useState<number | null>(null);
  const [uiGrid, setUiGrid] = useState('');

  useEffect(() => {
    let mounted = true;
    const poll = () => {
      invoke<{ gps_ready: boolean; ui_grid: string; fix_lat: number | null; fix_lon: number | null }>('position_status')
        .then((s) => {
          if (!mounted) return;
          setGpsReady(s.gps_ready);
          setUiGrid(s.ui_grid);
          setFixLat(s.fix_lat);
          setFixLon(s.fix_lon);
        })
        .catch(() => { /* status unavailable — leave last known */ });
    };
    poll();
    const id = setInterval(poll, 2000);
    return () => { mounted = false; clearInterval(id); };
  }, []);
```

Add the four fields to the returned object.

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/location/useLocationConfig.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/location/useLocationConfig.ts src/location/useLocationConfig.test.ts
git commit -m "feat(location): useLocationConfig polls live position_status (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task C3: GpsSourcePicker embeds the map + live readout

**Files:**
- Modify: `src/location/GpsSourcePicker.tsx` (props + render)
- Modify: `src/location/GpsSourcePicker.css`
- Test: `src/location/GpsSourcePicker.test.tsx`

The picker gains `gpsReady`/`fixLatLon`/`uiGrid` props (passed by both chromes from the hook) and renders `LocationMap` + a readout. Keep it presentational.

- [ ] **Step 1: Write the failing test**

```tsx
it('shows "acquiring" before a fix and the grid readout after', async () => {
  mockProbes({ gpsd: { reachable: true } });
  const { rerender } = render(
    <GpsSourcePicker grid="EM75km" onGridChange={vi.fn()} selectedSource="gpsd"
      onSelectSource={vi.fn()} gpsReady={false} fixLatLon={null} uiGrid="" />,
  );
  expect(await screen.findByTestId('gps-readout-acquiring')).toBeInTheDocument();
  rerender(
    <GpsSourcePicker grid="EM75km" onGridChange={vi.fn()} selectedSource="gpsd"
      onSelectSource={vi.fn()} gpsReady={true} fixLatLon={{ lat: 36.1, lon: -86.8 }} uiGrid="EM75km" />,
  );
  expect(await screen.findByTestId('gps-readout-fixed')).toHaveTextContent('EM75km');
});
```

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/location/GpsSourcePicker.test.tsx`
Expected: FAIL — new props/readout testids absent (and TS error on missing props).

- [ ] **Step 3: Implement** — extend props + render in `GpsSourcePicker.tsx`:

```tsx
import { LocationMap } from './LocationMap';

export interface GpsSourcePickerProps {
  grid: string;
  onGridChange: (grid: string) => void;
  selectedSource: string;
  onSelectSource: (id: string) => void;
  // live status (tuxlink-yy1m) — supplied by each chrome from useLocationConfig
  gpsReady: boolean;
  fixLatLon: { lat: number; lon: number } | null;
  uiGrid: string;
}
```

At the top of the picker's returned JSX (above `gps-picker__head`), add the map + readout:

```tsx
      <LocationMap grid={uiGrid || grid} fixLatLon={fixLatLon} onGridChange={onGridChange} />
      {selectedSource !== 'manual' && (
        gpsReady ? (
          <div className="gps-readout gps-readout--ok" data-testid="gps-readout-fixed" role="status">
            <span className="gps-readout__grid">{uiGrid || grid || '—'}</span>
            <span className="gps-readout__sub">GPS fix acquired</span>
          </div>
        ) : (
          <div className="gps-readout gps-readout--acq" data-testid="gps-readout-acquiring" role="status">
            Acquiring GPS fix…
          </div>
        )
      )}
```

Add minimal styles to `GpsSourcePicker.css` for `.gps-readout`, `.gps-readout--ok`, `.gps-readout--acq`, `.gps-card--nodevice` (match the existing card palette).

- [ ] **Step 4: Run to verify pass + typecheck**

Run: `pnpm -C . vitest run src/location/GpsSourcePicker.test.tsx && pnpm -C . exec tsc --noEmit`
Expected: PASS; no TS errors. (Existing picker tests now pass the new props — update `renderPicker` defaults to include `gpsReady: false, fixLatLon: null, uiGrid: ''`.)

- [ ] **Step 5: Commit**

```bash
git add src/location/GpsSourcePicker.tsx src/location/GpsSourcePicker.css src/location/GpsSourcePicker.test.tsx
git commit -m "feat(location): embed confirm map + live GPS readout in the picker (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task C4: Wire the new props through both chromes + full-screen wizard layout

**Files:**
- Modify: `src/location/LocationSettings.tsx`
- Modify: `src/wizard/StepLocation.tsx`
- Modify: `src/wizard/wizard.css` (+ `src/location/GpsSourcePicker.css` for the rail layout)
- Test: existing `LocationSettings.test.tsx` / `StepLocation` coverage updated

- [ ] **Step 1: Write the failing test** — assert the wizard step renders the map (reachability regression for the "blank box" bug). Add to the StepLocation test (or `GpsSourcePicker.test.tsx` via the wizard render path):

```tsx
it('wizard Location step renders the map even with no GPS device', async () => {
  // No gpsd, no serial → the OLD behavior showed only a grid box. Assert the
  // map (and diagnostics) are present now.
  mockProbes({ gpsd: { reachable: false }, serial: { devices: [] }, dialout: { member: false, groupExists: true } });
  // render StepLocation within WizardProvider per existing wizard test harness
  // ...
  expect(await screen.findByTestId('basemap')).toBeInTheDocument();
});
```

> Use the existing wizard test harness (`WizardProvider` wrapper) already present in `src/wizard/*.test.tsx`; do not invent a new provider. Mock `@tauri-apps/api/core` `invoke` to also answer `config_read` + `position_status`.

- [ ] **Step 2: Run to verify failure**

Run: `pnpm -C . vitest run src/wizard`
Expected: FAIL — map not found / props missing.

- [ ] **Step 3: Implement**

In `LocationSettings.tsx`, pass the new live fields through:

```tsx
  const { grid, selectedSource, error, onGridChange, onSelectSource, gpsReady, fixLat, fixLon, uiGrid } = useLocationConfig();
  // ...
  <GpsSourcePicker
    grid={grid} onGridChange={onGridChange}
    selectedSource={selectedSource} onSelectSource={onSelectSource}
    gpsReady={gpsReady}
    fixLatLon={fixLat != null && fixLon != null ? { lat: fixLat, lon: fixLon } : null}
    uiGrid={uiGrid}
  />
```

In `StepLocation.tsx`, do the same and wrap in the full-screen two-pane layout:

```tsx
  const { grid, selectedSource, error, onGridChange, onSelectSource, gpsReady, fixLat, fixLon, uiGrid } = useLocationConfig();
  return (
    <div className="wizard-step wizard-step-location wizard-step-location--fullscreen" data-testid="wizard-step-location">
      <div className="wizard-location__rail">
        <h1>Set up your location</h1>
        <p>This is where Tuxlink thinks your station is. Confirm it on the map, or set it
           yourself. Optional — change it any time under <strong>Settings → Location</strong>.</p>
        {error && <div role="alert" className="wizard-error-banner" data-testid="wizard-location-error">{error}</div>}
        <GpsSourcePicker
          grid={grid} onGridChange={onGridChange}
          selectedSource={selectedSource} onSelectSource={onSelectSource}
          gpsReady={gpsReady}
          fixLatLon={fixLat != null && fixLon != null ? { lat: fixLat, lon: fixLon } : null}
          uiGrid={uiGrid}
        />
        <div className="wizard-submit-row">
          <button type="button" className="wizard-btn-secondary" data-testid="wizard-location-skip"
            onClick={() => dispatch({ type: 'ADVANCE_FROM_LOCATION' })}>Set later</button>
          <button type="button" data-testid="wizard-location-continue"
            onClick={() => dispatch({ type: 'ADVANCE_FROM_LOCATION' })}>Looks right — Continue</button>
        </div>
      </div>
    </div>
  );
```

> The picker already renders the map at the top of its output, so the "two-pane" split is achieved with CSS: in `wizard.css`, make `.wizard-step-location--fullscreen` fill the viewport and float the embedded `.gps-picker .location-map` (the `BaseMap` wrapper) to a large left pane with the cards/readout in a right rail. Concretely add a CSS rule that gives `.location-map` a `min-height: 60vh` in Settings (stacked) and a large left column in the wizard. Keep the component markup identical across chromes (D7); only CSS differs by the `--fullscreen` modifier on the wizard wrapper. Wrap the `BaseMap` output of `LocationMap` in a `<div className="location-map">` so CSS can target it.

(`LocationMap` already wraps its `BaseMap` in `<div className="location-map">` — added in Task C1 — so no markup change is needed here; only the CSS rules above.)

- [ ] **Step 4: Run to verify pass**

Run: `pnpm -C . vitest run src/wizard src/location && pnpm -C . exec tsc --noEmit`
Expected: PASS; no TS errors.

- [ ] **Step 5: Commit**

```bash
git add src/wizard/StepLocation.tsx src/wizard/wizard.css src/location/LocationSettings.tsx src/location/LocationMap.tsx src/location/GpsSourcePicker.css
git commit -m "feat(location): full-screen wizard Location layout + wire live position through both chromes (tuxlink-yy1m)

Agent: swallow-hemlock-fox
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task C5: Full verify gate + push + PR

- [ ] **Step 1: Run the CI-equivalent gates locally where cheap**

Run: `pnpm -C . vitest run src/location src/wizard src/shell && pnpm -C . exec tsc --noEmit`
Expected: PASS. (Per project memory, do NOT cold-run cargo locally; let Cloud CI compile the Rust. Clippy `--all-targets -D warnings` + full vitest are the CI gate.)

- [ ] **Step 2: Push**

```bash
git push
```

- [ ] **Step 3: Open the PR** (no-squash; never hand-merge the release PR)

```bash
gh pr create --base main --head bd-tuxlink-yy1m/gps-location-confirm \
  --title "[swallow-hemlock-fox] GPS/Location wizard: map confirmation + unconditional diagnostics (tuxlink-yy1m)" \
  --body "Implements docs/design/2026-06-13-gps-location-confirmation-design.md. Map-based position confirmation (precise GPS pin via local-only lat/lon), unconditional Linux GPS diagnostics, draggable manual pin, full-screen wizard layout. pkexec one-click (tuxlink-m9ej) lands as a stacked follow-up. CI is the gate (no local cargo)."
```

- [ ] **Step 4: Watch CI green; fix-forward on failures.**

---

## Wire-walk gate (before declaring done)

Per the project's `wire-walk` skill and the regression that motivated this feature: trace, from a **clean install with NO GPS device**, that the wizard Location step renders the map + the Linux diagnostics (dialout/ModemManager) — NOT a blank grid box. Confirmed in code by Task C4's reachability test; confirmed on a real host by the operator smoke (`tuxlink-6wz3`-style). CI-green ≠ works-on-clean-host.

## Out of scope (this plan)

pkexec one-click "Fix it for me" (`tuxlink-m9ej`, separate plan), native NMEA reader (`tuxlink-ley0`), live background monitoring (`tuxlink-gnws`), Bluetooth NMEA, satellite-count/fix-quality, sub-4-char broadcast precision.
