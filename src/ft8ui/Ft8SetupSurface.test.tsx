// src/ft8ui/Ft8SetupSurface.test.tsx
//
// Task C9a (plan tuxlink-b026z.4): device-picker half of the FT-8 setup
// surface. Covers the four blocked-reason arms + the meter/start handover
// race-safety ordering (§FirstRun + §States).

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { Ft8SetupSurface } from './Ft8SetupSurface';
import type { AudioDeviceChoice, Ft8Snapshot } from './ft8Types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

const DEV_A: AudioDeviceChoice = {
  humanName: 'USB Audio CODEC',
  stableId: { kind: 'byIdSymlink', value: 'usb-Burr-Brown_USB_Audio_CODEC-00' },
  alsaHw: 'hw:1,0',
};
const DEV_B: AudioDeviceChoice = {
  humanName: 'Digirig',
  stableId: { kind: 'usbVidPidSerial', value: '10c4:ea60:DR001' },
  alsaHw: 'hw:2,0',
};

/** Base snapshot builder — every field the component reads is filled with a
 *  neutral default; tests override only what they need. */
function makeSnapshot(overrides: Partial<Ft8Snapshot> = {}): Ft8Snapshot {
  return {
    service: { axis: 'blocked', reason: 'needs-device-selection' },
    flags: { clockUnsynced: false, catFixedBand: false, jt9Degraded: false },
    slotPhase: 'waiting-first-slot',
    band: '20m',
    dialHz: 14074000,
    bandSource: 'default-unconfirmed',
    bandLabelConfirmedUtcMs: null,
    sweep: { mode: 'inactive', bandIdx: null, dwellProgress: null },
    engineVersion: null,
    nConsecutive: 0,
    kConsecutive: 0,
    lastSlotUtcMs: null,
    lastFailure: null,
    availableDevices: [DEV_A, DEV_B],
    ringTail: [],
    sweepConfig: { enabled: false, bands: [], dwellSlots: 8 },
    configuredDeviceName: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'live' };
    if (cmd === 'ft8_list_devices') return [DEV_A];
    if (cmd === 'ft8_set_device') return undefined;
    if (cmd === 'ft8_listener_start') return undefined;
    return undefined;
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Ft8SetupSurface', () => {
  describe('wsjtx-absent arm', () => {
    it('shows package-install copy WITH a configured device present (never plug-in guidance)', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'wsjtx-absent' },
            configuredDeviceName: 'USB Audio CODEC',
          })}
        />,
      );
      const arm = screen.getByTestId('ft8-setup-arm-wsjtx-absent');
      expect(arm.textContent).toMatch(/wsjt-x/i);
      expect(arm.textContent).toMatch(/jt9/i);
      // Never plug-in/device guidance in this arm when a device is configured.
      expect(screen.queryByText(/plug in/i)).toBeNull();
      expect(screen.queryByTestId('ft8-setup-arm-zero-devices')).toBeNull();
      expect(screen.queryByTestId('ft8-setup-device-list')).toBeNull();
      // The persisted device name is acknowledged instead.
      expect(screen.getByTestId('ft8-setup-using-configured').textContent).toMatch(
        /USB Audio CODEC/,
      );
    });

    it('shows the device picker beneath the package copy when no device is configured', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'wsjtx-absent' },
            configuredDeviceName: null,
            availableDevices: [DEV_A],
          })}
        />,
      );
      expect(screen.getByTestId('ft8-setup-arm-wsjtx-absent').textContent).toMatch(/wsjt-x/i);
      expect(screen.getByTestId('ft8-setup-device-list')).toBeTruthy();
      expect(screen.queryByTestId('ft8-setup-using-configured')).toBeNull();
    });
  });

  describe('unsupported-sample-rate arm', () => {
    it('fetches the device list via ft8_list_devices (snapshot omits it in this state)', async () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'unsupported-sample-rate' },
            availableDevices: null, // L2 presence rule: omitted in this state
          })}
        />,
      );
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_list_devices');
      });
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-device-list').textContent).toMatch(/USB Audio CODEC/);
      });
      expect(screen.getByTestId('ft8-setup-arm-unsupported-sample-rate').textContent).toMatch(
        /48 kHz/i,
      );
    });

    it('never renders the zero-devices plug-in copy in this arm, even on an empty fetch', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_list_devices') return [];
        return undefined;
      });
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'unsupported-sample-rate' },
            availableDevices: null,
          })}
        />,
      );
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_list_devices');
      });
      expect(screen.queryByTestId('ft8-setup-arm-zero-devices')).toBeNull();
      expect(screen.queryByText(/plug in/i)).toBeNull();
    });
  });

  describe('zero-devices arm', () => {
    it('appears only when enumeration completed empty (needs-device-selection reason)', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            availableDevices: [],
          })}
        />,
      );
      expect(screen.getByTestId('ft8-setup-arm-zero-devices')).toBeTruthy();
      expect(screen.getByText(/plug in/i)).toBeTruthy();
    });

    it('does NOT appear while enumeration is still loading (availableDevices null)', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            availableDevices: null,
          })}
        />,
      );
      expect(screen.queryByTestId('ft8-setup-arm-zero-devices')).toBeNull();
      // Loading is not "empty" — the device-selection arm renders (with no rows yet).
      expect(screen.getByTestId('ft8-setup-arm-device-selection')).toBeTruthy();
    });

    it('does NOT appear when devices are present', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            availableDevices: [DEV_A],
          })}
        />,
      );
      expect(screen.queryByTestId('ft8-setup-arm-zero-devices')).toBeNull();
    });

    it('Refresh re-fetches via ft8_list_devices', async () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            availableDevices: [],
          })}
        />,
      );
      fireEvent.click(screen.getByTestId('ft8-setup-refresh'));
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_list_devices');
      });
      // Let the resulting device row (and its meter-poll mount effect) fully
      // settle before the test ends — otherwise the row's passive mount
      // effect can fire after this test's `afterEach` tears down the mock.
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-device-list').textContent).toMatch(/USB Audio CODEC/);
      });
    });
  });

  describe('needs-device-selection / device-absent arm', () => {
    it('renders one row per available device with name + alsaHw', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            availableDevices: [DEV_A, DEV_B],
          })}
        />,
      );
      const rowA = screen.getByTestId(
        `ft8-setup-device-row-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`,
      );
      expect(rowA.textContent).toMatch(/USB Audio CODEC/);
      expect(rowA.textContent).toMatch(/hw:1,0/);
      const rowB = screen.getByTestId(
        `ft8-setup-device-row-${DEV_B.stableId.kind}:${DEV_B.stableId.value}`,
      );
      expect(rowB.textContent).toMatch(/Digirig/);
    });

    it('device-absent reason (no device configured) renders the same picker', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'device-absent' },
            configuredDeviceName: null,
            availableDevices: [DEV_A],
          })}
        />,
      );
      expect(screen.getByTestId('ft8-setup-device-list')).toBeTruthy();
    });

    it('shows an in-use badge in place of the meter when state is in-use', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'in-use' };
        return undefined;
      });
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({ availableDevices: [DEV_A] })}
        />,
      );
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-meter-inuse')).toBeTruthy();
      });
      expect(screen.getByTestId('ft8-setup-meter-inuse').textContent).toMatch(/in use/i);
    });
  });

  describe('meter polling', () => {
    it('polls ft8_device_meter roughly every 500ms per visible device', async () => {
      vi.useFakeTimers();
      const meterCalls: unknown[] = [];
      vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
        if (cmd === 'ft8_device_meter') {
          meterCalls.push(args);
          return { rmsDbfs: -20, state: 'live' };
        }
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot({ availableDevices: [DEV_A] })} />);

      await act(async () => {
        await Promise.resolve();
      });
      expect(meterCalls.length).toBeGreaterThanOrEqual(1);

      await act(async () => {
        vi.advanceTimersByTime(1600); // ~3 more ticks at 500ms
        await Promise.resolve();
      });
      expect(meterCalls.length).toBeGreaterThanOrEqual(3);
      vi.useRealTimers();
    });
  });

  describe('meter/start handover race-safety', () => {
    it('stops metering and awaits the in-flight read BEFORE calling ft8_set_device / ft8_listener_start', async () => {
      // Manually control the meter promise so we can observe the ordering:
      // click "Use this device" WHILE a meter read is in flight, and assert
      // set_device/start are NOT called until that meter read resolves.
      let resolveMeter: ((v: { rmsDbfs: number; state: string }) => void) | null = null;
      const callOrder: string[] = [];

      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        callOrder.push(cmd);
        if (cmd === 'ft8_device_meter') {
          return new Promise((resolve) => {
            resolveMeter = resolve;
          });
        }
        if (cmd === 'ft8_set_device') return undefined;
        if (cmd === 'ft8_listener_start') return undefined;
        return undefined;
      });

      render(<Ft8SetupSurface snapshot={makeSnapshot({ availableDevices: [DEV_A] })} />);

      // Let the first meter poll fire and land in-flight (unresolved).
      await act(async () => {
        await Promise.resolve();
      });
      expect(callOrder).toEqual(['ft8_device_meter']);

      const useBtn = screen.getByTestId(
        `ft8-setup-device-use-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`,
      );
      fireEvent.click(useBtn);

      // Give any microtasks a chance to run — set_device must NOT have fired
      // yet, because the meter read it's awaiting hasn't resolved.
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(callOrder).toEqual(['ft8_device_meter']);
      expect(callOrder).not.toContain('ft8_set_device');
      expect(callOrder).not.toContain('ft8_listener_start');

      // Now let the in-flight meter read settle — only THEN should the
      // handover proceed to set_device, then start.
      await act(async () => {
        resolveMeter?.({ rmsDbfs: -20, state: 'live' });
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(callOrder).toEqual(['ft8_device_meter', 'ft8_set_device', 'ft8_listener_start']);
    });

    it('calls onStarted after a successful handover', async () => {
      const onStarted = vi.fn();
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({ availableDevices: [DEV_A] })}
          onStarted={onStarted}
        />,
      );
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-meter-live')).toBeTruthy();
      });
      fireEvent.click(
        screen.getByTestId(`ft8-setup-device-use-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`),
      );
      await waitFor(() => {
        expect(onStarted).toHaveBeenCalled();
      });
    });

    it('surfaces a select error without crashing when ft8_set_device rejects', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'live' };
        if (cmd === 'ft8_set_device') {
          return Promise.reject({ kind: 'device-not-found', detail: 'card unplugged' });
        }
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot({ availableDevices: [DEV_A] })} />);
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-meter-live')).toBeTruthy();
      });
      fireEvent.click(
        screen.getByTestId(`ft8-setup-device-use-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`),
      );
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-select-error').textContent).toMatch(/unplugged/i);
      });
    });
  });

  describe('defensive rendering', () => {
    it('renders nothing when the snapshot axis is not blocked (never mounted this way in prod, but must not crash)', () => {
      const { container } = render(
        <Ft8SetupSurface snapshot={makeSnapshot({ service: { axis: 'listening' } })} />,
      );
      expect(container.textContent).toBe('');
    });
  });
});
