import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
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
  const props = {
    grid: '',
    onGridChange: vi.fn(),
    selectedSource: 'manual',
    onSelectSource: vi.fn(),
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

  it('ships the "Fix it for me" button disabled (pkexec helper is slice 2)', async () => {
    mockProbes({
      serial: { devices: [{ path: '/dev/ttyACM0', vendor: null, model: null, vendorId: null, productId: null }] },
      dialout: { member: false, groupExists: true },
    });
    renderPicker();
    const fix = await screen.findByTestId('gps-fix-dialout');
    expect(fix.hasAttribute('disabled')).toBe(true);
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
});
