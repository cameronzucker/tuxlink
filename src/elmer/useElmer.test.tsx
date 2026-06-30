/**
 * useElmer listener-cleanup tests (tuxlink-hn5k6).
 *
 * The hook registers its EV_TURN / EV_CHIP / EV_OUTCOME listeners inside an
 * async `setupListeners()` (each `listen()` is awaited). If the effect is torn
 * down before those promises resolve — React StrictMode's mount→unmount→mount,
 * a Vite HMR re-run, or a fast unmount — the cleanup must still tear the
 * listeners down once they DO resolve, or they leak and every subsequent event
 * is handled by the orphaned set too (the doubled-output bug seen on r2-poe).
 *
 * These tests drive that timing explicitly with deferred `listen()` promises.
 */
import { renderHook, act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// `vi.hoisted` so the mock factory (hoisted above imports) can share state.
const h = vi.hoisted(() => ({
  unlistens: [] as Array<ReturnType<typeof vi.fn>>,
  resolvers: [] as Array<() => void>,
}));

vi.mock('@tauri-apps/api/event', () => ({
  // Each listen() hands back a fresh unlisten mock, but only once the test
  // chooses to resolve it — letting us interleave resolution with unmount.
  listen: vi.fn(() => {
    const unlisten = vi.fn();
    h.unlistens.push(unlisten);
    return new Promise((resolve) => {
      h.resolvers.push(() => resolve(unlisten));
    });
  }),
}));

// useElmer doesn't invoke() on mount, but mock it so nothing throws if it ever does.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

import { useElmer } from './useElmer';

// setupListeners() awaits each listen() sequentially, so resolving the pending
// batch only unblocks the NEXT listen() call. Loop, draining + flushing
// microtasks, until all three have registered (and any post-cleanup teardown
// has run).
const resolveAllListens = async () => {
  await act(async () => {
    for (let i = 0; i < 20; i++) {
      const batch = h.resolvers.splice(0, h.resolvers.length);
      for (const r of batch) r();
      await Promise.resolve();
      await Promise.resolve();
    }
  });
};

describe('useElmer listener cleanup (tuxlink-hn5k6)', () => {
  beforeEach(() => {
    h.unlistens.length = 0;
    h.resolvers.length = 0;
  });

  it('tears down listeners that resolve AFTER the effect is cleaned up (no leak)', async () => {
    const { unmount } = renderHook(() => useElmer());

    // The three listen() promises are still pending. Tear the effect down
    // first (StrictMode/HMR/fast-close), THEN let listen() resolve.
    unmount();
    await resolveAllListens();

    // All three listeners registered post-cleanup must be unlistened exactly
    // once — proving none leaked to double-handle future events. (Pre-fix, the
    // cleanup ran on an empty array and these were never called.)
    expect(h.unlistens).toHaveLength(3);
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });

  it('normal lifecycle: listeners register while mounted, then tear down on unmount', async () => {
    const { unmount } = renderHook(() => useElmer());

    // Resolve while still mounted → listeners register and are NOT torn down yet.
    await resolveAllListens();
    expect(h.unlistens).toHaveLength(3);
    for (const u of h.unlistens) expect(u).not.toHaveBeenCalled();

    // Unmount → cleanup tears each down exactly once.
    unmount();
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });
});
