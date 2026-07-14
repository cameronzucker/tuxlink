/**
 * Tests for useRoutines.ts — the routines library hook (plan-5 Task 6).
 *
 * `@tauri-apps/api/core` is mocked at module scope, gated on the command name
 * (feedback_vitest_invoke_mock_cleanup_call — vitest's stray no-arg teardown
 * call must be inert). `@tauri-apps/api/event`'s `listen` is mocked to
 * capture the registered handler for manual dispatch and to resolve a
 * `vi.fn()` unlisten so cleanup is assertable, mirroring
 * `useFt8Listener.test.ts`'s proven pattern.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

let routinesEventHandler: ((e: { payload: unknown }) => void) | null = null;
const unlistenFn = vi.fn();
const { mockListen } = vi.hoisted(() => ({ mockListen: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mockListen }));

import { useRoutines } from './useRoutines';
import type { RoutinesEvent } from './routinesEvents';

function callsFor(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

function emit(payload: RoutinesEvent) {
  act(() => {
    routinesEventHandler?.({ payload });
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockListen.mockReset();
  unlistenFn.mockReset();
  routinesEventHandler = null;

  mockListen.mockImplementation((event: string, handler: (e: { payload: unknown }) => void) => {
    if (event === 'routines:event') routinesEventHandler = handler;
    return Promise.resolve(unlistenFn);
  });

  // Teardown pitfall: invoke mocks get called with NO args at cleanup — the
  // `cmd === undefined` branch must be inert, never throw.
  mockInvoke.mockImplementation((cmd?: string) => {
    if (cmd === undefined) return Promise.resolve();
    switch (cmd) {
      case 'routines_list':
        return Promise.resolve([
          { routine: 'morning-ics', transmitMode: 'attended', enabled: true, triggers: [] },
        ]);
      case 'routines_missed_fires':
        return Promise.resolve([]);
      case 'routines_next_fires':
        return Promise.resolve([{ routine: 'morning-ics', at: 1_752_400_000 }]);
      case 'routines_fleet_check':
        return Promise.resolve([]);
      case 'routines_validate':
        return Promise.resolve([]);
      default:
        return Promise.resolve([]);
    }
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useRoutines — initial load', () => {
  it('invokes list + missedFires + nextFires + fleetCheck + per-routine validate', async () => {
    const { result } = renderHook(() => useRoutines());
    await act(async () => {});

    expect(callsFor('routines_list').length).toBeGreaterThanOrEqual(1);
    expect(callsFor('routines_missed_fires').length).toBeGreaterThanOrEqual(1);
    expect(callsFor('routines_next_fires').length).toBeGreaterThanOrEqual(1);
    expect(callsFor('routines_fleet_check').length).toBeGreaterThanOrEqual(1);
    const validateCalls = callsFor('routines_validate');
    expect(validateCalls.length).toBeGreaterThanOrEqual(1);
    expect(validateCalls[0]?.[1]).toEqual({ name: 'morning-ics' });

    expect(result.current.summaries).toEqual([
      { routine: 'morning-ics', transmitMode: 'attended', enabled: true, triggers: [] },
    ]);
    expect(result.current.nextFires).toEqual({ 'morning-ics': 1_752_400_000 });
    expect(result.current.scheduleStatus).toEqual([]);
    expect(result.current.fleetFindings).toEqual([]);
    expect(result.current.findingsByRoutine).toEqual({ 'morning-ics': [] });
  });

  it('registers the listener before the caller can observe the first commit', async () => {
    renderHook(() => useRoutines());
    await act(async () => {});
    expect(mockListen).toHaveBeenCalledWith('routines:event', expect.any(Function));
  });
});

describe('useRoutines — re-reads on library/schedule events', () => {
  it('re-invokes routines_list after a debounced libraryChanged event', async () => {
    vi.useFakeTimers();
    renderHook(() => useRoutines());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    const initialListCalls = callsFor('routines_list').length;
    expect(initialListCalls).toBeGreaterThanOrEqual(1);

    emit({ kind: 'libraryChanged', entity: 'routine', name: 'morning-ics' });

    // Not yet — inside the debounce window.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    expect(callsFor('routines_list').length).toBe(initialListCalls);

    // Past the 150ms debounce — the refresh fires.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60);
    });
    expect(callsFor('routines_list').length).toBeGreaterThan(initialListCalls);
  });

  it('coalesces a burst of trigger events into exactly one extra refresh', async () => {
    vi.useFakeTimers();
    renderHook(() => useRoutines());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    const initialListCalls = callsFor('routines_list').length;

    emit({ kind: 'scheduledFire', routine: 'morning-ics', runId: 'run-1', at: 1_752_400_000 });
    emit({ kind: 'runFinished', runId: 'run-1', state: 'completed' });
    emit({ kind: 'missedFires', routine: 'morning-ics', missed: 1, policy: 'skip', ran: false });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    expect(callsFor('routines_list').length).toBe(initialListCalls + 1);
  });

  it('does not refresh on run-progress events this hook does not own', async () => {
    vi.useFakeTimers();
    renderHook(() => useRoutines());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    const initialListCalls = callsFor('routines_list').length;

    emit({ kind: 'runStarted', runId: 'run-1', routine: 'morning-ics', dryRun: false });
    emit({ kind: 'stateChanged', runId: 'run-1', state: 'running' });
    emit({ kind: 'stepCompleted', runId: 'run-1', stepId: 's1', ok: true });
    emit({ kind: 'awaitingConsent', runId: 'run-1', stepId: 's2' });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    expect(callsFor('routines_list').length).toBe(initialListCalls);
  });
});

describe('useRoutines — unmount safety', () => {
  it('calls the unlisten function on unmount', async () => {
    const { unmount } = renderHook(() => useRoutines());
    await act(async () => {});
    unmount();
    expect(unlistenFn).toHaveBeenCalledTimes(1);
  });

  it('does not throw or update state after unmount when an event fires late', async () => {
    vi.useFakeTimers();
    const { unmount } = renderHook(() => useRoutines());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    unmount();
    expect(() => {
      emit({ kind: 'libraryChanged', entity: 'routine', name: 'morning-ics' });
    }).not.toThrow();
  });
});
