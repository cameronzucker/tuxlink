import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mirror useAprsChat.test.ts's listen idiom: capture the registered handler per
// channel so the test can drive `aprs-position:new` payloads synchronously.
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => {
      delete handlers[name];
    });
  },
}));

import { useAprsPositions } from './useAprsPositions';

function emitPos(payload: Record<string, unknown>) {
  act(() => {
    handlers['aprs-position:new']?.({ payload });
  });
}

const BASE = {
  sender: 'KK6XYZ',
  lat: 49.05,
  lon: -72.03,
  symbolTable: '/',
  symbolCode: '-',
  comment: 'Hello',
  ambiguity: 0,
};

describe('useAprsPositions', () => {
  beforeEach(() => {
    for (const k of Object.keys(handlers)) delete handlers[k];
  });

  it('starts empty', () => {
    const { result } = renderHook(() => useAprsPositions());
    expect(result.current.positions).toEqual([]);
  });

  it('accumulates a heard position', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE);
    expect(result.current.positions).toHaveLength(1);
    const p = result.current.positions[0];
    expect(p.call).toBe('KK6XYZ');
    expect(p.lat).toBe(49.05);
    expect(p.lon).toBe(-72.03);
    expect(p.symbolTable).toBe('/');
    expect(p.symbolCode).toBe('-');
    expect(p.comment).toBe('Hello');
    expect(typeof p.at).toBe('number');
  });

  it('dedupes by callsign, latest-position-wins', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE);
    emitPos({ ...BASE, lat: 50.0, lon: -73.0, comment: 'moved' });
    expect(result.current.positions).toHaveLength(1);
    const p = result.current.positions[0];
    expect(p.lat).toBe(50.0);
    expect(p.lon).toBe(-73.0);
    expect(p.comment).toBe('moved');
  });

  it('tracks multiple distinct stations', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE);
    emitPos({ ...BASE, sender: 'W7ABC', lat: 40.0, lon: -100.0 });
    expect(result.current.positions).toHaveLength(2);
    const calls = result.current.positions.map((p) => p.call).sort();
    expect(calls).toEqual(['KK6XYZ', 'W7ABC']);
  });

  it('carries the decoded ambiguity level (RF-honesty)', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos({ ...BASE, ambiguity: 3 });
    expect(result.current.positions[0].ambiguity).toBe(3);
  });

  it('carries the via-chain from the inbound DTO onto the heard position', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos({
      ...BASE,
      sender: 'KE7XYZ-9',
      via: [
        { call: 'W7RPT-1', repeated: true },
        { call: 'WIDE2-1', repeated: false },
      ],
    });
    expect(result.current.positions[0].via).toEqual([
      { call: 'W7RPT-1', repeated: true },
      { call: 'WIDE2-1', repeated: false },
    ]);
  });

  it('defaults via to an empty array when the payload omits it (legacy)', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE); // no via field on the payload
    expect(result.current.positions[0].via).toEqual([]);
  });

  it('labels an object/item report by its name, not the reporting sender', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    // DIGI1 reports object "LEADER" — the pin is labeled by the object name.
    emitPos({ ...BASE, sender: 'DIGI1', name: 'LEADER' });
    expect(result.current.positions).toHaveLength(1);
    expect(result.current.positions[0].call).toBe('LEADER');
  });

  it('keeps distinct objects from the same sender as separate pins', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    // One station reporting two named objects → two pins (keyed by name, not
    // collapsed onto the single reporting callsign).
    emitPos({ ...BASE, sender: 'DIGI1', name: 'LEADER', lat: 49 });
    emitPos({ ...BASE, sender: 'DIGI1', name: 'AIDSTN', lat: 40, lon: -100 });
    expect(result.current.positions).toHaveLength(2);
    expect(result.current.positions.map((p) => p.call).sort()).toEqual(['AIDSTN', 'LEADER']);
  });

  // tuxlink-xsv5 (the "drunk map"): the positions array reference MUST be stable
  // across re-renders that don't change the data. A fresh `[...byCall.values()]`
  // every render made every `[map, positions]`-keyed map effect (sprite force-
  // rebake, GeoJSON setData re-push, feature-state) re-run on EVERY parent render
  // (1s clock, 2s drafts poll, pans) → continuous re-tile + sprite re-bake storm.
  it('returns a STABLE positions reference across re-renders with no new fix (xsv5)', async () => {
    const { result, rerender } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE);
    const first = result.current.positions;
    rerender();
    rerender();
    expect(result.current.positions).toBe(first); // same REFERENCE, not just equal
  });

  it('returns a NEW reference only when the heard set actually changes (xsv5)', async () => {
    const { result } = renderHook(() => useAprsPositions());
    await act(async () => {});
    emitPos(BASE);
    const first = result.current.positions;
    emitPos({ ...BASE, sender: 'W7ABC', lat: 40, lon: -100 });
    expect(result.current.positions).not.toBe(first);
  });

  describe('staleness', () => {
    beforeEach(() => {
      vi.useFakeTimers();
      vi.setSystemTime(0);
    });
    afterEach(() => {
      vi.useRealTimers();
    });

    it('prunes a station that has gone silent past the TTL', async () => {
      const { result } = renderHook(() => useAprsPositions());
      await act(async () => {});
      emitPos(BASE); // heard at t=0
      expect(result.current.positions).toHaveLength(1);
      // 181 minutes of silence (> 3-h TTL): the prune sweep drops the pin.
      await act(async () => {
        vi.setSystemTime(181 * 60 * 1000);
        vi.advanceTimersByTime(181 * 60 * 1000);
      });
      expect(result.current.positions).toHaveLength(0);
    });

    it('keeps a station heard within the TTL', async () => {
      const { result } = renderHook(() => useAprsPositions());
      await act(async () => {});
      emitPos(BASE); // heard at t=0
      await act(async () => {
        vi.setSystemTime(10 * 60 * 1000);
        vi.advanceTimersByTime(10 * 60 * 1000);
      });
      expect(result.current.positions).toHaveLength(1);
    });
  });
});
