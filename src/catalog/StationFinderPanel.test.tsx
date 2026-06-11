import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactElement } from 'react';

vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
vi.mock('leaflet', async () => (await import('../map/testMapMock')).createLeafletMock());
vi.mock('../map/assets/world-equirect-2048.png', () => ({ default: '/world-equirect-2048.png' }));
vi.mock('leaflet/dist/leaflet.css', () => ({}));
vi.mock('leaflet/dist/images/marker-icon.png', () => ({ default: '/marker-icon.png' }));
vi.mock('leaflet/dist/images/marker-icon-2x.png', () => ({ default: '/marker-icon-2x.png' }));
vi.mock('leaflet/dist/images/marker-shadow.png', () => ({ default: '/marker-shadow.png' }));
import { resetMapMock } from '../map/testMapMock';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { StationFinderPanel } from './StationFinderPanel';

function renderPanel(ui: ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const N0DAJ = {
  channel: 'N0DAJ', callsign: 'N0DAJ', sysopName: 'Doug', grid: 'DM34oa', location: 'Wickenburg, AZ',
  frequenciesKhz: [3590, 7103], lastUpdate: null, email: null, homepage: null,
};

beforeEach(() => {
  resetMapMock();
  vi.mocked(invoke).mockReset();
  // cmd-gated so the runner's stray no-arg cleanup call stays inert.
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
    if (cmd === 'catalog_fetch_stations')
      return [{ mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1_700_000_000_000, gateways: [N0DAJ] }] as unknown as never;
    if (cmd === 'propagation_predict_path')
      return {
        bearingDeg: 318, distanceKm: 77, ssn: 118, year: 2026, month: 6,
        channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }],
      } as unknown as never;
    return undefined as unknown as never;
  });
});

describe('StationFinderPanel', () => {
  it('renders the Find a Station dialog with the controls bar', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    expect(await screen.findByRole('dialog', { name: /find a station/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /40 m/ })).toBeTruthy();
  });

  it('fetches + aggregates stations and shows a pin', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    await waitFor(() => expect(screen.getAllByTestId('station-pin').length).toBeGreaterThan(0));
  });

  it('selecting a pin populates the right rail', async () => {
    renderPanel(<StationFinderPanel onClose={() => {}} activePrefillMode="vara-hf" />);
    const pin = await screen.findByTestId('station-pin');
    fireEvent.click(pin);
    // Rail-only content (sysop + location) confirms the rail populated; the
    // callsign itself appears in both the pin tag and the rail header.
    expect(await screen.findByText(/Doug · Wickenburg, AZ/)).toBeTruthy();
  });

  it('closes on the × button', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    fireEvent.click(await screen.findByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalled();
  });

  it('does not crash when catalog_fetch_stations resolves undefined (degenerate backend)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { grid: 'DM43bp' } as unknown as never;
      if (cmd === 'catalog_fetch_stations') return undefined as unknown as never; // null/empty response
      return undefined as unknown as never;
    });
    renderPanel(<StationFinderPanel onClose={() => {}} />);
    // The dialog renders on first paint; the crash (if any) is on the post-fetch
    // re-render. Wait a tick so the fetch resolves, then assert still mounted.
    expect(await screen.findByRole('dialog', { name: /find a station/i })).toBeTruthy();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('catalog_fetch_stations', expect.anything()));
    expect(screen.getByRole('dialog', { name: /find a station/i })).toBeTruthy();
  });

  it('closes on Escape', async () => {
    const onClose = vi.fn();
    renderPanel(<StationFinderPanel onClose={onClose} />);
    await screen.findByRole('dialog');
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });
});
