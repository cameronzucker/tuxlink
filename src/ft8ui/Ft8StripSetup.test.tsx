// src/ft8ui/Ft8StripSetup.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Mock the meter-poll primitive so `stopAndAwait` (the device-handle release the
// Start flow must await before ft8_listener_start) is directly observable /
// controllable in tests. The real hook's live polling is exercised in its own
// unit test; here we only care about the start/stop handover contract.
vi.mock('./useDeviceMeterPoll', () => ({ useDeviceMeterPoll: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { useDeviceMeterPoll } from './useDeviceMeterPoll';
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
  // Default meter-poll stub: an immediately-resolving stopAndAwait so the
  // handover awaits complete synchronously in tests that don't care about it.
  vi.mocked(useDeviceMeterPoll).mockReset();
  vi.mocked(useDeviceMeterPoll).mockReturnValue({
    meter: null,
    error: null,
    stopAndAwait: vi.fn(async () => {}),
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

  // Codex P2: start() must release the meter's device handle (await
  // stopAndAwait) BEFORE invoking ft8_listener_start, or ALSA can reject the
  // start while the meter probe still holds the capture device.
  it('does not invoke ft8_listener_start until the meter stop promise resolves', async () => {
    let resolveStop!: () => void;
    const stopAndAwait = vi.fn(
      () =>
        new Promise<void>((r) => {
          resolveStop = () => r();
        }),
    );
    vi.mocked(useDeviceMeterPoll).mockReturnValue({ meter: null, error: null, stopAndAwait });

    render(<Ft8StripSetup snapshot={snap({ configuredDeviceName: 'Digirig Mobile' })} />);
    fireEvent.click(await screen.findByTestId('ft8-setup-start'));

    // The meter release is awaited first: the listener start must NOT fire yet.
    await waitFor(() => expect(stopAndAwait).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalledWith('ft8_listener_start');

    // Once the meter handle is released, the listener start proceeds.
    resolveStop();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('ft8_listener_start'));
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

  it('stale start-error clears when picking a different device', async () => {
    render(<Ft8StripSetup snapshot={snap({ configuredDeviceName: 'Digirig Mobile' })} />);

    // Cause Start to fail so the error banner renders
    vi.mocked(invoke).mockRejectedValueOnce(new Error('Device not ready'));
    fireEvent.click(await screen.findByTestId('ft8-setup-start'));

    // Wait for error banner to appear
    await waitFor(() => expect(screen.getByTestId('ft8-setup-start-error')).toBeTruthy());

    // Reset mock to succeed on ft8_set_device
    vi.mocked(invoke).mockImplementation(async (cmd?: string) => {
      if (!cmd) return undefined;
      if (cmd === 'ft8_set_device') return undefined;
      if (cmd === 'ft8_list_devices') return DEVICES;
      if (cmd === 'ft8_device_meter') return { rmsDbfs: -32, state: 'live' };
      return undefined;
    });

    // Pick a different device
    const select = await screen.findByTestId('ft8-setup-device-select');
    fireEvent.change(select, { target: { value: 'Loopback: Analog' } });

    // Error banner should be cleared
    await waitFor(() => expect(screen.queryByTestId('ft8-setup-start-error')).toBeNull());
  });
});
