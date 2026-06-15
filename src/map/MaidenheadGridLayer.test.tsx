/**
 * Wiring tests for the MapLibre Maidenhead grid overlay (tuxlink-ndi4, phase 2).
 *
 * The MapLibre re-expression of MaidenheadOverlay: a GeoJSON source with a line
 * layer (the lattice) + a symbol layer (cell labels), driven by the pure
 * gridGeometry. Render correctness is grim-only; this asserts the source/layers
 * exist and the right features are pushed.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, act } from '@testing-library/react';
import type { Map as MaplibreMap } from 'maplibre-gl';
import { createMapLibreMock, type MapLibreMock } from './testMapLibreMock';
import { MapProvider } from './MapContext';
import { MaidenheadGridLayer, GRID_SOURCE_ID } from './MaidenheadGridLayer';
import { GridLevel, type GridBounds } from './gridGeometry';

function renderInMap(map: MapLibreMock, ui: React.ReactNode) {
  return render(<MapProvider value={map as unknown as MaplibreMap}>{ui}</MapProvider>);
}

const BOUNDS: GridBounds = { south: 40, west: -130, north: 50, east: -120 };

function sourceData(map: MapLibreMock): { type: string; features: Array<{ properties: { kind: string } }> } {
  return (map.getSource(GRID_SOURCE_ID) as { data: { type: string; features: Array<{ properties: { kind: string } }> } }).data;
}

describe('MaidenheadGridLayer', () => {
  it('adds the grid source + line and label layers when visible', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderInMap(map, <MaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />);
    expect(map.getSource(GRID_SOURCE_ID)).toBeTruthy();
    expect(map.getLayer('maidenhead-lines')).toBeTruthy();
    expect(map.getLayer('maidenhead-labels')).toBeTruthy();
  });

  it('pushes line and label features for the given bounds/level', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderInMap(map, <MaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />);
    const data = sourceData(map);
    expect(data.type).toBe('FeatureCollection');
    expect(data.features.some((f) => f.properties.kind === 'line')).toBe(true);
    expect(data.features.some((f) => f.properties.kind === 'label')).toBe(true);
  });

  it('pushes an empty collection when not visible', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderInMap(map, <MaidenheadGridLayer visible={false} bounds={BOUNDS} level={GridLevel.Square} />);
    expect(sourceData(map).features).toHaveLength(0);
  });

  it('re-pushes the lattice after a style swap (styledata)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderInMap(map, <MaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />);
    expect(sourceData(map).features.length).toBeGreaterThan(0);
    // Style swap drops the source; the overlay re-adds + re-pushes on styledata.
    act(() => map.setStyle({ version: 8, sources: {}, layers: [] }));
    act(() => map.__emit('styledata'));
    expect(map.getSource(GRID_SOURCE_ID)).toBeTruthy();
    expect(sourceData(map).features.length).toBeGreaterThan(0);
  });

  it('does NOT regenerate/setData on a moveend within the padded extent (B6)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    map.__setZoom(8);
    map.__setBounds({ west: -123, south: 47, east: -122, north: 48 });
    renderInMap(map, <MaidenheadGridLayer />); // derives bounds/level from the map
    const src = map.getSource(GRID_SOURCE_ID) as { setData: ReturnType<typeof vi.fn> };
    const before = src.setData.mock.calls.length;
    // A small pan that stays inside the already-generated padded extent.
    map.__setBounds({ west: -122.9, south: 47.1, east: -121.9, north: 48.1 });
    act(() => map.__emit('moveend'));
    expect(src.setData.mock.calls.length).toBe(before);
  });

  it('DOES regenerate when the view leaves the padded extent (B6)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    map.__setZoom(8);
    map.__setBounds({ west: -123, south: 47, east: -122, north: 48 });
    renderInMap(map, <MaidenheadGridLayer />);
    const src = map.getSource(GRID_SOURCE_ID) as { setData: ReturnType<typeof vi.fn> };
    const before = src.setData.mock.calls.length;
    map.__setBounds({ west: 10, south: 10, east: 11, north: 11 }); // far outside
    act(() => map.__emit('moveend'));
    expect(src.setData.mock.calls.length).toBeGreaterThan(before);
  });

  it('does not throw when the map is null (pre-load)', () => {
    expect(() =>
      render(
        <MapProvider value={null}>
          <MaidenheadGridLayer bounds={BOUNDS} level={GridLevel.Square} />
        </MapProvider>,
      ),
    ).not.toThrow();
  });
});
