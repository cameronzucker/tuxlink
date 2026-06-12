import { describe, it, expect } from 'vitest';
import { groupChannelsByMode, channelToDial, channelReliability, type ChannelGroup } from './channelGrouping';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';

const channels: Channel[] = [
  { mode: 'vara-hf', frequencyKhz: 7103, band: '40m' },
  { mode: 'vara-hf', frequencyKhz: 3590, band: '80m' },
  { mode: 'ardop-hf', frequencyKhz: 7103, band: '40m' },
  { mode: 'packet', frequencyKhz: 145710, ssid: 'N0DAJ-10', band: 'vhf-uhf' },
];
const station: Station = { baseCallsign: 'N0DAJ', grid: 'DM34oa', sysopName: null, location: null,
  modes: ['vara-hf', 'ardop-hf', 'packet'], channels, fetchedAtMs: 1, gatewayAntenna: null };

describe('groupChannelsByMode', () => {
  it('groups channels under their mode, ascending by frequency', () => {
    const groups = groupChannelsByMode(station);
    const vara = groups.find((g: ChannelGroup) => g.mode === 'vara-hf')!;
    expect(vara.channels.map((c) => c.frequencyKhz)).toEqual([3590, 7103]);
    expect(groups.map((g) => g.mode)).toEqual(['vara-hf', 'ardop-hf', 'packet']);
  });
});

describe('channelToDial', () => {
  it('builds an HF dial keyed on the base call', () => {
    expect(channelToDial(station, channels[0])).toEqual({ mode: 'vara-hf', gateway: 'N0DAJ', freq: '7.103', grid: 'DM34oa' });
  });
  it('uses the SSID as the packet connect target', () => {
    expect(channelToDial(station, channels[3])).toEqual({ mode: 'packet', gateway: 'N0DAJ-10', freq: '145.710', grid: 'DM34oa' });
  });
  it('returns null for a non-prefillable mode', () => {
    const pactor: Channel = { mode: 'pactor', frequencyKhz: 7103, band: '40m' };
    expect(channelToDial(station, pactor)).toBeNull();
  });
});

describe('channelReliability', () => {
  const prediction: PathPrediction = { bearingDeg: 0, distanceKm: 1, ssn: 118, year: 2026, month: 6,
    channels: [{ frequencyKhz: 7103, voacapMhz: 7, relByHour: Array(24).fill(0.86), snrByHour: Array(24).fill(12), mufdayByHour: Array(24).fill(0.9) }] };
  it('returns rel + tier for an HF channel present in the prediction', () => {
    expect(channelReliability(channels[0], prediction, 21)).toEqual({ rel: 0.86, tier: 'good' });
  });
  it('returns null for a VHF/UHF channel (no model)', () => {
    expect(channelReliability(channels[3], prediction, 21)).toBeNull();
  });
  it('returns null when the prediction lacks that dial', () => {
    expect(channelReliability(channels[1], prediction, 21)).toBeNull(); // 3590 not in prediction
  });
});
