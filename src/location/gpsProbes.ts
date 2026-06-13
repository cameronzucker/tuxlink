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
}

/** Best human label for a serial device, falling back to the path. */
export function serialDeviceLabel(d: SerialDevice): string {
  if (d.model && d.vendor) return `${d.vendor} ${d.model}`;
  return d.model ?? d.vendor ?? 'Serial device';
}

/**
 * Turn raw probe results into source-cards (working) + triage-cards (blocked).
 *
 * Sources (something usable to read a position from):
 * - gpsd reachable → a source card (the easy path).
 * - serial devices present AND user is in `dialout` → one source card each.
 *
 * Triage is DEVICE-INDEPENDENT (tuxlink-yy1m). The original design gated triage
 * on a serial device already being enumerated, but on Linux the device
 * frequently won't appear *until* these are fixed (ModemManager grabs the port;
 * `dialout` blocks opening it), so the diagnostics must surface before any
 * device shows up — they're most needed exactly when nothing is detected:
 * - user NOT in `dialout` → a dialout triage card.
 * - ModemManager active → a ModemManager triage card.
 *
 * `noDevice` reports "nothing to read from yet" (no serial + gpsd down) so the
 * picker can render a "plug in + Rescan" card. Manual-grid entry is always
 * offered by the picker itself, so it's not modeled here.
 */
export function classifyGpsSources(d: GpsDetection): GpsClassification {
  const sources: GpsSourceCard[] = [];
  const triage: GpsTriageCard[] = [];

  if (d.gpsd.reachable) {
    sources.push({ id: 'gpsd', kind: 'gpsd', label: 'gpsd daemon', detail: '127.0.0.1:2947' });
  }

  const hasSerial = d.serial.devices.length > 0;
  if (hasSerial && d.dialout.member) {
    for (const dev of d.serial.devices) {
      sources.push({ id: `serial:${dev.path}`, kind: 'serial', label: serialDeviceLabel(dev), detail: dev.path });
    }
  }

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

  return { sources, triage, noDevice: !hasSerial && !d.gpsd.reachable };
}
