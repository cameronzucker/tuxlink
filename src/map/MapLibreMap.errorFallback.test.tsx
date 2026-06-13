// MapLibreMap construction-failure fallback (tuxlink-52h6). The 0.60.0 blank
// screen was `new maplibregl.Map()` throwing (WebGL unavailable in WebKitGTK,
// or a bad style) and, with no boundary, taking the whole app down. MapLibreMap
// must catch its OWN construction throw and degrade to a "map unavailable"
// panel so every consumer (location, compose, station finder, grid picker)
// keeps its surrounding UI instead of vanishing.
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';

// Override the global maplibre mock for THIS file: keep the protocol/control
// surface (module load + addControl), but make the Map constructor throw the
// way a WebGL-less WebKitGTK does.
vi.mock('maplibre-gl', async () => {
  const mod = await import('./testMapLibreMock');
  const base = mod.makeMapLibreModuleMock();
  const ThrowingMap = function ThrowingMap(): never {
    throw new Error('Failed to initialize WebGL');
  };
  return {
    ...base,
    Map: ThrowingMap,
    default: { ...(base.default as Record<string, unknown>), Map: ThrowingMap },
  };
});

import { MapLibreMap } from './MapLibreMap';

afterEach(cleanup);

describe('MapLibreMap construction failure', () => {
  it('renders a map-unavailable fallback instead of letting the throw escape', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    // The render must NOT throw — if the construction throw escaped, this call
    // would reject and fail the test (that is the 0.60.0 blank-window bug).
    render(
      <MapLibreMap>
        <div data-testid="overlay-child">overlay</div>
      </MapLibreMap>,
    );
    expect(screen.getByTestId('map-unavailable')).toBeInTheDocument();
    spy.mockRestore();
  });
});
