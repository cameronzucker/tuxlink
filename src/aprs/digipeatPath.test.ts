import { describe, it, expect } from 'vitest';
import { resolveDigipeatPath, type ResolveInput } from './digipeatPath';

const YOU = { lat: 47.0, lon: -122.0 };
const SRC = { call: 'KE7XYZ-9', lat: 48.1, lon: -122.6 };
const RPT = { lat: 47.8, lon: -122.4 };

function input(p: Partial<ResolveInput>): ResolveInput {
  return { src: SRC, via: [], located: new Map(), operator: YOU, ...p };
}

describe('resolveDigipeatPath', () => {
  it('direct no-digi frame → one solid src→you segment', () => {
    const segs = resolveDigipeatPath(input({}));
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: YOU }]);
  });

  it('all hops located → all solid', () => {
    const segs = resolveDigipeatPath(
      input({
        via: [{ call: 'W7RPT-1', repeated: true }],
        located: new Map([['W7RPT-1', RPT]]),
      }),
    );
    expect(segs).toEqual([
      { kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT },
      { kind: 'solid', from: RPT, to: YOU },
    ]);
  });

  it('mid-path unlocated hop → solid then dashed with pos? label', () => {
    const segs = resolveDigipeatPath(
      input({
        via: [
          { call: 'W7RPT-1', repeated: true },
          { call: 'WIDE2-1', repeated: true },
        ],
        located: new Map([['W7RPT-1', RPT]]),
      }),
    );
    expect(segs).toEqual([
      { kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT },
      { kind: 'dashed', from: RPT, to: YOU, unknownLabels: ['WIDE2-1'] },
    ]);
  });

  it('alias-only via, none located → dashed direct with all pos? labels', () => {
    const segs = resolveDigipeatPath(
      input({
        via: [
          { call: 'WIDE1-1', repeated: true },
          { call: 'WIDE2-1', repeated: true },
        ],
      }),
    );
    expect(segs).toEqual([
      {
        kind: 'dashed',
        from: { lat: SRC.lat, lon: SRC.lon },
        to: YOU,
        unknownLabels: ['WIDE1-1', 'WIDE2-1'],
      },
    ]);
  });

  it('non-repeated digis are ignored (not traversed)', () => {
    const segs = resolveDigipeatPath(
      input({
        via: [{ call: 'W7RPT-1', repeated: false }],
        located: new Map([['W7RPT-1', RPT]]),
      }),
    );
    // W7RPT-1 did not relay → treat as direct.
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: YOU }]);
  });

  it('no operator → terminate at last located hop', () => {
    const segs = resolveDigipeatPath(
      input({
        operator: null,
        via: [{ call: 'W7RPT-1', repeated: true }],
        located: new Map([['W7RPT-1', RPT]]),
      }),
    );
    expect(segs).toEqual([{ kind: 'solid', from: { lat: SRC.lat, lon: SRC.lon }, to: RPT }]);
  });

  it('no operator and no located downstream hop → no segments', () => {
    const segs = resolveDigipeatPath(input({ operator: null }));
    expect(segs).toEqual([]);
  });
});
