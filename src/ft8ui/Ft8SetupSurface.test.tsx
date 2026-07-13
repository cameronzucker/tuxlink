// src/ft8ui/Ft8SetupSurface.test.tsx
//
// Task C9a (plan tuxlink-b026z.4): device-picker half of the FT-8 setup
// surface. Covers the four blocked-reason arms + the meter/start handover
// race-safety ordering (§FirstRun + §States).
//
// Task C9b: Step 2 (rig control / Test CAT) + the `Start listening on
// <band> →` CTA. Covers commitNow-before-probe ordering, Test-CAT success/
// error copy, and the CTA disable-reason matrix.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { Ft8SetupSurface } from './Ft8SetupSurface';
import type { AudioDeviceChoice, Ft8Snapshot } from './ft8Types';
import type { RigConfig } from '../radio/modes/RigControlSection';

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

/** Neutral RigConfig fixture — Step 2's RigControlSection loads this on
 *  mount via config_get_rig. */
const KNOWN_RIG_CONFIG: RigConfig = {
  rig_hamlib_model: null,
  rigctld_host: '127.0.0.1',
  rigctld_port: 4534,
  rigctld_binary: 'rigctld',
  close_serial_sequencing: false,
  live_vfo_poll: false,
  qsy_on_fail: false,
  cat_serial_path: '/dev/ttyUSB0',
  cat_baud: 38400,
  data_mode: 'PKTUSB',
  rig_field_overrides: [],
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
    // Step 2's RigControlSection (third render site) mounts alongside Step 1
    // and issues its own mount-time reads — a neutral, resolvable default so
    // Step-1-focused tests above don't need to know about Step 2 at all.
    if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
    if (cmd === 'rig_list_models') return [];
    if (cmd === 'packet_list_serial_devices') return [];
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

  // Task D2 (operator decision, 2026-07-12): "Use this device" selects,
  // only the Start CTA starts. A device row's click is now SELECT-ONLY —
  // it persists via ft8_set_device and stops there; ft8_listener_start and
  // onStarted are reserved for the CTA (covered separately below).
  describe('device-row select-only handover (Task D2)', () => {
    it('stops metering and awaits the in-flight read BEFORE calling ft8_set_device — and never calls ft8_listener_start', async () => {
      // Manually control the meter promise so we can observe the ordering:
      // click "Use this device" WHILE a meter read is in flight, and assert
      // set_device is NOT called until that meter read resolves.
      let resolveMeter: ((v: { rmsDbfs: number; state: string }) => void) | null = null;
      const callOrder: string[] = [];
      // Task C9b: Step 2's RigControlSection mounts alongside Step 1 now and
      // fires its own mount-time commands (config_get_rig / rig_list_models /
      // packet_list_serial_devices) independently of the device-meter
      // handover this test is about — filter the shared callOrder log down
      // to the device/listener commands under test rather than asserting on
      // the raw log, which would otherwise couple this C9a assertion to
      // Step 2's unrelated mount ordering.
      const deviceCallOrder = () => callOrder.filter((c) => c.startsWith('ft8_device') || c === 'ft8_set_device' || c === 'ft8_listener_start');

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
      expect(deviceCallOrder()).toEqual(['ft8_device_meter']);

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
      expect(deviceCallOrder()).toEqual(['ft8_device_meter']);
      expect(callOrder).not.toContain('ft8_set_device');
      expect(callOrder).not.toContain('ft8_listener_start');

      // Now let the in-flight meter read settle — only THEN should the
      // handover proceed to set_device. It stops there — select-only. The
      // trailing `ft8_device_meter` is the Task D2 "meter resumes" contract:
      // `selecting` drops back to false right after `ft8_set_device`
      // resolves, which flips the row's poll hook `enabled` back to true and
      // fires an immediate resumed read — never a `ft8_listener_start`.
      await act(async () => {
        resolveMeter?.({ rmsDbfs: -20, state: 'live' });
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(deviceCallOrder()).toEqual(['ft8_device_meter', 'ft8_set_device', 'ft8_device_meter']);
      expect(callOrder).not.toContain('ft8_listener_start');
    });

    it('never calls ft8_listener_start or onStarted, even after a successful persist', async () => {
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
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_set_device', { stableId: DEV_A.stableId });
      });
      expect(vi.mocked(invoke)).not.toHaveBeenCalledWith('ft8_listener_start');
      expect(onStarted).not.toHaveBeenCalled();
    });

    it('resumes the meter after the persist settles (no permanently-frozen row)', async () => {
      let meterCalls = 0;
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_device_meter') {
          meterCalls += 1;
          return { rmsDbfs: -20, state: 'live' };
        }
        if (cmd === 'ft8_set_device') return undefined;
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot({ availableDevices: [DEV_A] })} />);
      await waitFor(() => expect(meterCalls).toBeGreaterThanOrEqual(1));
      const callsBeforeSelect = meterCalls;

      fireEvent.click(
        screen.getByTestId(`ft8-setup-device-use-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`),
      );
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_set_device', { stableId: DEV_A.stableId });
      });
      // Once `selecting` drops back to false, the row's meter poll re-enables
      // and fires again — it does not stay frozen forever.
      await waitFor(() => expect(meterCalls).toBeGreaterThan(callsBeforeSelect));
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

  describe('device-row selected state', () => {
    it('shows a "selected" badge when configuredDeviceName matches the row', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({ availableDevices: [DEV_A, DEV_B], configuredDeviceName: DEV_A.humanName })}
        />,
      );
      expect(
        screen.getByTestId(`ft8-setup-device-selected-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`),
      ).toBeInTheDocument();
      expect(
        screen.queryByTestId(`ft8-setup-device-selected-${DEV_B.stableId.kind}:${DEV_B.stableId.value}`),
      ).toBeNull();
    });

    it('shows no selected badge on any row when no device is configured', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({ availableDevices: [DEV_A, DEV_B], configuredDeviceName: null })}
        />,
      );
      expect(
        screen.queryByTestId(`ft8-setup-device-selected-${DEV_A.stableId.kind}:${DEV_A.stableId.value}`),
      ).toBeNull();
      expect(
        screen.queryByTestId(`ft8-setup-device-selected-${DEV_B.stableId.kind}:${DEV_B.stableId.value}`),
      ).toBeNull();
    });
  });

  // Task D2 "no way back to setup once capture runs": the surface can now be
  // force-mounted over a LIVE session (LiveBandStrip's header "setup"
  // button, Finding 4b) to fix a wrong device/rig pick without restarting.
  describe('D2 active-axis mode (revisit setup during a live session)', () => {
    it('renders (not null) while listening, with device rows and a Stop CTA instead of Start', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_list_devices') return [DEV_A];
        if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'live' };
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        return undefined;
      });
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'listening' },
            configuredDeviceName: DEV_A.humanName,
            availableDevices: null, // L2 presence rule: the backend omits it while listening
          })}
        />,
      );
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_list_devices');
      });
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-device-list').textContent).toMatch(/USB Audio CODEC/);
      });
      expect(screen.queryByTestId('ft8-setup-start-cta')).toBeNull();
      const stopBtn = screen.getByTestId('ft8-setup-stop-cta');
      expect(stopBtn.textContent).toMatch(/stop listening/i);
      expect(screen.getByTestId('ft8-setup-cta-caption').textContent).toMatch(/stop to change devices/i);
    });

    it('Stop CTA invokes ft8_listener_stop', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_list_devices') return [DEV_A];
        if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'live' };
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_listener_stop') return undefined;
        return undefined;
      });
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'starting' },
            configuredDeviceName: DEV_A.humanName,
            availableDevices: null,
          })}
        />,
      );
      const stopBtn = await screen.findByTestId('ft8-setup-stop-cta');
      fireEvent.click(stopBtn);
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_listener_stop');
      });
    });

    it('renders for yielded axis too', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'ft8_list_devices') return [DEV_A];
        if (cmd === 'ft8_device_meter') return { rmsDbfs: -20, state: 'live' };
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        return undefined;
      });
      const { container } = render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({ service: { axis: 'yielded' }, configuredDeviceName: DEV_A.humanName })}
        />,
      );
      await waitFor(() => expect(container.textContent).not.toBe(''));
      expect(screen.getByTestId('ft8-setup-stop-cta')).toBeInTheDocument();
    });
  });

  describe('defensive rendering', () => {
    it('renders nothing for stopped/stopping axes (never mounted this way in prod, but must not crash)', () => {
      const { container } = render(
        <Ft8SetupSurface snapshot={makeSnapshot({ service: { axis: 'stopped' } })} />,
      );
      expect(container.textContent).toBe('');
    });
  });

  // ── Task C9b: Step 2 · Rig control (CAT) · Test CAT ──────────────────────

  describe('Step 2 · Test CAT', () => {
    it('renders the shared RigControlSection as Step 2, storageKeyPrefix="ft8"', async () => {
      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      expect(screen.getByTestId('ft8-setup-step2-head').textContent).toMatch(/Step 2/);
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('config_get_rig');
      });
      // localStorage key is namespaced "ft8" — distinct from ardop/vara.
      expect(screen.getByTestId('ft8-setup-test-cat')).toBeInTheDocument();
    });

    it('awaits commitNow (flushes an unblurred CAT-serial edit) BEFORE calling ft8_cat_probe', async () => {
      const callOrder: string[] = [];
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        callOrder.push(cmd);
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_cat_probe') return { dialHz: 14074000, band: '20m' };
        return undefined;
      });

      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      const manual = (await screen.findByTestId('rig-cat-port-manual')) as HTMLInputElement;
      await waitFor(() => expect(manual.value).toBe('/dev/ttyUSB0'));

      callOrder.length = 0;
      // Type a NEW value but do NOT blur — commitCatSerial only fires on
      // blur or an explicit commitNow() call.
      fireEvent.change(manual, { target: { value: '/dev/ttyUSB9' } });
      expect(callOrder).not.toContain('config_set_rig');

      fireEvent.click(screen.getByTestId('ft8-setup-test-cat'));

      await waitFor(() => {
        expect(callOrder).toContain('ft8_cat_probe');
      });
      // The unblurred edit reached the backend BEFORE the probe fired — a
      // just-typed serial path must never false-fail the probe.
      const setIdx = callOrder.indexOf('config_set_rig');
      const probeIdx = callOrder.indexOf('ft8_cat_probe');
      expect(setIdx).toBeGreaterThanOrEqual(0);
      expect(setIdx).toBeLessThan(probeIdx);
      const setCall = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === 'config_set_rig');
      expect((setCall?.[1] as { value: RigConfig }).value.cat_serial_path).toBe('/dev/ttyUSB9');
    });

    it('Test CAT success shows the dial/band from ft8_cat_probe', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_cat_probe') return { dialHz: 14074000, band: '20m' };
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      await screen.findByTestId('ft8-setup-test-cat');
      fireEvent.click(screen.getByTestId('ft8-setup-test-cat'));
      await waitFor(() => {
        const success = screen.getByTestId('ft8-setup-cat-success');
        expect(success.textContent).toMatch(/14\.074/);
        expect(success.textContent).toMatch(/20m/);
      });
      expect(screen.queryByTestId('ft8-setup-cat-error')).toBeNull();
    });

    it('shows the modem-busy reason (never the raw detail) on a busy-radio probe failure', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_cat_probe') {
          return Promise.reject({ kind: 'modem-busy', detail: 'ardop session active on ttyUSB0' });
        }
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      await screen.findByTestId('ft8-setup-test-cat');
      fireEvent.click(screen.getByTestId('ft8-setup-test-cat'));
      await waitFor(() => {
        const err = screen.getByTestId('ft8-setup-cat-error');
        expect(err.textContent).toMatch(/busy/i);
        expect(err.textContent).toMatch(/disconnect/i);
        // Never parses/surfaces the raw detail string — kind-keyed copy only.
        expect(err.textContent).not.toMatch(/ttyUSB0/);
      });
    });

    it('shows the rig-not-configured reason on a probe failure with no Config.rig', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_cat_probe') {
          return Promise.reject({ kind: 'rig-not-configured', detail: 'no Config.rig' });
        }
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      await screen.findByTestId('ft8-setup-test-cat');
      fireEvent.click(screen.getByTestId('ft8-setup-test-cat'));
      await waitFor(() => {
        expect(screen.getByTestId('ft8-setup-cat-error').textContent).toMatch(/no radio configured/i);
      });
    });

    it('shows the probe-timeout reason on a timed-out probe', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === 'config_get_rig') return KNOWN_RIG_CONFIG;
        if (cmd === 'rig_list_models') return [];
        if (cmd === 'packet_list_serial_devices') return [];
        if (cmd === 'ft8_cat_probe') {
          return Promise.reject({ kind: 'probe-timeout', detail: 'no response in 3s' });
        }
        return undefined;
      });
      render(<Ft8SetupSurface snapshot={makeSnapshot()} />);
      await screen.findByTestId('ft8-setup-test-cat');
      fireEvent.click(screen.getByTestId('ft8-setup-test-cat'));
      await waitFor(() => {
        const err = screen.getByTestId('ft8-setup-cat-error');
        expect(err.textContent).toMatch(/didn.t respond/i);
        expect(err.textContent).toMatch(/cat cable/i);
      });
    });
  });

  // ── Task C9b: CTA `Start listening on <band> →` disable-reason matrix ────

  describe('CTA `Start listening on <band> →`', () => {
    it('renders the band in the label', () => {
      render(<Ft8SetupSurface snapshot={makeSnapshot({ band: '40m' })} />);
      expect(screen.getByTestId('ft8-setup-start-cta').textContent).toMatch(/40m/);
    });

    it('disabled + "install wsjt-x" reason for the wsjtx-absent blocker', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'wsjtx-absent' },
            configuredDeviceName: 'USB Audio CODEC',
          })}
        />,
      );
      const cta = screen.getByTestId('ft8-setup-start-cta') as HTMLButtonElement;
      expect(cta.disabled).toBe(true);
      expect(screen.getByTestId('ft8-setup-cta-blocked-reason').textContent).toMatch(/wsjt-x/i);
    });

    it('disabled + "choose a supported audio input" reason for unsupported-sample-rate', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'unsupported-sample-rate' },
            configuredDeviceName: 'USB Audio CODEC',
            availableDevices: null,
          })}
        />,
      );
      const cta = screen.getByTestId('ft8-setup-start-cta') as HTMLButtonElement;
      expect(cta.disabled).toBe(true);
      expect(screen.getByTestId('ft8-setup-cta-blocked-reason').textContent).toMatch(
        /supported audio input/i,
      );
    });

    it('disabled + "select an audio input" reason when no device is configured', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            configuredDeviceName: null,
          })}
        />,
      );
      const cta = screen.getByTestId('ft8-setup-start-cta') as HTMLButtonElement;
      expect(cta.disabled).toBe(true);
      expect(screen.getByTestId('ft8-setup-cta-blocked-reason').textContent).toMatch(
        /select an audio input/i,
      );
    });

    it('disabled + "restart Tuxlink" reason for capture-wedged (defensive — not reachable via the normal mounting contract)', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'capture-wedged' },
            configuredDeviceName: 'USB Audio CODEC',
          })}
        />,
      );
      const cta = screen.getByTestId('ft8-setup-start-cta') as HTMLButtonElement;
      expect(cta.disabled).toBe(true);
      expect(screen.getByTestId('ft8-setup-cta-blocked-reason').textContent).toMatch(/restart tuxlink/i);
    });

    it('enabled once a device is resolved and no other blocker applies; clicking calls ft8_listener_start', async () => {
      // A synthetic combination (device already configured while the arm is
      // still needs-device-selection) — exercises the "no blocker" branch
      // that a real snapshot only reaches transiently, right before this
      // surface unmounts via onStarted.
      const onStarted = vi.fn();
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'needs-device-selection' },
            configuredDeviceName: 'USB Audio CODEC',
          })}
          onStarted={onStarted}
        />,
      );
      const cta = screen.getByTestId('ft8-setup-start-cta') as HTMLButtonElement;
      expect(cta.disabled).toBe(false);
      expect(screen.queryByTestId('ft8-setup-cta-blocked-reason')).toBeNull();

      fireEvent.click(cta);
      await waitFor(() => {
        expect(vi.mocked(invoke)).toHaveBeenCalledWith('ft8_listener_start');
      });
      await waitFor(() => {
        expect(onStarted).toHaveBeenCalled();
      });
    });

    it('a click while disabled never invokes ft8_listener_start (guarded, never a silent re-render)', () => {
      render(
        <Ft8SetupSurface
          snapshot={makeSnapshot({
            service: { axis: 'blocked', reason: 'wsjtx-absent' },
            configuredDeviceName: null,
          })}
        />,
      );
      fireEvent.click(screen.getByTestId('ft8-setup-start-cta'));
      expect(vi.mocked(invoke)).not.toHaveBeenCalledWith('ft8_listener_start');
    });
  });
});
