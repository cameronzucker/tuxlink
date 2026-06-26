import { describe, it, expect } from 'vitest';
import { friendlyAudioOptions, cardKey, type AlsaDeviceLike } from './audioDevices';

// Derived from a real `arecord -L` snapshot on pandora (two C-Media USB CODECs
// + the snd-aloop Loopback), as the backend `parse_alsa_devices` returns it
// (description = indented lines joined with " — ").
const PANDORA: AlsaDeviceLike[] = [
  { name: 'hw:CARD=Device,DEV=0', description: 'USB Audio Device, USB Audio — Direct hardware device without any conversions', isHardware: true },
  { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio Device, USB Audio — Hardware device with all software conversions', isHardware: true },
  { name: 'hw:CARD=Device_2,DEV=0', description: 'USB PnP Sound Device, USB Audio — Direct hardware device without any conversions', isHardware: true },
  { name: 'plughw:CARD=Device_2,DEV=0', description: 'USB PnP Sound Device, USB Audio — Hardware device with all software conversions', isHardware: true },
  { name: 'hw:CARD=Loopback,DEV=0', description: 'Loopback, Loopback PCM — Direct hardware device without any conversions', isHardware: true },
  { name: 'plughw:CARD=Loopback,DEV=1', description: 'Loopback, Loopback PCM — Hardware device with all software conversions', isHardware: true },
];

describe('friendlyAudioOptions', () => {
  it('leads with the friendly name, not the cryptic id', () => {
    const opts = friendlyAudioOptions(PANDORA);
    const dra = opts.find((o) => o.value.includes('CARD=Device,'));
    expect(dra?.primary).toBe('USB Audio Device');
    // Raw id is demoted to the secondary hint, not the primary label.
    expect(dra?.secondary).toContain('plughw:CARD=Device,DEV=0');
    expect(dra?.secondary).toContain('card "Device"');
  });

  it('collapses hw + plughw for the same card to one row, preferring plughw', () => {
    const opts = friendlyAudioOptions(PANDORA);
    const deviceRows = opts.filter((o) => /CARD=Device,/.test(o.value));
    expect(deviceRows).toHaveLength(1);
    expect(deviceRows[0].value).toBe('plughw:CARD=Device,DEV=0');
  });

  it('distinguishes the two USB CODECs by name', () => {
    const opts = friendlyAudioOptions(PANDORA);
    const primaries = opts.filter((o) => !o.isVirtual).map((o) => o.primary);
    expect(primaries).toContain('USB Audio Device');
    expect(primaries).toContain('USB PnP Sound Device');
  });

  it('classifies Loopback as virtual and sorts it after real devices', () => {
    const opts = friendlyAudioOptions(PANDORA);
    const loop = opts.find((o) => /Loopback/.test(o.value));
    expect(loop?.isVirtual).toBe(true);
    const firstVirtualIdx = opts.findIndex((o) => o.isVirtual);
    const lastRealIdx = opts.map((o) => o.isVirtual).lastIndexOf(false);
    expect(lastRealIdx).toBeLessThan(firstVirtualIdx);
  });

  it('strips the ALSA plugin-chain boilerplate from the label', () => {
    const opts = friendlyAudioOptions(PANDORA);
    expect(opts.every((o) => !/software conversions|hardware device/i.test(o.primary))).toBe(true);
  });

  it('collapses the two real USB cards to one row each (distinct Loopback subdevices stay separate)', () => {
    const opts = friendlyAudioOptions(PANDORA);
    // Device + Device_2 each collapse hw+plughw → 2 real rows. The Loopback
    // sample uses DEV=0 and DEV=1 (genuinely different subdevices) → 2 virtual.
    expect(opts.filter((o) => !o.isVirtual)).toHaveLength(2);
    expect(opts).toHaveLength(4);
  });

  it('drops non-hardware ALSA plugin chains (default, pulse, null)', () => {
    const opts = friendlyAudioOptions([
      { name: 'null', description: 'Discard all samples', isHardware: false },
      { name: 'default', description: 'Default Audio Device', isHardware: false },
      { name: 'pulse', description: 'PulseAudio Sound Server', isHardware: false },
      { name: 'plughw:CARD=Device,DEV=0', description: 'USB Audio Device, USB Audio', isHardware: true },
    ]);
    expect(opts).toHaveLength(1);
    expect(opts[0].value).toBe('plughw:CARD=Device,DEV=0');
  });

  it('handles the numeric plughw:1,0 form with a Card fallback label', () => {
    const opts = friendlyAudioOptions([
      { name: 'plughw:1,0', description: '', isHardware: true },
    ]);
    expect(opts[0].primary).toBe('Card 1');
    expect(opts[0].value).toBe('plughw:1,0');
  });
});

describe('cardKey', () => {
  it('parses CARD=Name,DEV=N', () => {
    expect(cardKey('plughw:CARD=Device,DEV=0')).toEqual({ card: 'Device', dev: '0' });
    expect(cardKey('hw:CARD=Device,DEV=0')).toEqual({ card: 'Device', dev: '0' });
  });
  it('parses the numeric form', () => {
    expect(cardKey('plughw:1,0')).toEqual({ card: '1', dev: '0' });
    expect(cardKey('hw:2')).toEqual({ card: '2', dev: '0' });
  });
  it('returns null for plugin names', () => {
    expect(cardKey('default')).toBeNull();
    expect(cardKey('pulse')).toBeNull();
  });
});
