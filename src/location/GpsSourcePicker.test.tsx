import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Stub the map so these tests don't pull leaflet into jsdom (LocationMap has its
// own shape test). Expose the grid it received for assertions.
vi.mock('./LocationMap', () => ({
  LocationMap: (p: { grid: string }) => <div data-testid="location-map-stub" data-grid={p.grid} />,
}));
import { invoke } from '@tauri-apps/api/core';
import { GpsSourcePicker, type GpsSourcePickerProps } from './GpsSourcePicker';

interface ProbeShape {
  gpsd?: { reachable: boolean };
  serial?: { devices: { path: string; vendor: string | null; model: string | null; vendorId: string | null; productId: string | null }[] };
  dialout?: { member: boolean; groupExists: boolean };
  modemManager?: { active: boolean };
}

function mockProbes(p: ProbeShape) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'gps_probe_gpsd': return (p.gpsd ?? { reachable: false }) as unknown as never;
      case 'gps_probe_serial_devices': return (p.serial ?? { devices: [] }) as unknown as never;
      case 'gps_probe_dialout': return (p.dialout ?? { member: false, groupExists: true }) as unknown as never;
      case 'gps_probe_modemmanager': return (p.modemManager ?? { active: false }) as unknown as never;
      default: return undefined as unknown as never;
    }
  });
}

function renderPicker(over: Partial<GpsSourcePickerProps> = {}) {
  const props: GpsSourcePickerProps = {
    grid: '',
    onGridChange: vi.fn(),
    selectedSource: 'manual',
    onSelectSource: vi.fn(),
    gpsReady: false,
    fixLatLon: null,
    uiGrid: '',
    ...over,
  };
  render(<GpsSourcePicker {...props} />);
  return props;
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('GpsSourcePicker', () => {
  it('renders gpsd as a usable source when reachable, and selecting it calls onSelectSource', async () => {
    mockProbes({ gpsd: { reachable: true } });
    const props = renderPicker();
    await screen.findByTestId('gps-source-gpsd');
    fireEvent.click(screen.getByTestId('gps-use-gpsd'));
    expect(props.onSelectSource).toHaveBeenCalledWith('gpsd');
  });

  it('renders a serial device source with its human label when the user is in dialout', async () => {
    mockProbes({
      serial: { devices: [{ path: '/dev/ttyACM0', vendor: 'u-blox AG', model: 'GNSS receiver', vendorId: '1546', productId: '01a8' }] },
      dialout: { member: true, groupExists: true },
    });
    renderPicker();
    const card = await screen.findByTestId('gps-source-serial:/dev/ttyACM0');
    expect(card.textContent).toMatch(/u-blox AG GNSS receiver/);
    expect(card.textContent).toMatch(/\/dev\/ttyACM0/);
  });

  it('shows a dialout triage card with a copy-pasteable fix command (the core Linux GPS wall)', async () => {
    mockProbes({
      serial: { devices: [{ path: '/dev/ttyACM0', vendor: null, model: null, vendorId: null, productId: null }] },
      dialout: { member: false, groupExists: true },
    });
    renderPicker();
    await screen.findByTestId('gps-triage-dialout');
    // Command hidden until "Show command".
    expect(screen.queryByTestId('gps-command-dialout')).toBeNull();
    fireEvent.click(screen.getByTestId('gps-show-command-dialout'));
    expect(screen.getByTestId('gps-command-dialout').textContent).toContain('usermod -aG dialout');
    expect(screen.getByTestId('gps-copy-dialout')).toBeTruthy();
  });

  it('disables "Fix it for me" when pkexec is unavailable (AppImage / minimal install)', async () => {
    // mockProbes does not answer gps_pkexec_available → pkexec stays false.
    mockProbes({
      serial: { devices: [{ path: '/dev/ttyACM0', vendor: null, model: null, vendorId: null, productId: null }] },
      dialout: { member: false, groupExists: true },
    });
    renderPicker();
    const fix = await screen.findByTestId('gps-fix-dialout');
    expect(fix.hasAttribute('disabled')).toBe(true);
  });

  it('runs the dialout fix via pkexec and shows the re-login notice (tuxlink-m9ej)', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'gps_pkexec_available': return true as unknown as never;
        case 'gps_probe_gpsd': return { reachable: false } as unknown as never;
        case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
        case 'gps_probe_dialout': return { member: false, groupExists: true } as unknown as never;
        case 'gps_probe_modemmanager': return { active: false } as unknown as never;
        case 'gps_run_fix': return 'ok' as unknown as never;
        default: return undefined as unknown as never;
      }
    });
    renderPicker();
    const fix = await screen.findByTestId('gps-fix-dialout');
    await waitFor(() => expect(fix).not.toBeDisabled());
    fireEvent.click(fix);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('gps_run_fix', { action: 'add-dialout' }));
    expect(await screen.findByTestId('gps-relogin-notice')).toBeInTheDocument();
  });

  it('always offers manual grid entry; typing calls onGridChange and an invalid grid shows an error', async () => {
    mockProbes({});
    const props = renderPicker({ grid: 'ZZ99zz' });
    const input = await screen.findByTestId('gps-manual-grid-input');
    fireEvent.change(input, { target: { value: 'EM75' } });
    expect(props.onGridChange).toHaveBeenCalledWith('EM75');
    // The provided invalid grid surfaces a validation error.
    expect(screen.getByTestId('gps-grid-error')).toBeTruthy();
  });

  it('rescans on demand', async () => {
    mockProbes({});
    renderPicker();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('gps_probe_gpsd'));
    const callsBefore = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'gps_probe_gpsd').length;
    fireEvent.click(screen.getByTestId('gps-picker-rescan'));
    await waitFor(() => {
      const after = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'gps_probe_gpsd').length;
      expect(after).toBeGreaterThan(callsBefore);
    });
  });

  // tuxlink-yy1m additions ----------------------------------------------------

  it('always renders the confirmation map', async () => {
    mockProbes({});
    renderPicker();
    expect(await screen.findByTestId('location-map-stub')).toBeInTheDocument();
  });

  it('shows the dialout triage AND a no-device card when no GPS and not in dialout', async () => {
    mockProbes({ gpsd: { reachable: false }, serial: { devices: [] }, dialout: { member: false, groupExists: true } });
    renderPicker();
    expect(await screen.findByTestId('gps-triage-dialout')).toBeInTheDocument();
    expect(screen.getByTestId('gps-no-device')).toBeInTheDocument();
  });

  it('shows "acquiring" when a GPS source is selected without a fix', async () => {
    mockProbes({ gpsd: { reachable: true } });
    renderPicker({ selectedSource: 'gpsd', gpsReady: false });
    expect(await screen.findByTestId('gps-readout-acquiring')).toBeInTheDocument();
  });

  it('shows the grid readout once a fix is acquired', async () => {
    mockProbes({ gpsd: { reachable: true } });
    renderPicker({ selectedSource: 'gpsd', gpsReady: true, uiGrid: 'EM75km', fixLatLon: { lat: 36.1, lon: -86.8 } });
    expect(await screen.findByTestId('gps-readout-fixed')).toHaveTextContent('EM75km');
  });
});
