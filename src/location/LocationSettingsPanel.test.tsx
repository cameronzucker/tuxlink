import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Stub the map so this test doesn't mount MapLibre/WebGL in jsdom (LocationMap
// has its own wiring test).
vi.mock('./LocationMap', () => ({ LocationMap: () => <div data-testid="location-map-stub" /> }));
import { invoke } from '@tauri-apps/api/core';
import { LocationSettingsPanel } from './LocationSettingsPanel';

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

describe('LocationSettingsPanel', () => {
  it('renders nothing when closed', () => {
    render(<LocationSettingsPanel open={false} onClose={vi.fn()} />);
    expect(screen.queryByTestId('location-modal')).toBeNull();
  });

  it('renders a dedicated modal with the GPS picker when open', async () => {
    render(<LocationSettingsPanel open onClose={vi.fn()} />);
    expect(screen.getByTestId('location-modal')).toBeInTheDocument();
    // The shared picker (map + source/diagnostics/manual) is mounted inside.
    expect(await screen.findByTestId('gps-picker')).toBeInTheDocument();
    expect(screen.getByTestId('location-map-stub')).toBeInTheDocument();
  });

  it('closes via the Done button and the backdrop', () => {
    const onClose = vi.fn();
    render(<LocationSettingsPanel open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('location-done'));
    fireEvent.click(screen.getByTestId('location-backdrop'));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does not close when the dialog body is clicked', () => {
    const onClose = vi.fn();
    render(<LocationSettingsPanel open onClose={onClose} />);
    fireEvent.click(screen.getByTestId('location-modal'));
    expect(onClose).not.toHaveBeenCalled();
  });
});
