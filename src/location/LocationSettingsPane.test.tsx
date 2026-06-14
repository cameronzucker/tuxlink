import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Stub the map so this test doesn't mount MapLibre/WebGL in jsdom.
vi.mock('./LocationMap', () => ({ LocationMap: () => <div data-testid="location-map-stub" /> }));
import { invoke } from '@tauri-apps/api/core';
import { LocationSettingsPane } from './LocationSettingsPane';

beforeEach(() => {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'config_read':
        return { grid: 'EM75', position_source: 'Gps' } as unknown as never;
      case 'position_status':
        return { gps_ready: false, broadcast_grid: '', ui_grid: '', fix_lat: null, fix_lon: null } as unknown as never;
      case 'gps_probe_gpsd':
        return { reachable: true } as unknown as never;
      case 'gps_probe_serial_devices':
        return { devices: [] } as unknown as never;
      case 'gps_probe_dialout':
        return { member: true, groupExists: true } as unknown as never;
      case 'gps_probe_modemmanager':
        return { active: false } as unknown as never;
      case 'gps_pkexec_available':
        return false as unknown as never;
      case 'gps_pkg_manager':
        return 'apt' as unknown as never;
      default:
        return undefined as unknown as never;
    }
  });
});

describe('LocationSettingsPane', () => {
  it('renders the shared GPS picker inline (no modal, no Open button)', async () => {
    render(<LocationSettingsPane />);
    expect(await screen.findByTestId('gps-picker')).toBeInTheDocument();
    expect(screen.getByTestId('location-map-stub')).toBeInTheDocument();
    expect(screen.queryByTestId('open-location-settings')).toBeNull();
    expect(screen.queryByTestId('location-modal')).toBeNull();
  });

  it('seeds the manual grid from config', async () => {
    render(<LocationSettingsPane />);
    const input = (await screen.findByTestId('gps-manual-grid-input')) as HTMLInputElement;
    await vi.waitFor(() => expect(input.value).toBe('EM75'));
  });
});
