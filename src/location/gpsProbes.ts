// GPS source detection: TypeScript bindings for the Rust probe commands
// (src-tauri/src/position/probe.rs) + the pure classification that turns raw
// probe results into the source-cards / triage-cards the GpsSourcePicker renders.
// tuxlink-9xy1 slice 1.

import { invoke } from '@tauri-apps/api/core';

// ---- wire types (mirror the Rust serde shapes) ----------------------------

export interface GpsdProbe {
  reachable: boolean;
}
export interface SerialDevice {
  path: string;
  vendor: string | null;
  model: string | null;
  vendorId: string | null;
  productId: string | null;
}
export interface SerialProbe {
  devices: SerialDevice[];
}
export interface DialoutProbe {
  member: boolean;
  groupExists: boolean;
}
export interface ModemManagerProbe {
  active: boolean;
}

export interface GpsDetection {
  gpsd: GpsdProbe;
  serial: SerialProbe;
  dialout: DialoutProbe;
  modemManager: ModemManagerProbe;
}

// ---- command bindings ------------------------------------------------------

// ---- "Fix it for me" (tuxlink-m9ej) ---------------------------------------

/** Outcome of a privileged GPS fix (mirrors the Rust GpsFixOutcome serde). */
export type GpsFixOutcome = 'ok' | 'auth_dismissed' | 'pkexec_missing' | 'failed';

/** Fixed action tokens accepted by the helper. */
export type GpsFixAction = 'add-dialout' | 'mask-modemmanager' | 'unmask-modemmanager';

/** Whether pkexec exists — gates "Fix it for me" button visibility. */
export const pkexecAvailable = () => invoke<boolean>('gps_pkexec_available');

/** Run a fixed GPS fix via pkexec + the privileged helper. */
export const runGpsFix = (action: GpsFixAction) => invoke<GpsFixOutcome>('gps_run_fix', { action });

/** One-click full gpsd setup (install + configure + enable) in a single pkexec
 *  prompt (tuxlink-n399). `device` (optional) pins the detected device. */
export const setupGpsd = (device: string | null) =>
  invoke<GpsFixOutcome>('gps_setup_gpsd', { device: device ?? null });

/** The system package manager ('apt' | 'dnf' | 'pacman') or null — drives
 *  whether the UI offers one-click setup or copy-paste guidance. */
export const pkgManager = () => invoke<string | null>('gps_pkg_manager');

/** Map a triage card kind to its fix action. Exhaustive: adding a new triage
 *  kind without a mapping is a compile error, not a silent wrong root action. */
export function triageFixAction(kind: GpsTriageCard['kind']): GpsFixAction {
  switch (kind) {
    case 'dialout':
      return 'add-dialout';
    case 'modemmanager':
      return 'mask-modemmanager';
    default: {
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}

export const probeGpsd = () => invoke<GpsdProbe>('gps_probe_gpsd');
export const probeSerialDevices = () => invoke<SerialProbe>('gps_probe_serial_devices');
export const probeDialout = () => invoke<DialoutProbe>('gps_probe_dialout');
export const probeModemManager = () => invoke<ModemManagerProbe>('gps_probe_modemmanager');

/** Run all four probes in parallel (the picker shows a single spinner). */
export async function runGpsDetection(): Promise<GpsDetection> {
  const [gpsd, serial, dialout, modemManager] = await Promise.all([
    probeGpsd(),
    probeSerialDevices(),
    probeDialout(),
    probeModemManager(),
  ]);
  return { gpsd, serial, dialout, modemManager };
}

// ---- classification (pure; unit-tested) -----------------------------------

/** A working GPS source the operator can select. */
export interface GpsSourceCard {
  /** Stable id for selection + React keys. */
  id: string;
  kind: 'gpsd' | 'serial';
  /** Short title, e.g. "gpsd daemon" or "u-blox GNSS receiver". */
  label: string;
  /** Secondary line, e.g. the device path or "127.0.0.1:2947". */
  detail: string;
}

/** A blocked source with a diagnosis + the exact command to unblock it. */
export interface GpsTriageCard {
  kind: 'dialout' | 'modemmanager';
  /** What's wrong, operator-readable. */
  title: string;
  /** One-line explanation of the consequence. */
  problem: string;
  /** The exact shell command that fixes it (shown under "Show command"). */
  command: string;
  /** Whether a "Fix it for me" one-click is possible (pkexec helper = slice 2). */
  fixable: boolean;
}

export interface GpsClassification {
  sources: GpsSourceCard[];
  triage: GpsTriageCard[];
  /** True when no serial GPS device is present AND gpsd is unreachable — i.e.
   *  there is nothing to read a position from yet. Drives the "plug in + Rescan"
   *  card (tuxlink-yy1m). Device-INDEPENDENT diagnostics (dialout/ModemManager)
   *  still surface separately, because the device frequently won't enumerate
   *  until those are fixed. */
  noDevice: boolean;
  /** Whether gpsd (the only thing that reads a GPS device) is reachable. When
   *  false, the picker shows the "Set up GPS automatically" card (tuxlink-n399).*/
  gpsdReachable: boolean;
  /** First detected serial device path (e.g. /dev/ttyACM0), to pin in gpsd's
   *  config during one-click setup, or null. */
  detectedDevice: string | null;
  /** Human label for the detected device, for the setup card. */
  detectedDeviceLabel: string | null;
}

/** Best human label for a serial device, falling back to the path. */
export function serialDeviceLabel(d: SerialDevice): string {
  if (d.model && d.vendor) return `${d.vendor} ${d.model}`;
  return d.model ?? d.vendor ?? 'Serial device';
}

/**
 * Turn raw probe results into the source card (gpsd), triage cards (blocked),
 * and the metadata the picker needs to offer gpsd setup.
 *
 * gpsd is the ONLY thing that reads a GPS device (Tuxlink has no native serial
 * reader — that's tuxlink-ley0). So:
 * - gpsd reachable → ONE source card (gpsd). A raw serial device is NOT shown as
 *   a selectable source: selecting it could never produce a fix without gpsd, so
 *   it was a dead control (tuxlink-n399). Detected devices instead inform the
 *   "Set up GPS automatically" card when gpsd is down.
 * - gpsd unreachable → no source card; `gpsdReachable=false` drives the setup card.
 *
 * Triage is DEVICE-INDEPENDENT (tuxlink-yy1m): dialout / ModemManager block gpsd
 * from reading the device and often stop it enumerating, so they surface before
 * any device appears.
 *
 * `noDevice` = nothing to read from yet (no serial + gpsd down). Manual-grid
 * entry is always offered by the picker itself, so it's not modeled here.
 */
export function classifyGpsSources(d: GpsDetection): GpsClassification {
  const sources: GpsSourceCard[] = [];
  const triage: GpsTriageCard[] = [];

  if (d.gpsd.reachable) {
    sources.push({ id: 'gpsd', kind: 'gpsd', label: 'gpsd daemon', detail: '127.0.0.1:2947' });
  }

  const hasSerial = d.serial.devices.length > 0;
  const firstDevice = hasSerial ? d.serial.devices[0] : null;

  // Device-independent: surface even with no device, since it blocks GPS the
  // moment a device appears (and often prevents it from appearing at all).
  if (!d.dialout.member) {
    triage.push({
      kind: 'dialout',
      title: 'GPS access blocked: not in the "dialout" group',
      problem:
        "Even once a GPS is plugged in, your user can't open its serial port without this. It's the #1 Linux GPS wall.",
      command: 'sudo usermod -aG dialout "$USER"   # then log out and back in',
      fixable: d.dialout.groupExists,
    });
  }

  if (d.modemManager.active) {
    triage.push({
      kind: 'modemmanager',
      title: 'ModemManager is running',
      problem:
        'ModemManager probes serial devices on connect and frequently grabs the GPS port the moment you plug it in — making the device "never appear".',
      command: 'sudo systemctl mask ModemManager   # reversible: systemctl unmask',
      fixable: true,
    });
  }

  return {
    sources,
    triage,
    noDevice: !hasSerial && !d.gpsd.reachable,
    gpsdReachable: d.gpsd.reachable,
    detectedDevice: firstDevice?.path ?? null,
    detectedDeviceLabel: firstDevice ? serialDeviceLabel(firstDevice) : null,
  };
}
