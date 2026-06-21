/**
 * Drag-select hazard tests for the Leaflet GridPicker (tuxlink-rqvk, finding 8 —
 * the historically bug-prone interaction). The map runs REAL in jsdom; we capture
 * the live L.Map via vi.spyOn(L,'map') and drive raw mouse events on it. Wiring
 * only; the live rubber-band render is grim-verified.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, act, waitFor } from '@testing-library/react';
import L from 'leaflet';
import { GridPicker } from './GridPicker';

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

async function renderMap(ui: React.ReactElement) {
  const result = render(ui);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

const ll = (lat: number, lon: number) => ({ latlng: L.latLng(lat, lon) }) as L.LeafletMouseEvent;

describe('GridPicker — box mode drag-select', () => {
  it('disables dragging on mousedown and re-enables + fires onBoxChange on mouseup', async () => {
    const onBoxChange = vi.fn();
    await renderMap(<GridPicker mode="box" onBoxChange={onBoxChange} />);
    const disable = vi.spyOn(captured!.dragging, 'disable');
    const enable = vi.spyOn(captured!.dragging, 'enable');
    act(() => captured!.fire('mousedown', ll(40, -130)));
    expect(disable).toHaveBeenCalled();
    act(() => captured!.fire('mousemove', ll(50, -120)));
    act(() => captured!.fire('mouseup', ll(50, -120)));
    expect(onBoxChange).toHaveBeenCalledWith({ lat: 40, lon: -130 }, { lat: 50, lon: -120 });
    expect(enable).toHaveBeenCalled();
  });

  it('a window mouseup aborts the drag (re-enables dragging, no onBoxChange)', async () => {
    const onBoxChange = vi.fn();
    await renderMap(<GridPicker mode="box" onBoxChange={onBoxChange} />);
    const enable = vi.spyOn(captured!.dragging, 'enable');
    act(() => captured!.fire('mousedown', ll(40, -130)));
    act(() => {
      window.dispatchEvent(new MouseEvent('mouseup'));
    });
    expect(onBoxChange).not.toHaveBeenCalled();
    expect(enable).toHaveBeenCalled();
  });

  it('suppresses the click that fires right after a drag', async () => {
    const onGridChange = vi.fn();
    // a box-mode picker still wires click; the post-drag click must be eaten
    await renderMap(<GridPicker mode="box" onGridChange={onGridChange} onBoxChange={vi.fn()} />);
    act(() => captured!.fire('mousedown', ll(40, -130)));
    act(() => captured!.fire('mouseup', ll(50, -120)));
    act(() => captured!.fire('click', ll(50, -120)));
    expect(onGridChange).not.toHaveBeenCalled();
  });
});

describe('GridPicker — pin mode', () => {
  it('reports the 4-char grid for a clicked point', async () => {
    const onGridChange = vi.fn();
    await renderMap(<GridPicker mode="pin" onGridChange={onGridChange} />);
    act(() => captured!.fire('click', ll(0, 0)));
    expect(onGridChange).toHaveBeenCalledTimes(1);
    expect(onGridChange.mock.calls[0][0]).toHaveLength(4);
  });

  it('does not start a box drag in pin mode', async () => {
    const onBoxChange = vi.fn();
    await renderMap(<GridPicker mode="pin" onBoxChange={onBoxChange} />);
    const disable = vi.spyOn(captured!.dragging, 'disable');
    act(() => captured!.fire('mousedown', ll(40, -130)));
    expect(disable).not.toHaveBeenCalled();
    act(() => captured!.fire('mouseup', ll(50, -120)));
    expect(onBoxChange).not.toHaveBeenCalled();
  });

  it('renders the pin dot + grid-square highlight for the current grid', async () => {
    await renderMap(<GridPicker mode="pin" grid="CN87" onGridChange={vi.fn()} />);
    const dots: L.CircleMarker[] = [];
    const rects: L.Rectangle[] = [];
    captured!.eachLayer((l) => {
      if (l instanceof L.Rectangle) rects.push(l);
      else if (l instanceof L.CircleMarker) dots.push(l);
    });
    expect(dots.length).toBeGreaterThanOrEqual(1);
    expect(rects.length).toBeGreaterThanOrEqual(1);
  });
});

describe('GridPicker — composition', () => {
  it('mounts the Maidenhead lattice overlay by default', async () => {
    await renderMap(<GridPicker mode="box" />);
    let lines = 0;
    captured!.eachLayer((l) => {
      if (l instanceof L.Polyline && !(l instanceof L.Polygon)) lines += 1;
    });
    expect(lines).toBeGreaterThan(0);
  });

  it('omits the lattice when gridOverlay is false', async () => {
    await renderMap(<GridPicker mode="box" gridOverlay={false} />);
    let lines = 0;
    captured!.eachLayer((l) => {
      if (l instanceof L.Polyline && !(l instanceof L.Polygon)) lines += 1;
    });
    expect(lines).toBe(0);
  });
});
