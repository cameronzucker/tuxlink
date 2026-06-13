import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/react';

// tuxlink-ndi4 (plan A14): install the MapLibre test double GLOBALLY. The real
// `maplibregl.Map` constructor touches WebGL on instantiate, which jsdom lacks;
// once App-level / PositionFormV2 transitively mount a map, a per-file mock is a
// footgun. Mocking here makes every `import maplibregl from 'maplibre-gl'`
// resolve to the queryable fake. The factory dynamic-imports the double (the
// same pattern the legacy react-leaflet mock used) to stay hoist-safe.
vi.mock('maplibre-gl', async () => {
  const mod = await import('./map/testMapLibreMock');
  return mod.makeMapLibreModuleMock();
});

// globals:false means testing-library's automatic afterEach(cleanup) isn't
// registered; do it explicitly so the jsdom DOM is reset between tests.
afterEach(async () => {
  cleanup();
  const { resetMapLibreMock } = await import('./map/testMapLibreMock');
  resetMapLibreMock();
});
