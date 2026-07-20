import { describe, it, expect } from 'vitest';
import {
  corroborateStations,
  evidenceRadiusMi,
  EVIDENCE_SNR_MIN_DB_DEFAULT,
  type EvidenceOptions,
} from './ft8Evidence';
import { stationKey } from './useReachabilityMap';
import type { Station } from './stationModel';
import type { SlotRecord, DecodeDto } from '../ft8ui/ft8Types';
import fixture from './__fixtures__/evidence/basic.json';

// ---------------------------------------------------------------------------
// Fixture plumbing: the JSON is the cross-language contract (Task 12 copies
// it byte-identically into the Rust fixture dir). Only `grid`, `channels[].band`
// and a per-test identity key are consulted from Station, so build a minimal
// Station per the brief's Step 3 guidance.
// ---------------------------------------------------------------------------

interface FixtureStation {
  key: string;
  grid: string;
  bands: string[];
}

interface FixtureDecode {
  grid: string;
  band: string;
  snrDb: number;
  slotUtcMs: number;
}

function buildStation(entry: FixtureStation): Station {
  return {
    baseCallsign: entry.key,
    grid: entry.grid,
    sysopName: null,
    location: null,
    modes: [],
    channels: entry.bands.map((band) => ({
      mode: 'vara-hf',
      frequencyKhz: 0,
      band: band as Station['channels'][number]['band'],
    })),
    fetchedAtMs: null,
    gatewayAntenna: null,
  };
}

/** One fixture decode -> one SlotRecord carrying exactly that decode. The
 *  algorithm only reads `slot.band` and `decode.{grid,snrDb,slotUtcMs}`, so
 *  every other SlotRecord/DecodeDto field is a plausible, unconsulted filler. */
function buildRing(decodes: FixtureDecode[]): SlotRecord[] {
  return decodes.map((d) => {
    const decodeDto: DecodeDto = {
      slotUtcMs: d.slotUtcMs,
      snrDb: d.snrDb,
      dtS: 0,
      freqHz: 0,
      message: '',
      fromCall: null,
      toCall: null,
      grid: d.grid,
      partial: false,
    };
    const slot: SlotRecord = {
      slotUtcMs: d.slotUtcMs,
      band: d.band,
      dialHz: 0,
      bandSource: 'cat-confirmed',
      bandLabelConfirmedUtcMs: null,
      outcome: { kind: 'decoded' },
      decodes: [decodeDto],
      partialSalvage: false,
      lostFrames: 0,
      boundarySkewFrames: 0,
      clipFraction: 0,
      rmsDbfs: 0,
      dwellSlotIndex: null,
    };
    return slot;
  });
}

describe('corroborateStations (fixture-backed, basic.json)', () => {
  const stations = fixture.stations.map(buildStation);
  const stationByKey = new Map(stations.map((s, i) => [fixture.stations[i].key, s]));
  const ring = buildRing(fixture.decodes);
  const opts: EvidenceOptions = {
    nowMs: fixture.nowMs,
    snrMinDb: fixture.snrMinDb,
    operatorGrid: fixture.operatorGrid,
  };

  const result = corroborateStations(stations, ring, opts);

  it('corroborates exactly the near, band-matching, fresh station', () => {
    const expected = new Set(
      fixture.expectCorroborated.map((key) => stationKey(stationByKey.get(key)!)),
    );
    expect(new Set(result.corroborated)).toEqual(expected);
  });

  it.each(fixture.stations)('discriminates the "$key" case', (entry) => {
    const key = stationKey(stationByKey.get(entry.key)!);
    const shouldBeIn = fixture.expectCorroborated.includes(entry.key);
    expect(result.corroborated.has(key)).toBe(shouldBeIn);
  });

  it('reports sampledBands from qualifying decodes only (stale decode excluded)', () => {
    expect([...result.sampledBands].sort()).toEqual([...fixture.expectSampledBands].sort());
  });

  it('considers every station passed in, regardless of outcome', () => {
    expect(result.considered).toBe(fixture.stations.length);
  });
});

describe('evidenceRadiusMi', () => {
  it('floors at EVIDENCE_RADIUS_MIN_MI for a short operator<->decode path', () => {
    expect(evidenceRadiusMi(100)).toBe(50);
  });

  it('scales linearly by EVIDENCE_RADIUS_FACTOR in the unclamped middle', () => {
    expect(evidenceRadiusMi(1500)).toBe(225);
  });

  it('caps at EVIDENCE_RADIUS_MAX_MI for a very long DX path', () => {
    expect(evidenceRadiusMi(10000)).toBe(750);
  });
});

// ---------------------------------------------------------------------------
// Supplementary semantics not exercised by the shared fixture's four cases
// (band / too-far / stale-only), but still spec'd in the brief: SNR floor and
// the "decode has no grid" exclusion. Small inline scenarios, not the shared
// cross-language fixture.
// ---------------------------------------------------------------------------

describe('corroborateStations: SNR and grid-presence gates', () => {
  const station = buildStation({ key: 'snr-probe', grid: 'DN36', bands: ['20m'] });
  const baseOpts: EvidenceOptions = {
    nowMs: 1_000_000_000,
    snrMinDb: EVIDENCE_SNR_MIN_DB_DEFAULT,
    operatorGrid: 'DN17',
  };

  it('excludes a decode below opts.snrMinDb even when co-located and in-band', () => {
    const ring = buildRing([
      { grid: 'DN36', band: '20m', snrDb: -30, slotUtcMs: baseOpts.nowMs - 60_000 },
    ]);
    const result = corroborateStations([station], ring, baseOpts);
    expect(result.corroborated.size).toBe(0);
    expect(result.sampledBands).toEqual([]);
  });

  it('includes a decode exactly at opts.snrMinDb (inclusive floor)', () => {
    const ring = buildRing([
      { grid: 'DN36', band: '20m', snrDb: EVIDENCE_SNR_MIN_DB_DEFAULT, slotUtcMs: baseOpts.nowMs - 60_000 },
    ]);
    const result = corroborateStations([station], ring, baseOpts);
    expect(result.corroborated.has(stationKey(station))).toBe(true);
  });

  it('excludes a decode with no grid, regardless of band/recency/SNR', () => {
    const ring: SlotRecord[] = [
      {
        slotUtcMs: baseOpts.nowMs - 60_000,
        band: '20m',
        dialHz: 0,
        bandSource: 'cat-confirmed',
        bandLabelConfirmedUtcMs: null,
        outcome: { kind: 'decoded' },
        decodes: [
          {
            slotUtcMs: baseOpts.nowMs - 60_000,
            snrDb: -8,
            dtS: 0,
            freqHz: 0,
            message: '',
            fromCall: null,
            toCall: null,
            grid: null,
            partial: false,
          },
        ],
        partialSalvage: false,
        lostFrames: 0,
        boundarySkewFrames: 0,
        clipFraction: 0,
        rmsDbfs: 0,
        dwellSlotIndex: null,
      },
    ];
    const result = corroborateStations([station], ring, baseOpts);
    expect(result.corroborated.size).toBe(0);
    expect(result.sampledBands).toEqual([]);
  });
});
