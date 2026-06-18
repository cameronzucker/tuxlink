/**
 * Tests for useActiveDownload (tuxlink-8g28) — the app-level hook the status bar
 * uses for ambient pack-download progress. `@tauri-apps/api/event` is mocked so
 * the test drives payloads synchronously (mirrors useDownloadProgress.test.ts).
 */
import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => {
      delete handlers[name];
    });
  },
}));

import { useActiveDownload } from './useActiveDownload';
import { DOWNLOAD_PROGRESS_EVENT, DOWNLOAD_DONE_EVENT } from './offlineMaps';

function emitProgress(packId: string, bytes: number, total: number) {
  act(() => {
    handlers[DOWNLOAD_PROGRESS_EVENT]?.({ payload: { packId, bytes, total } });
  });
}
function emitDone(packId: string, ok: boolean, error: string | null = null) {
  act(() => {
    handlers[DOWNLOAD_DONE_EVENT]?.({ payload: { packId, ok, error } });
  });
}

beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k];
});

describe('useActiveDownload', () => {
  it('starts null (nothing downloading)', () => {
    const { result } = renderHook(() => useActiveDownload());
    expect(result.current).toBeNull();
  });

  it('reports the active pack id and percent on progress', () => {
    const { result } = renderHook(() => useActiveDownload());
    emitProgress('continent-na', 500, 1000);
    expect(result.current?.packId).toBe('continent-na');
    expect(result.current?.bytes).toBe(500);
    expect(result.current?.percent).toBeCloseTo(0.5, 5);
    expect(result.current?.finishing).toBe(false);
  });

  it('clamps the denominator up so percent never exceeds 100%', () => {
    const { result } = renderHook(() => useActiveDownload());
    // Real extract exceeded the estimate — total clamps to bytes, percent finishes.
    emitProgress('continent-na', 1400, 1000);
    expect(result.current?.total).toBe(1400);
    expect(result.current?.finishing).toBe(true);
    expect(result.current?.percent).toBeLessThan(1);
  });

  it('clears on the terminal done event (success)', () => {
    const { result } = renderHook(() => useActiveDownload());
    emitProgress('continent-na', 500, 1000);
    expect(result.current).not.toBeNull();
    emitDone('continent-na', true);
    expect(result.current).toBeNull();
  });

  it('clears on a failed/cancelled done too (panel owns the error copy)', () => {
    const { result } = renderHook(() => useActiveDownload());
    emitProgress('continent-na', 500, 1000);
    emitDone('continent-na', false, 'go-pmtiles exit 1: boom');
    expect(result.current).toBeNull();
  });
});
