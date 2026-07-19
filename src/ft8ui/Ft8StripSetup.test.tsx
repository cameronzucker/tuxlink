// src/ft8ui/Ft8StripSetup.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { Ft8StripSetup } from './Ft8StripSetup';
import type { Ft8Snapshot } from './ft8Types';

const DEVICES = [
  { humanName: 'Digirig Mobile', stableId: { kind: 'usbVidPidSerial', value: 'a' }, alsaHw: 'hw:1,0' },
  { humanName: 'Loopback: Analog', stableId: { kind: 'cardIdHash', value: 'b' }, alsaHw: 'hw:2,0' },
];

function snap(over: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'blocked', reason: 'needs-device-selection' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'waiting-first-slot', band: '20m', dialHz: 14_074_000,
    bandSource: 'default-unconfirmed', bandLabelConfirmedUtcMs: null,
    sweep: { mode: 'inactive', bandIdx: 0, dwellProgress: 0 },
    engineVersion: null, nConsecutive: 0, kConsecutive: 0,
    lastSlotUtcMs: null, lastFailure: null,
    availableDevices: DEVICES, ringTail: [],
    sweepConfig: { enabled: false, bands: [], dwellSlots: 0 },
    configuredDeviceName: null,
    ...over,
  } as Ft8Snapshot;
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd?: string) => {
    if (!cmd) return undefined;                       // teardown no-arg calls
    if (cmd === 'ft8_list_devices') return DEVICES;
    if (cmd === 'ft8_device_meter') return { rmsDbfs: -32, state: 'live' };
    return undefined;
  });
});

describe('Ft8StripSetup', () => {
  it('renders one <select> with an option per device, not row-per-device buttons', async () => {
    render(<Ft8StripSetup snapshot={snap()} />);
    const select = await screen.findByTestId('ft8-setup-device-select');
    expect(select.tagName).toBe('SELECT');
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual(
      expect.arrayContaining(['Digirig Mobile', 'Loopback: Analog']),
    );
    expect(screen.queryByText('Use this device')).toBeNull();
  });

  it('selecting a device persists it via ft8_set_device with the stableId', async () => {
    render(<Ft8StripSetup snapshot={snap()} />);
    const select = await screen.findByTestId('ft8-setup-device-select');
    fireEvent.change(select, { target: { value: 'Loopback: Analog' } });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('ft8_set_device', { stableId: DEVICES[1].stableId }),
    );
  });

  it('Start listening invokes ft8_listener_start and fires onStarted', async () => {
    const onStarted = vi.fn();
    render(
      <Ft8StripSetup snapshot={snap({ configuredDeviceName: 'Digirig Mobile' })} onStarted={onStarted} />,
    );
    fireEvent.click(await screen.findByTestId('ft8-setup-start'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('ft8_listener_start'));
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
  });

  it('zero devices renders the plug-in notice with a Refresh button', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd?: string) =>
      cmd === 'ft8_list_devices' ? [] : undefined,
    );
    render(<Ft8StripSetup snapshot={snap({ availableDevices: [] })} />);
    expect(await screen.findByTestId('ft8-setup-zero-devices')).toBeTruthy();
    expect(screen.getByTestId('ft8-setup-refresh')).toBeTruthy();
  });

  it('wsjtx-absent renders install copy with Retry wired to onRetry', async () => {
    const onRetry = vi.fn();
    render(
      <Ft8StripSetup
        snapshot={snap({ service: { axis: 'blocked', reason: 'wsjtx-absent' } })}
        onRetry={onRetry}
      />,
    );
    fireEvent.click(await screen.findByTestId('ft8-setup-retry'));
    expect(onRetry).toHaveBeenCalled();
  });
});
