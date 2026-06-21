import { describe, it, expect } from 'vitest';
import {
  bucketForStation,
  BUCKETS,
  ALL_BUCKET_KEYS,
  emptyCounts,
  type BucketKey,
} from './stationBuckets';

const b = (symbolTable: string, symbolCode: string, isWeather = false): BucketKey =>
  bucketForStation({ symbolTable, symbolCode, isWeather });

// Every printable APRS code, ! (0x21) .. ~ (0x7E).
const CODES = Array.from({ length: 0x7e - 0x21 + 1 }, (_, i) => String.fromCharCode(0x21 + i));

describe('stationBuckets metadata', () => {
  it('exposes 8 ordered buckets with unique keys', () => {
    expect(BUCKETS.map((m) => m.key)).toEqual([
      'weather', 'igate', 'digipeater', 'emergency', 'vehicles', 'people', 'fixed', 'other',
    ]);
  });
  it('ALL_BUCKET_KEYS mirrors BUCKETS order', () => {
    expect(ALL_BUCKET_KEYS).toEqual(BUCKETS.map((m) => m.key));
  });
  it('every bucket has a label and a glyph', () => {
    for (const m of BUCKETS) {
      expect(m.label.length).toBeGreaterThan(0);
      expect(m.glyph.length).toBeGreaterThan(0);
    }
  });
  it('emptyCounts has every key at 0', () => {
    const c = emptyCounts();
    expect(Object.keys(c).sort()).toEqual([...ALL_BUCKET_KEYS].sort());
    expect(Object.values(c).every((n) => n === 0)).toBe(true);
  });
});

describe('bucketForStation — total coverage (no throw, always a valid bucket)', () => {
  it('classifies every primary-table code into a known bucket', () => {
    for (const code of CODES) expect(ALL_BUCKET_KEYS).toContain(b('/', code));
  });
  it('classifies every alternate-table code into a known bucket', () => {
    for (const code of CODES) expect(ALL_BUCKET_KEYS).toContain(b('\\', code));
  });
});

describe('bucketForStation — weather override', () => {
  it('a station with valid WX readings is Weather regardless of symbol', () => {
    expect(b('/', '>', /* isWeather */ true)).toBe('weather'); // a car reporting weather
  });
  it('the weather symbol itself is Weather', () => {
    expect(b('/', '_')).toBe('weather');
    expect(b('\\', '_')).toBe('weather');
    expect(b('/', 'W')).toBe('weather');
  });
  it('weather-condition objects are Weather', () => {
    expect(b('\\', 't')).toBe('weather'); // tornado
    expect(b('\\', ':')).toBe('weather'); // hail
    expect(b('\\', 'w')).toBe('weather'); // flooding
    expect(b('\\', '@')).toBe('weather'); // hurricane
  });
});

describe('bucketForStation — infrastructure', () => {
  it('digipeaters', () => {
    expect(b('/', '#')).toBe('digipeater');
    expect(b('\\', '#')).toBe('digipeater');
    expect(b('/', 'r')).toBe('digipeater'); // repeater
    expect(b('/', 'n')).toBe('digipeater'); // node
    expect(b('\\', '8')).toBe('digipeater'); // network node
    expect(b('W', '#')).toBe('digipeater'); // WIDEn-N overlay → falls through to alt '#'
    expect(b('D', 'a')).toBe('digipeater'); // D-STAR overlay
    expect(b('Y', 'a')).toBe('digipeater'); // C4FM repeater overlay
  });
  it('iGates / gateways', () => {
    expect(b('/', '&')).toBe('igate'); // HF gateway
    expect(b('\\', '&')).toBe('igate'); // igate
    expect(b('/', 'I')).toBe('igate'); // TCP/IP
    expect(b('I', '&')).toBe('igate'); // I& overlay → falls through to alt '&'
    expect(b('R', '&')).toBe('igate');
  });
});

describe('bucketForStation — emergency / emcomm', () => {
  it('served-agency, incident, and ARES/RACES symbols', () => {
    expect(b('\\', '!')).toBe('emergency'); // emergency
    expect(b('/', 'A')).toBe('emergency'); // aid station
    expect(b('/', 'o')).toBe('emergency'); // EOC
    expect(b('/', 'c')).toBe('emergency'); // incident command
    expect(b('/', 'a')).toBe('emergency'); // ambulance
    expect(b('/', 'f')).toBe('emergency'); // fire truck
    expect(b('/', '+')).toBe('emergency'); // red cross
    expect(b('/', '!')).toBe('emergency'); // police
    expect(b('/', 'P')).toBe('emergency'); // police
    expect(b('\\', 'C')).toBe('emergency'); // coast guard
    expect(b('A', 'a')).toBe('emergency'); // ARES overlay → alt 'a'
    expect(b('\\', 'a')).toBe('emergency'); // ARRL/ARES/WinLink base
  });
});

describe('bucketForStation — vehicles, people, fixed', () => {
  it('vehicles include aircraft and boats', () => {
    expect(b('/', '>')).toBe('vehicles'); // car
    expect(b('/', 'k')).toBe('vehicles'); // truck
    expect(b('/', '^')).toBe('vehicles'); // large aircraft
    expect(b('/', 'X')).toBe('vehicles'); // helicopter
    expect(b('/', 'Y')).toBe('vehicles'); // yacht
    expect(b('\\', 's')).toBe('vehicles'); // ship/boat
    expect(b('B', '>')).toBe('vehicles'); // EV overlay → alt '>'
  });
  it('people', () => {
    expect(b('/', '[')).toBe('people'); // person
    expect(b('/', 'b')).toBe('people'); // bicycle
    expect(b('/', ')')).toBe('people'); // wheelchair
    expect(b('/', 'e')).toBe('people'); // horse
  });
  it('fixed / places', () => {
    expect(b('/', '-')).toBe('fixed'); // house
    expect(b('/', 'h')).toBe('fixed'); // hospital
    expect(b('/', 'K')).toBe('fixed'); // school
    expect(b('\\', 'R')).toBe('fixed'); // restaurant
    expect(b('\\', '%')).toBe('fixed'); // power plant
    expect(b('S', '-')).toBe('fixed'); // solar house overlay → alt '-'
  });
});

describe('bucketForStation — other catch-all', () => {
  it('unknown / undefined symbols fall to other and never throw', () => {
    expect(b('/', 'J')).toBe('other'); // undefined
    expect(b('\\', 'Z')).toBe('other'); // undefined
    expect(b('/', '?')).toBe('other'); // file server
    expect(b('@', '@')).toBe('other'); // nonsense overlay/code combo
    expect(b('', '')).toBe('other'); // malformed
  });
});
