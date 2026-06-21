/**
 * PositionPickerOverlay (tuxlink-sdbd / design §6) — the expand-to-overlay
 * Position location picker + precision selector.
 *
 * SHAPE/WIRING ONLY: real Leaflet projection + pin render are grim-verified. The
 * map runs REAL in jsdom (PositionMapWidget is now Leaflet); we capture the live
 * L.Map via vi.spyOn(L,'map') and drive clicks / zoom on it. The precision gate
 * (sixCharAllowed) is unit-tested in sixCharAllowed.test.ts — here we assert the
 * overlay HONORS it: zoomed out ⇒ 6-char disabled, 4-char the default, confirm
 * returns the precision-applied grid.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act, fireEvent, waitFor } from '@testing-library/react';
import L from 'leaflet';
import { PositionPickerOverlay } from './PositionPickerOverlay';

// LeafletMap fetches packs via invoke('basemap_list_packs') (wants {packs}).
const invokeMock = vi.hoisted(() =>
  vi.fn(async (cmd: string) => (cmd === 'basemap_list_packs' ? { packs: [] } : undefined)),
);
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

// Mock the base-layer builder → inert layer (PMTiles fetch/decode is grim-verified).
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
  window.localStorage.clear();
  invokeMock.mockClear();
});
afterEach(() => {
  vi.restoreAllMocks();
  if (origW) Object.defineProperty(HTMLElement.prototype, 'clientWidth', origW);
  if (origH) Object.defineProperty(HTMLElement.prototype, 'clientHeight', origH);
});

/** Render the overlay and flush LeafletMap's whenReady (sync) + async pack fetch. */
async function renderOverlay(ui: React.ReactElement) {
  const result = render(ui);
  await act(async () => {
    await Promise.resolve();
  });
  await waitFor(() => expect(captured).not.toBeNull());
  return result;
}

/** Click the live map at a lat/lon (the LeafletMap click seam → onGridChange). */
function clickMap(lat: number, lon: number) {
  act(() => {
    captured!.fire('click', { latlng: L.latLng(lat, lon) } as L.LeafletMouseEvent);
  });
}

/** Drive the live zoom; LeafletMap emits onZoomChange on moveend (deduped). */
function setZoom(zoom: number) {
  act(() => {
    captured!.setView(captured!.getCenter(), zoom, { animate: false });
  });
}

describe('PositionPickerOverlay (tuxlink-sdbd / §6)', () => {
  it('renders an in-app overlay (dimmed backdrop, not an OS window)', async () => {
    await renderOverlay(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-overlay')).toBeInTheDocument();
    expect(screen.getByRole('dialog', { name: /pick.*location/i })).toBeInTheDocument();
  });

  it('seeds the readout from initialGrid and updates it from a map pin', async () => {
    await renderOverlay(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/CN87/);
    clickMap(33.6, -118.2);
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/^[A-Z]{2}\d{2}/);
  });

  it('defaults to 4-char precision and confirms a 4-char locator', async () => {
    const onConfirm = vi.fn();
    await renderOverlay(<PositionPickerOverlay initialGrid="" onConfirm={onConfirm} onCancel={vi.fn()} />);
    clickMap(33.6, -118.2);
    fireEvent.click(screen.getByTestId('position-picker-confirm'));
    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onConfirm.mock.calls[0][0]).toMatch(/^[A-Z]{2}\d{2}$/);
  });

  it('disables 6-char until the view is zoomed past the subsquare threshold', async () => {
    await renderOverlay(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    // initialZoom 6 (placed grid) — below the threshold.
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    expect(screen.getByTestId('precision-hint').textContent).toMatch(/zoom in/i);
    setZoom(4); // still zoomed out
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
  });

  it('6-char gate tracks live map zoom via onZoomChange', async () => {
    await renderOverlay(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    setZoom(8); // below SIX_CHAR_MIN_ZOOM (9)
    expect(screen.getByTestId('precision-6char')).toBeDisabled();
    setZoom(12); // above the threshold → unlock
    expect(screen.getByTestId('precision-6char')).not.toBeDisabled();
    expect(screen.queryByTestId('precision-hint')).toBeNull();
  });

  it('Reset to GPS fix returns the pin to the arbiter grid', async () => {
    const onConfirm = vi.fn();
    await renderOverlay(
      <PositionPickerOverlay initialGrid="CN87us" gpsGrid="EM26" onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    clickMap(10, 10);
    fireEvent.click(screen.getByTestId('position-picker-reset-gps'));
    expect(screen.getByTestId('position-picker-readout').textContent).toMatch(/EM26/);
    fireEvent.click(screen.getByTestId('position-picker-confirm'));
    expect(onConfirm).toHaveBeenCalledWith('EM26');
  });

  it('hides Reset to GPS fix when no GPS grid is available', async () => {
    await renderOverlay(
      <PositionPickerOverlay initialGrid="CN87us" gpsGrid={null} onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(screen.queryByTestId('position-picker-reset-gps')).toBeNull();
  });

  it('cancels on the × button, the Cancel button, and a backdrop click', async () => {
    const onCancel = vi.fn();
    await renderOverlay(<PositionPickerOverlay initialGrid="CN87us" onConfirm={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    fireEvent.click(screen.getByLabelText('Close'));
    fireEvent.mouseDown(screen.getByTestId('position-picker-overlay'));
    expect(onCancel).toHaveBeenCalledTimes(3);
  });

  it('confirm is disabled until a locator is set (no initial grid, no pin)', async () => {
    await renderOverlay(<PositionPickerOverlay initialGrid="" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('position-picker-confirm')).toBeDisabled();
  });
});
