/**
 * Hazard tests for the owned MapLibre hook layer (tuxlink-ndi4, plan A15).
 *
 * "Thin" means a small API with LARGE lifecycle correctness. Each test pins one
 * hazard the react-leaflet→MapLibre swap introduces:
 *   - never add before the style is loaded;
 *   - re-add on `styledata` (it fires repeatedly AND after every `setStyle`);
 *   - idempotent (guard getLayer/getSource so repeated `styledata` never double-adds);
 *   - tolerate StrictMode double-invoke (production keeps <StrictMode>);
 *   - teardown removeLayer BEFORE removeSource (hook-call ordering contract).
 *
 * Wiring only — jsdom has no WebGL; render/projection correctness is grim-only.
 */
import * as React from 'react';
import { describe, it, expect, vi } from 'vitest';
import { render, renderHook } from '@testing-library/react';
import { createMapLibreMock } from './testMapLibreMock';
import { useMapLayer, useMapSource, useMapOverlay } from './mapHooks';

const LAYER = { id: 'water', type: 'fill', source: 'basemap' };
const SOURCE = { type: 'vector', url: 'tile://pmtiles/world' };

describe('useMapLayer', () => {
  it('adds the layer when the style is already loaded', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderHook(() => useMapLayer(map, LAYER));
    expect(map.getLayer('water')).toBeTruthy();
    expect(map.addLayer).toHaveBeenCalledTimes(1);
  });

  it('defers the add until the style loads, then adds on styledata', () => {
    const map = createMapLibreMock({ styleLoaded: false });
    renderHook(() => useMapLayer(map, LAYER));
    expect(map.getLayer('water')).toBeUndefined();
    // Style finishes loading; the map fires styledata.
    map.__setStyleLoaded(true);
    map.__emit('styledata');
    expect(map.getLayer('water')).toBeTruthy();
  });

  it('is idempotent: repeated styledata does not double-add', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderHook(() => useMapLayer(map, LAYER));
    map.__emit('styledata');
    map.__emit('styledata');
    expect(map.addLayer).toHaveBeenCalledTimes(1);
    expect(map.__state.layers.filter((l) => l.id === 'water')).toHaveLength(1);
  });

  it('re-adds the layer after setStyle drops it (the pane-occlusion regression)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderHook(() => useMapLayer(map, LAYER));
    expect(map.getLayer('water')).toBeTruthy();
    // Light↔dark swap: setStyle drops sources/layers; styledata then fires.
    map.setStyle({ version: 8, sources: {}, layers: [] });
    expect(map.getLayer('water')).toBeUndefined();
    map.__emit('styledata');
    expect(map.getLayer('water')).toBeTruthy();
  });

  it('teardown removes the layer and unsubscribes styledata', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    const { unmount } = renderHook(() => useMapLayer(map, LAYER));
    unmount();
    expect(map.getLayer('water')).toBeUndefined();
    expect(map.off).toHaveBeenCalledWith('styledata', expect.any(Function));
    // After teardown, a stray styledata must not re-add.
    map.__emit('styledata');
    expect(map.getLayer('water')).toBeUndefined();
  });

  it('tolerates StrictMode double-invoke (ends with exactly one layer)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderHook(() => useMapLayer(map, LAYER), {
      wrapper: ({ children }) => <React.StrictMode>{children}</React.StrictMode>,
    });
    expect(map.__state.layers.filter((l) => l.id === 'water')).toHaveLength(1);
  });

  it('does nothing when the map is null', () => {
    expect(() => renderHook(() => useMapLayer(null, LAYER))).not.toThrow();
  });
});

describe('useMapSource', () => {
  it('adds the source when the style is loaded; idempotent on repeated styledata', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    renderHook(() => useMapSource(map, 'basemap', SOURCE));
    map.__emit('styledata');
    expect(map.getSource('basemap')).toBeTruthy();
    expect(map.addSource).toHaveBeenCalledTimes(1);
  });

  it('teardown removes the source', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    const { unmount } = renderHook(() => useMapSource(map, 'basemap', SOURCE));
    unmount();
    expect(map.getSource('basemap')).toBeUndefined();
  });
});

describe('useMapOverlay (coupled source + layers)', () => {
  it('adds the source then its layers when the style is loaded', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    render(<Overlay map={map} />);
    expect(map.getSource('grid')).toBeTruthy();
    expect(map.getLayer('grid-line')).toBeTruthy();
    expect(map.getLayer('grid-fill')).toBeTruthy();
  });

  it('tears down removeLayer BEFORE removeSource (single-cleanup ordering)', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    const { unmount } = render(<Overlay map={map} />);
    unmount();
    const lastLayerRemoval = Math.max(...vi.mocked(map.removeLayer).mock.invocationCallOrder);
    const sourceRemoval = vi.mocked(map.removeSource).mock.invocationCallOrder[0];
    // Every layer removal precedes the source removal.
    expect(lastLayerRemoval).toBeLessThan(sourceRemoval);
  });

  it('re-adds the whole overlay after a setStyle swap', () => {
    const map = createMapLibreMock({ styleLoaded: true });
    render(<Overlay map={map} />);
    map.setStyle({ version: 8, sources: {}, layers: [] });
    expect(map.getSource('grid')).toBeUndefined();
    map.__emit('styledata');
    expect(map.getSource('grid')).toBeTruthy();
    expect(map.getLayer('grid-line')).toBeTruthy();
    expect(map.getLayer('grid-fill')).toBeTruthy();
  });
});

function Overlay({ map }: { map: ReturnType<typeof createMapLibreMock> }) {
  useMapOverlay(map, 'grid', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } }, [
    { id: 'grid-line', type: 'line', source: 'grid' },
    { id: 'grid-fill', type: 'fill', source: 'grid' },
  ]);
  return null;
}
