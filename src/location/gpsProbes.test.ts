import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import {
  classifyGpsSources,
  serialDeviceLabel,
  runGpsDetection,
  type GpsDetection,
  type SerialDevice,
} from './gpsProbes';

const dev = (over: Partial<SerialDevice> = {}): SerialDevice => ({
  path: '/dev/ttyACM0', vendor: null, model: null, vendorId: null, productId: null, ...over,
});

const detection = (over: Partial<GpsDetection> = {}): GpsDetection => ({
  gpsd: { reachable: false },
  serial: { devices: [] },
  dialout: { member: false, groupExists: true },
  modemManager: { active: false },
  ...over,
});

beforeEach(() => vi.mocked(invoke).mockReset());

describe('serialDeviceLabel', () => {
  it('prefers vendor + model, falls back through model, vendor, generic', () => {
    expect(serialDeviceLabel(dev({ vendor: 'u-blox AG', model: 'GNSS receiver' }))).toBe('u-blox AG GNSS receiver');
    expect(serialDeviceLabel(dev({ model: 'GNSS receiver' }))).toBe('GNSS receiver');
    expect(serialDeviceLabel(dev({ vendor: 'u-blox AG' }))).toBe('u-blox AG');
    expect(serialDeviceLabel(dev())).toBe('Serial device');
  });
});

describe('classifyGpsSources', () => {
  it('offers gpsd as a source when reachable', () => {
    const { sources } = classifyGpsSources(detection({ gpsd: { reachable: true } }));
    expect(sources).toHaveLength(1);
    expect(sources[0]).toMatchObject({ kind: 'gpsd', id: 'gpsd' });
  });

  it('offers each serial device as a source when the user is in dialout', () => {
    const { sources, triage } = classifyGpsSources(
      detection({
        serial: { devices: [dev({ path: '/dev/ttyACM0', vendor: 'u-blox AG', model: 'GNSS' }), dev({ path: '/dev/ttyUSB0' })] },
        dialout: { member: true, groupExists: true },
      }),
    );
    expect(sources.map((s) => s.detail)).toEqual(['/dev/ttyACM0', '/dev/ttyUSB0']);
    expect(sources[0].label).toBe('u-blox AG GNSS');
    expect(triage).toHaveLength(0);
  });

  it('raises a dialout triage card (not a source) when a device exists but the user is NOT in dialout — the core fix', () => {
    const { sources, triage } = classifyGpsSources(
      detection({ serial: { devices: [dev()] }, dialout: { member: false, groupExists: true } }),
    );
    expect(sources).toHaveLength(0);
    expect(triage).toHaveLength(1);
    expect(triage[0]).toMatchObject({ kind: 'dialout', fixable: true });
    expect(triage[0].command).toContain('usermod -aG dialout');
  });

  it('marks the dialout fix non-fixable when the group does not exist', () => {
    const { triage } = classifyGpsSources(
      detection({ serial: { devices: [dev()] }, dialout: { member: false, groupExists: false } }),
    );
    expect(triage[0].fixable).toBe(false);
  });

  it('raises a ModemManager triage card when it is active and a serial device exists', () => {
    const { triage } = classifyGpsSources(
      detection({ serial: { devices: [dev()] }, dialout: { member: true, groupExists: true }, modemManager: { active: true } }),
    );
    expect(triage.map((t) => t.kind)).toContain('modemmanager');
    expect(triage.find((t) => t.kind === 'modemmanager')!.command).toContain('systemctl mask ModemManager');
  });

  it('does not raise serial triage cards when there are no serial devices', () => {
    const { sources, triage } = classifyGpsSources(detection({ modemManager: { active: true } }));
    expect(sources).toHaveLength(0);
    expect(triage).toHaveLength(0);
  });
});

describe('runGpsDetection', () => {
  it('runs all four probes in parallel and aggregates', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'gps_probe_gpsd': return { reachable: true } as unknown as never;
        case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
        case 'gps_probe_dialout': return { member: true, groupExists: true } as unknown as never;
        case 'gps_probe_modemmanager': return { active: false } as unknown as never;
        default: return undefined as unknown as never;
      }
    });
    const d = await runGpsDetection();
    expect(d.gpsd.reachable).toBe(true);
    expect(d.dialout.member).toBe(true);
    expect(invoke).toHaveBeenCalledTimes(4);
  });
});
