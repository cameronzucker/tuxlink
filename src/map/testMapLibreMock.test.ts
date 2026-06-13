/**
 * Tests for the imperative MapLibre test double (tuxlink-ndi4, plan A14).
 *
 * Two concerns:
 *  1. `createMapLibreMock()` is a queryable fake `maplibregl.Map` — add/get/remove
 *     source+layer spies backed by a registry, an event registry, isStyleLoaded.
 *  2. The double is installed GLOBALLY (test-setup.ts `vi.mock('maplibre-gl')`),
 *     so `import maplibregl from 'maplibre-gl'` resolves to the mock and never
 *     touches WebGL (the real constructor does, which is why per-file mocks were
 *     a footgun — A14).
 */
import { describe, it, expect, beforeEach } from 'vitest';
import maplibregl from 'maplibre-gl';
import {
  createMapLibreMock,
  constructedMaps,
  resetMapLibreMock,
  getLastMap,
} from './testMapLibreMock';

beforeEach(() => resetMapLibreMock());

describe('createMapLibreMock registry', () => {
  it('records added sources and layers and exposes them', () => {
    const map = createMapLibreMock();
    map.addSource('basemap', { type: 'vector', url: 'tile://pmtiles/world' });
    map.addLayer({ id: 'water', type: 'fill', source: 'basemap' });

    expect(map.getSource('basemap')).toBeTruthy();
    expect(map.getLayer('water')).toBeTruthy();
    expect(map.__state.sources.has('basemap')).toBe(true);
    expect(map.__state.layers.map((l) => l.id)).toEqual(['water']);
  });

  it('removeLayer / removeSource clear the registry', () => {
    const map = createMapLibreMock();
    map.addSource('s', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } });
    map.addLayer({ id: 'l', type: 'line', source: 's' });
    map.removeLayer('l');
    map.removeSource('s');
    expect(map.getLayer('l')).toBeUndefined();
    expect(map.getSource('s')).toBeUndefined();
  });

  it('isStyleLoaded reflects the controllable flag', () => {
    const map = createMapLibreMock({ styleLoaded: false });
    expect(map.isStyleLoaded()).toBe(false);
    map.__setStyleLoaded(true);
    expect(map.isStyleLoaded()).toBe(true);
  });

  it('on/off/__emit drive the event registry', () => {
    const map = createMapLibreMock();
    let fired = 0;
    const handler = () => {
      fired += 1;
    };
    map.on('styledata', handler);
    map.__emit('styledata');
    map.__emit('styledata');
    expect(fired).toBe(2);
    map.off('styledata', handler);
    map.__emit('styledata');
    expect(fired).toBe(2);
  });

  it('setStyle drops sources+layers (real MapLibre behavior) without auto-emitting', () => {
    const map = createMapLibreMock();
    map.addSource('s', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } });
    map.addLayer({ id: 'l', type: 'line', source: 's' });
    map.setStyle({ version: 8, sources: {}, layers: [] });
    expect(map.getLayer('l')).toBeUndefined();
    expect(map.getSource('s')).toBeUndefined();
  });

  it('getZoom is controllable and click/zoom helpers are spies', () => {
    const map = createMapLibreMock({ zoom: 4.5 });
    expect(map.getZoom()).toBe(4.5);
    map.flyTo({ center: [0, 0], zoom: 3 });
    expect(map.flyTo).toHaveBeenCalled();
    map.setMaxZoom(14);
    expect(map.setMaxZoom).toHaveBeenCalledWith(14);
  });
});

describe('global maplibre-gl module mock', () => {
  it('new maplibregl.Map(...) yields a tracked mock, not a real WebGL map', () => {
    const m = new maplibregl.Map({ container: document.createElement('div') });
    // The instance is a registry-backed mock.
    expect(typeof (m as unknown as { __emit: unknown }).__emit).toBe('function');
    expect(constructedMaps).toHaveLength(1);
    expect(getLastMap()).toBe(m as unknown as ReturnType<typeof createMapLibreMock>);
  });

  it('exposes addProtocol + Marker + control constructors used by tuxlink', () => {
    expect(typeof maplibregl.addProtocol).toBe('function');
    const marker = new maplibregl.Marker();
    expect(typeof marker.setLngLat).toBe('function');
    expect(typeof marker.addTo).toBe('function');
    expect(marker.getElement()).toBeInstanceOf(HTMLElement);
    expect(maplibregl.NavigationControl).toBeTruthy();
    expect(maplibregl.AttributionControl).toBeTruthy();
  });
});
