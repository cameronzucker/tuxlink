/**
 * Wiring tests for the Leaflet Maidenhead grid overlay (tuxlink-4hol).
 *
 * The Leaflet re-expression of MaidenheadGridLayer: lattice lines as L.polyline +
 * cell labels as divIcon markers on the owned LayerGroup, driven by the pure
 * gridGeometry. The map runs REAL in jsdom; render correctness is grim-only. This
 * asserts the lines/labels exist, the not-visible empty case, and the B6 recompute
 * gating (no regeneration on a pan that stays inside the padded extent).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import { LeafletMap } from './LeafletMap';
import { LeafletMaidenheadGridLayer } from './LeafletMaidenheadGridLayer';
import { GridLevel, type GridBounds } from './gridGeometry';

const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('./basemapLeaflet', () => ({
  buildBaseLayers: vi.fn(() => [L.layerGroup()]),
  OSM_ATTRIBUTION: '© OpenStreetMap contributors',
  flavorBackground: () => '#34373d',
}));

const origW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
const realLMap = L.map.bind(L);
let captured: L.Map | null = null;

beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });
  captured = null;
  vi.spyOn(L, 'map').mockImplementation(((el: HTMLElement | string, opts?: L.MapOptions) => {
    const m = realLMap(el as HTMLElement, opts);
    captured = m;
    return m;
  }) as typeof L.map);
  invokeMock.mockClear();
});
afterEach(() => {
  vi.restoreAllMocks();
  if (origW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origW);
  if (origH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origH);
});

const BOUNDS: GridBounds = { south: 40, west: -130, north: 50, east: -120 };

async function renderLattice(node: React.ReactNode) {
  const result = render(<LeafletMap initialCenter={{ lat: 45, lon: -125 }} initialZoom={5}>{node}</LeafletMap>);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

function polylines(): L.Polyline[] {
  const out: L.Polyline[] = [];
  captured!.eachLayer((l) => {
    // L.Rectangle/Polygon extend Polyline — exclude them (none here, but be exact).
    if (l instanceof L.Polyline && !(l instanceof L.Polygon)) out.push(l);
  });
  return out;
}

describe('LeafletMaidenheadGridLayer', () => {
  it('draws lattice lines + cell labels when visible', async () => {
    const { container } = await renderLattice(
      <LeafletMaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />,
    );
    expect(polylines().length).toBeGreaterThan(0);
    expect(container.querySelectorAll('.maidenhead-grid-label').length).toBeGreaterThan(0);
  });

  it('draws nothing when not visible', async () => {
    const { container } = await renderLattice(
      <LeafletMaidenheadGridLayer visible={false} bounds={BOUNDS} level={GridLevel.Square} />,
    );
    expect(polylines()).toHaveLength(0);
    expect(container.querySelectorAll('.maidenhead-grid-label')).toHaveLength(0);
  });

  it('does NOT regenerate on a moveend within the padded extent (B6)', async () => {
    await renderLattice(<LeafletMaidenheadGridLayer level={GridLevel.Square} />);
    const before = polylines().length;
    expect(before).toBeGreaterThan(0);
    // A small pan staying inside the already-generated padded extent → no regen.
    // (Lines have stable identity: count unchanged AND same instances.)
    const firstLine = polylines()[0];
    act(() => {
      captured!.panBy([2, 2], { animate: false });
      captured!.fire('moveend');
    });
    expect(polylines().length).toBe(before);
    expect(polylines()[0]).toBe(firstLine); // not re-tessellated
  });

  it('regenerates when the grid level changes (controlled prop)', async () => {
    const { rerender } = await renderLattice(<LeafletMaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Field} />);
    const fieldLines = polylines().length;
    await act(async () => {
      rerender(
        <LeafletMap initialCenter={{ lat: 45, lon: -125 }} initialZoom={5}>
          <LeafletMaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />
        </LeafletMap>,
      );
      await Promise.resolve();
    });
    // Square cells (2°×1°) are far denser than Field cells (20°×10°) over the same bounds.
    expect(polylines().length).toBeGreaterThan(fieldLines);
  });
});
