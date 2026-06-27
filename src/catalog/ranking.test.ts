import { describe, it, expect } from 'vitest';
import { rankedDialsFor, RANKED_DIALS_CAP } from './ranking';
import { bandForKhz } from './bandPlan';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';

function ardopChannel(frequencyKhz: number): Channel {
  return { mode: 'ardop-hf', frequencyKhz, band: bandForKhz(frequencyKhz) };
}

function station(channels: Channel[]): Station {
  return {
    baseCallsign: 'W7DG',
    grid: 'DM34',
    sysopName: null,
    location: null,
    modes: ['ardop-hf'],
    channels,
    fetchedAtMs: null,
    gatewayAntenna: null,
  };
}

/** Build a PathPrediction giving each frequency a flat reliability across hours. */
function prediction(rels: Record<number, number>): PathPrediction {
  return {
    bearingDeg: 0,
    distanceKm: 100,
    ssn: 50,
    year: 2026,
    month: 6,
    channels: Object.entries(rels).map(([khz, rel]) => ({
      frequencyKhz: Number(khz),
      voacapMhz: Number(khz) / 1000,
      relByHour: new Array(24).fill(rel),
      snrByHour: new Array(24).fill(10),
      mufdayByHour: new Array(24).fill(1),
    })),
  };
}

describe('rankedDialsFor', () => {
  // 7103 kHz (40m), 14105 kHz (20m), 10145 kHz (30m) — freq-asc would be
  // 7103, 10145, 14105.
  const sta = station([ardopChannel(7103), ardopChannel(10145), ardopChannel(14105)]);

  it('orders by reliability DESC when a prediction is present', () => {
    // Make 20m best, 40m worst.
    const pred = prediction({ 7103: 0.1, 10145: 0.5, 14105: 0.9 });
    const dials = rankedDialsFor(sta, 'ardop-hf', pred, 12);
    expect(dials.map((d) => d.freq)).toEqual(['14.105', '10.145', '7.103']);
  });

  it('falls back to frequency-ascending order when no prediction is supplied', () => {
    const dials = rankedDialsFor(sta, 'ardop-hf');
    expect(dials.map((d) => d.freq)).toEqual(['7.103', '10.145', '14.105']);
  });

  it('falls back to frequency-ascending when prediction has no matching channels', () => {
    const pred = prediction({ 18100: 0.9 }); // a frequency the station does not offer
    const dials = rankedDialsFor(sta, 'ardop-hf', pred, 12);
    expect(dials.map((d) => d.freq)).toEqual(['7.103', '10.145', '14.105']);
  });

  it('returns an empty list when the station has no channels in the mode', () => {
    expect(rankedDialsFor(sta, 'vara-hf')).toEqual([]);
  });

  it('caps the list to RANKED_DIALS_CAP', () => {
    const many = station(
      [7103, 10145, 14105, 18100, 21100, 24910, 28100].map(ardopChannel),
    );
    const dials = rankedDialsFor(many, 'ardop-hf');
    expect(dials.length).toBe(RANKED_DIALS_CAP);
  });
});
