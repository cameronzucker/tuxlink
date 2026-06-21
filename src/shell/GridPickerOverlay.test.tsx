/**
 * GridPickerOverlay + GridEdit "Pick on map" wiring (triage #18).
 *
 * SHAPE/WIRING ONLY: real Leaflet projection + pin render are grim-verified. The
 * map runs REAL in jsdom (GridPicker is now Leaflet); we capture the live L.Map
 * via vi.spyOn(L,'map') and fire a pin-mode click on it, proving it yields a
 * locator that confirm commits through the field's existing onCommit path.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, fireEvent, waitFor } from '@testing-library/react';
import L from 'leaflet';

import { GridPickerOverlay } from './GridPickerOverlay';
import { GridEdit } from './GridEdit';

const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));
vi.mock('../map/basemapLeaflet', () => ({
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

/** Flush LeafletMap's whenReady (sync) + async pack fetch, then resolve the map. */
async function flushMap() {
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
}

/** Fire a pin-mode click on the live Leaflet map at (lng, lat). */
function dropPin(lng: number, lat: number) {
  act(() => {
    captured!.fire('click', { latlng: L.latLng(lat, lng) } as L.LeafletMouseEvent);
  });
}

describe('GridPickerOverlay (triage #18)', () => {
  it('confirm is disabled until a pin is dropped', async () => {
    render(<GridPickerOverlay onConfirm={vi.fn()} onCancel={vi.fn()} />);
    await flushMap();
    expect(screen.getByTestId('grid-picker-confirm')).toBeDisabled();
    expect(screen.getByTestId('grid-picker-readout').textContent).toMatch(/click the map/i);
  });

  it('a map pin yields a locator; confirm commits the normalized grid', async () => {
    const onConfirm = vi.fn();
    render(<GridPickerOverlay onConfirm={onConfirm} onCancel={vi.fn()} />);
    await flushMap();
    dropPin(-118.2, 33.6);
    const readout = screen.getByTestId('grid-picker-readout').textContent ?? '';
    expect(readout).toMatch(/Locator: [A-Z]{2}\d{2}/);
    const confirm = screen.getByTestId('grid-picker-confirm');
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onConfirm.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}/);
  });

  it('backdrop click and Cancel both cancel', async () => {
    const onCancel = vi.fn();
    render(<GridPickerOverlay onConfirm={vi.fn()} onCancel={onCancel} />);
    await flushMap();
    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByTestId('grid-picker-overlay'));
    expect(onCancel).toHaveBeenCalledTimes(2);
  });
});

describe('GridEdit "Pick on map" wiring (triage #18)', () => {
  it('opens the picker from edit mode and commits the pinned grid via onCommit', async () => {
    const onCommit = vi.fn();
    render(
      <GridEdit
        grid="DM33"
        source="Manual"
        gpsReady={false}
        onCommit={onCommit}
        onUseGps={vi.fn()}
        onUseManual={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('grid-value-display'));
    const pick = screen.getByTestId('grid-pick-on-map');
    fireEvent.mouseDown(pick);
    expect(screen.getByTestId('grid-picker-overlay')).toBeInTheDocument();
    await flushMap();
    dropPin(-118.2, 33.6);
    fireEvent.click(screen.getByTestId('grid-picker-confirm'));
    expect(onCommit).toHaveBeenCalledOnce();
    expect(onCommit.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}/);
  });
});
