import { describe, it, expect, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import L from 'leaflet';
import { useLeafletLayerGroup } from './leafletHooks';

// Leaflet reads clientWidth/clientHeight off the container to size the map; jsdom
// reports 0. Shim the prototype (R5 P2) so L.map() constructs with a real size.
const origDescW = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
const origDescH = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight');
Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: 800 });
Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: 600 });

afterEach(() => {
  if (origDescW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origDescW);
  if (origDescH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origDescH);
});

function makeMap(): L.Map {
  const div = document.createElement('div');
  document.body.appendChild(div);
  return L.map(div, { center: [0, 0], zoom: 2 });
}

describe('useLeafletLayerGroup', () => {
  it('adds a layer group to the map and removes it on unmount', () => {
    const map = makeMap();
    const { result, unmount } = renderHook(() => useLeafletLayerGroup(map));
    const lg = result.current!;
    expect(lg).not.toBeNull();
    expect(map.hasLayer(lg)).toBe(true);
    unmount();
    expect(map.hasLayer(lg)).toBe(false);
  });

  it('returns null for a null map', () => {
    const { result } = renderHook(() => useLeafletLayerGroup(null));
    expect(result.current).toBeNull();
  });
});
