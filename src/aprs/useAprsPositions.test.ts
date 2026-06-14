import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

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
});
