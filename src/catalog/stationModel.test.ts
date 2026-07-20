import { describe, it, expect } from 'vitest';
import { baseCallsign, aggregateStations, stationMatchesFilters, formatDialKhz, type Station } from './stationModel';
import { HF_BANDS, type Band } from './bandPlan';
import { BANDWIDTH_CLASSES, type BandwidthClass, type ChannelDetail, type Gateway, type StationListing } from './stationTypes';

function gw(partial: Partial<Gateway> & { callsign: string }): Gateway {
  return {
    channel: partial.callsign, callsign: partial.callsign,
    sysopName: partial.sysopName ?? null, grid: partial.grid === undefined ? 'DM34oa' : partial.grid,
    location: partial.location ?? null, frequenciesKhz: partial.frequenciesKhz ?? [],
    lastUpdate: partial.lastUpdate ?? null, email: null, homepage: null,
    antenna: partial.antenna ?? null,
    channelDetails: partial.channelDetails,
  };
}

function detail(partial: Partial<ChannelDetail> & Pick<ChannelDetail, 'frequencyKhz' | 'mode'>): ChannelDetail {
  return {
    bandwidthHz: partial.bandwidthHz ?? null,
    operatingHours: partial.operatingHours ?? null,
    grid: partial.grid ?? null,
    ...partial,
  };
}

const ALL_BANDWIDTHS = new Set<BandwidthClass>(BANDWIDTH_CLASSES);

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

describe('aggregateStations — gateway antenna carry-through (tuxlink-s0r1 #3d)', () => {
  it('carries a gateway antenna code onto the station', () => {
    const s = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', antenna: 'vertical', frequenciesKhz: [7103] })] },
    ])[0];
    expect(s.gatewayAntenna).toBe('vertical');
  });

  it('defaults gatewayAntenna to null when no listing carries a code', () => {
    const s = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103] })] },
    ])[0];
    expect(s.gatewayAntenna).toBeNull();
  });

  it('fills gatewayAntenna from a later listing when the first had none', () => {
    const s = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103] })] },
      { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', antenna: 'dipole', frequenciesKhz: [7103] })] },
    ])[0];
    expect(s.gatewayAntenna).toBe('dipole');
  });
});

describe('stationMatchesFilters: band+mode FILTER (tuxlink-hlas)', () => {
  // A real N0DAJ-shaped station: 40m VARA/ARDOP HF + 145 MHz / 441 MHz packet.
  const station: Station = aggregateStations([
    { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103, 14103] })] },
    { mode: 'packet', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'N0DAJ-10', grid: 'DM34oa', frequenciesKhz: [145710] })] },
  ])[0];

  // A pure-VHF packet station whose only channel is 145 MHz, the kind that was
  // wrongly surfacing under the default 40m selection.
  const vhfOnly: Station = aggregateStations([
    { mode: 'packet', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
      gateways: [gw({ callsign: 'W7VHF-1', grid: 'DM43aa', frequenciesKhz: [145010] })] },
  ])[0];

  const HF_DEFAULT = new Set<Band>(HF_BANDS);

  it('matches when a channel is on a selected band AND an enabled mode (bandwidth wide open)', () => {
    expect(stationMatchesFilters(station, new Set<Band>(['40m']), new Set(['vara-hf']), ALL_BANDWIDTHS)).toBe(true);
  });

  it('does NOT surface a 145 MHz packet station under the default HF band set (THE bug)', () => {
    // band === 'vhf-uhf' is not in the HF default → the station has no matching channel.
    expect(stationMatchesFilters(vhfOnly, HF_DEFAULT, new Set(['packet']), ALL_BANDWIDTHS)).toBe(false);
  });

  it('DOES surface that same 145 MHz packet station once VHF/UHF is selected', () => {
    expect(stationMatchesFilters(vhfOnly, new Set<Band>(['vhf-uhf']), new Set(['packet']), ALL_BANDWIDTHS)).toBe(true);
  });

  it('does NOT match when the band is selected but the station has no channel in an enabled mode', () => {
    // 40m is selected, but the only 40m channel is VARA: filtering to packet alone misses it.
    expect(stationMatchesFilters(station, new Set<Band>(['40m']), new Set(['packet']), ALL_BANDWIDTHS)).toBe(false);
  });

  it('matches the multi-mode station across either of its bands (multi-select)', () => {
    // VHF/UHF picks up the 145 MHz packet channel; 40m would pick up VARA.
    expect(stationMatchesFilters(station, new Set<Band>(['vhf-uhf']), new Set(['packet']), ALL_BANDWIDTHS)).toBe(true);
    expect(stationMatchesFilters(station, new Set<Band>(['20m']), new Set(['vara-hf']), ALL_BANDWIDTHS)).toBe(true);
  });
});

describe('stationMatchesFilters: bandwidth FILTER (Task 9, load-bearing unknown-bandwidth rule)', () => {
  const HF_DEFAULT = new Set<Band>(HF_BANDS);
  const ALL_MODES = new Set(['vara-hf', 'ardop-hf']);

  it('drops a station whose only 20m VARA channel is 500 Hz when bandwidths = {2300, 2750}', () => {
    const station = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({
          callsign: 'N0DAJ', grid: 'DM34oa',
          channelDetails: [detail({ frequencyKhz: 14103, mode: 'vara-hf', bandwidthHz: 500 })],
        })] },
    ])[0];
    const narrowOnly = new Set<BandwidthClass>(['2300', '2750']);
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, narrowOnly)).toBe(false);
    // Sanity: the SAME station matches once 500 Hz is included.
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, new Set<BandwidthClass>(['500']))).toBe(true);
  });

  it('keeps a null-bandwidth channel under any subset, including an empty one', () => {
    // Text-listing-only data: no channelDetails, so bandwidthHz is never set.
    const station = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [14103] })] },
    ])[0];
    expect(station.channels[0].bandwidthHz).toBeUndefined();
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, new Set<BandwidthClass>(['500']))).toBe(true);
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, new Set<BandwidthClass>(['2750']))).toBe(true);
    // Even a wholly empty bandwidth selection (every chip off) does not
    // exclude a channel the chips have no known classification for.
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, new Set<BandwidthClass>())).toBe(true);
  });

  it('keeps the station if ANY channel passes (one known-mismatched, one null-bandwidth)', () => {
    const station = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({
          callsign: 'N0DAJ', grid: 'DM34oa',
          channelDetails: [detail({ frequencyKhz: 14103, mode: 'vara-hf', bandwidthHz: 500 })],
        })] },
      { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        // No channelDetails on this gateway → falls back to frequenciesKhz, so
        // this channel's bandwidthHz is unset (null-equivalent).
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103] })] },
    ])[0];
    const narrowOnly = new Set<BandwidthClass>(['2300', '2750']);
    // The 500 Hz VARA channel fails; the null-bandwidth ARDOP channel passes,
    // and its band+mode also match, so the station stays visible.
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, narrowOnly)).toBe(true);
  });

  it('excludes a station entirely once its only channel is a known, non-matching bandwidth', () => {
    const station = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({
          callsign: 'K7UAZ', grid: 'DM43aa',
          channelDetails: [detail({ frequencyKhz: 14103, mode: 'vara-hf', bandwidthHz: 2300 })],
        })] },
    ])[0];
    expect(stationMatchesFilters(station, HF_DEFAULT, ALL_MODES, new Set<BandwidthClass>(['500']))).toBe(false);
  });
});

describe('aggregateStations: channelDetails preferred over frequenciesKhz (Task 8 wire shape, Task 9 consumer)', () => {
  it('sources channels from channelDetails when present, one Channel per detail, carrying bandwidthHz + per-channel mode', () => {
    const s = aggregateStations([
      { mode: 'vara-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({
          callsign: 'N0DAJ', grid: 'DM34oa',
          // frequenciesKhz present too; channelDetails must win, not merge.
          frequenciesKhz: [99999],
          channelDetails: [
            detail({ frequencyKhz: 7103, mode: 'vara-hf', bandwidthHz: 2300, grid: 'DM34oa' }),
            detail({ frequencyKhz: 14103, mode: 'vara-hf', bandwidthHz: 500 }),
          ],
        })] },
    ])[0];
    expect(s.channels).toHaveLength(2);
    expect(s.channels.some((c) => c.frequencyKhz === 99999)).toBe(false);
    const narrow = s.channels.find((c) => c.frequencyKhz === 7103);
    expect(narrow?.bandwidthHz).toBe(2300);
    expect(narrow?.mode).toBe('vara-hf');
    expect(narrow?.band).toBe('40m');
    const wide = s.channels.find((c) => c.frequencyKhz === 14103);
    expect(wide?.bandwidthHz).toBe(500);
  });

  it('falls back to frequenciesKhz expansion when a gateway carries no channelDetails', () => {
    const s = aggregateStations([
      { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103, 14103] })] },
    ])[0];
    expect(s.channels).toHaveLength(2);
    expect(s.channels.every((c) => c.mode === 'ardop-hf')).toBe(true);
    expect(s.channels.every((c) => c.bandwidthHz === undefined)).toBe(true);
  });

  it('falls back to frequenciesKhz expansion when channelDetails is an empty array', () => {
    const s = aggregateStations([
      { mode: 'ardop-hf', title: null, parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({ callsign: 'N0DAJ', grid: 'DM34oa', frequenciesKhz: [7103], channelDetails: [] })] },
    ])[0];
    expect(s.channels).toHaveLength(1);
    expect(s.channels[0].frequencyKhz).toBe(7103);
  });

  it('a VARA FM synthesized listing (frequenciesKhz empty, only channelDetails) still aggregates', () => {
    const s = aggregateStations([
      { mode: 'vara-fm', title: 'synth', parsedOk: true, raw: '', fetchedAtMs: 1,
        gateways: [gw({
          callsign: '4F1PUZ-10', grid: 'DM34oa', frequenciesKhz: [],
          channelDetails: [detail({ frequencyKhz: 145710, mode: 'vara-fm', bandwidthHz: null })],
        })] },
    ])[0];
    expect(s.channels).toHaveLength(1);
    expect(s.channels[0].mode).toBe('vara-fm');
    expect(s.modes).toContain('vara-fm');
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

// Task 10 (tuxlink-hcmfb): formatDialKhz backs the rail's frequency hero and
// every BandMatrix channel row's kHz label.
describe('formatDialKhz', () => {
  it('renders a fractional-kHz dial with one decimal and thousands separator', () => {
    expect(formatDialKhz(7103.5)).toBe('7,103.5 kHz');
  });
  it('renders a whole-kHz dial with a trailing .0 (never drops the decimal)', () => {
    expect(formatDialKhz(14108)).toBe('14,108.0 kHz');
  });
});
