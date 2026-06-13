import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Stub the offline map so the Settings test doesn't mount leaflet in jsdom.
vi.mock('./LocationMap', () => ({ LocationMap: () => <div data-testid="location-map-stub" /> }));
import { invoke } from '@tauri-apps/api/core';
import { LocationSettings } from './LocationSettings';

function mockBackend(over: { grid?: string | null; position_source?: string } = {}) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'config_read':
        return { grid: over.grid ?? null, position_source: over.position_source ?? 'Manual' } as unknown as never;
      case 'position_status':
        return { gps_ready: false, broadcast_grid: '', ui_grid: '', fix_lat: null, fix_lon: null } as unknown as never;
      case 'gps_probe_gpsd': return { reachable: true } as unknown as never;
      case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
      case 'gps_probe_dialout': return { member: false, groupExists: true } as unknown as never;
      case 'gps_probe_modemmanager': return { active: false } as unknown as never;
      default: return undefined as unknown as never;
    }
  });
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('LocationSettings', () => {
  it('loads the saved grid into the manual field', async () => {
    mockBackend({ grid: 'EM75xx', position_source: 'Manual' });
    render(<LocationSettings />);
    const input = (await screen.findByTestId('gps-manual-grid-input')) as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe('EM75xx'));
  });

  it('persists a GPS source selection via position_set_source (live switch)', async () => {
    mockBackend({ position_source: 'Manual' });
    render(<LocationSettings />);
    fireEvent.click(await screen.findByTestId('gps-use-gpsd'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('position_set_source', { source: 'Gps' }),
    );
  });

  it('persists a valid manual grid via config_set_grid, and skips invalid mid-typing', async () => {
    mockBackend({ grid: '', position_source: 'Manual' });
    render(<LocationSettings />);
    const input = await screen.findByTestId('gps-manual-grid-input');

    fireEvent.change(input, { target: { value: 'EM7' } }); // invalid — must NOT persist
    fireEvent.change(input, { target: { value: 'EM75' } }); // valid — persists
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('config_set_grid', { grid: 'EM75' }));
    const gridWrites = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'config_set_grid');
    expect(gridWrites).toHaveLength(1); // only the valid one
  });
});
