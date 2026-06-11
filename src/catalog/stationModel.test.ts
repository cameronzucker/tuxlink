import { describe, it, expect } from 'vitest';
import { baseCallsign, aggregateStations, type Station } from './stationModel';
import type { Gateway, StationListing } from './stationTypes';

function gw(partial: Partial<Gateway> & { callsign: string }): Gateway {
  return {
    channel: partial.callsign, callsign: partial.callsign,
    sysopName: partial.sysopName ?? null, grid: partial.grid === undefined ? 'DM34oa' : partial.grid,
    location: partial.location ?? null, frequenciesKhz: partial.frequenciesKhz ?? [],
    lastUpdate: partial.lastUpdate ?? null, email: null, homepage: null,
  };
}

describe('baseCallsign', () => {
  it('strips an SSID suffix', () => {
    expect(baseCallsign('N0DAJ-10')).toBe('N0DAJ');
    expect(baseCallsign('N0DAJ')).toBe('N0DAJ');
    expect(baseCallsign('w7ara-1')).toBe('W7ARA');
  });
});

describe('aggregateStations — N0DAJ multi-mode/SSID (spec §3)', () => {
  const listings: StationListing[] = [
    { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', sysopName: 'Doug Jarmuth', location: 'Wickenburg, AZ',
        frequenciesKhz: [3590, 7103, 7108, 10147, 14103, 14115] })] },
    { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [3590, 7103, 7108, 14103, 14115] })] },
    { mode: 'packet', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [
        gw({ callsign: 'N0DAJ-10', grid: 'DM34oa', frequenciesKhz: [145710] }),
        gw({ callsign: 'N0DAJ-11', grid: 'DM34oa', frequenciesKhz: [145010] }),
        gw({ callsign: 'N0DAJ-12', grid: 'DM34oa', frequenciesKhz: [441300] }),
      ] },
  ];

  it('collapses all listings into one station pin keyed by base call + grid', () => {
    const stations = aggregateStations(listings);
    expect(stations).toHaveLength(1);
    const s = stations[0];
    expect(s.baseCallsign).toBe('N0DAJ');
    expect(s.grid).toBe('DM34oa');
    expect(s.sysopName).toBe('Doug Jarmuth');
    expect(s.location).toBe('Wickenburg, AZ');
    expect(s.modes.slice().sort()).toEqual(['ardop-hf', 'packet', 'vara-hf']);
  });

  it('expands each mode-listing frequency into a channel; shared dial under two modes = two channels', () => {
    const s = aggregateStations(listings)[0];
    const vara7103 = s.channels.filter((c) => c.mode === 'vara-hf' && c.frequencyKhz === 7103);
    const ardop7103 = s.channels.filter((c) => c.mode === 'ardop-hf' && c.frequencyKhz === 7103);
    expect(vara7103).toHaveLength(1);
    expect(ardop7103).toHaveLength(1);
    expect(s.channels.filter((c) => c.mode === 'vara-hf')).toHaveLength(6);
  });

  it('carries the SSID as the packet connect target', () => {
    const s = aggregateStations(listings)[0];
    const pkt = s.channels.filter((c) => c.mode === 'packet');
    expect(pkt.map((c) => c.ssid).slice().sort()).toEqual(['N0DAJ-10', 'N0DAJ-11', 'N0DAJ-12']);
    expect(pkt.find((c) => c.frequencyKhz === 145710)?.ssid).toBe('N0DAJ-10');
  });

  it('tags each channel with its band', () => {
    const s = aggregateStations(listings)[0];
    expect(s.channels.find((c) => c.mode === 'vara-hf' && c.frequencyKhz === 7103)?.band).toBe('40m');
    expect(s.channels.find((c) => c.mode === 'packet' && c.frequencyKhz === 145710)?.band).toBe('vhf-uhf');
  });
});

describe('aggregateStations — distinct stations', () => {
  it('keeps stations with different base calls separate', () => {
    const listings: StationListing[] = [
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103] }),
                   gw({ callsign: 'K7UAZ', grid: 'DM43aa', frequenciesKhz: [7103] })] },
    ];
    const stations = aggregateStations(listings);
    expect(stations.map((s: Station) => s.baseCallsign).slice().sort()).toEqual(['K7UAZ', 'N0DAJ']);
  });
  it('degrades to empty for a non-array (malformed backend response)', () => {
    expect(aggregateStations(undefined as unknown as StationListing[])).toEqual([]);
    expect(aggregateStations(null as unknown as StationListing[])).toEqual([]);
  });
  it('drops gateways with no grid (cannot place on map)', () => {
    const listings: StationListing[] = [
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'NOGRID', grid: null, frequenciesKhz: [7103] })] },
    ];
    expect(aggregateStations(listings)).toHaveLength(0);
  });
});
