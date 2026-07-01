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
  // Per-channel handler capture for the streaming tests (phase 2b). The
  // cleanup-timing tests above don't dispatch events, so they ignore this.
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  // Each listen() hands back a fresh unlisten mock, but only once the test
  // chooses to resolve it — letting us interleave resolution with unmount.
  // The channel + handler are captured so streaming tests can dispatch events.
  listen: vi.fn((channel: string, handler: (event: { payload: unknown }) => void) => {
    const unlisten = vi.fn();
    h.unlistens.push(unlisten);
    h.handlers.set(channel, handler);
    return new Promise((resolve) => {
      h.resolvers.push(() => resolve(unlisten));
    });
  }),
}));

// useElmer doesn't invoke() on mount, but mock it so nothing throws if it ever does.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));

import { useElmer, keyStatusForOrigins } from './useElmer';
import { EV_CONTEXT, EV_DELTA, EV_OUTCOME, EV_TURN } from './elmerEvents';
import type { ElmerContextPayload, ElmerDeltaPayload, ElmerOutcomePayload, ElmerTurnPayload } from './elmerEvents';
import { invoke } from '@tauri-apps/api/core';

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

    // The listen() promises are still pending. Tear the effect down first
    // (StrictMode/HMR/fast-close), THEN let listen() resolve.
    unmount();
    await resolveAllListens();

    // All five listeners (EV_DELTA + EV_TURN + EV_CHIP + EV_OUTCOME + EV_CONTEXT)
    // registered post-cleanup must be unlistened exactly once — proving none
    // leaked to double-handle future events. (Pre-fix, the cleanup ran on an
    // empty array and these were never called.)
    expect(h.unlistens).toHaveLength(5);
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });

  it('normal lifecycle: listeners register while mounted, then tear down on unmount', async () => {
    const { unmount } = renderHook(() => useElmer());

    // Resolve while still mounted → listeners register and are NOT torn down yet.
    await resolveAllListens();
    expect(h.unlistens).toHaveLength(5);
    for (const u of h.unlistens) expect(u).not.toHaveBeenCalled();

    // Unmount → cleanup tears each down exactly once.
    unmount();
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// Phase 2b — streaming buffers (EV_DELTA) + finalize-swap (EV_TURN)
// ---------------------------------------------------------------------------

describe('useElmer streaming (phase 2b)', () => {
  beforeEach(() => {
    h.unlistens.length = 0;
    h.resolvers.length = 0;
    h.handlers.clear();
  });

  // Dispatch an event into the captured handler for a channel.
  const dispatch = async <T,>(channel: string, payload: T) => {
    await act(async () => {
      const handler = h.handlers.get(channel);
      if (handler) handler({ payload });
      await Promise.resolve();
    });
  };

  it('accumulates reasoning then assistant deltas into the live buffers, then EV_TURN commits with reasoning and clears both', async () => {
    const { result } = renderHook(() => useElmer());
    // Resolve the deferred listen() promises so the handlers register.
    await resolveAllListens();

    // Reasoning deltas arrive first (reasoning-then-answer ordering).
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'Let me ' });
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'think.' });
    expect(result.current.streamingReasoning).toBe('Let me think.');
    expect(result.current.streamingAnswer).toBe('');

    // Then the assistant answer streams in.
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'The ' });
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'answer.' });
    expect(result.current.streamingAnswer).toBe('The answer.');
    expect(result.current.streamingReasoning).toBe('Let me think.');

    // EV_TURN finalizes: a committed assistant item appears carrying reasoning,
    // and BOTH transient buffers clear (live bubble → committed item swap).
    await dispatch<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'The answer.' });

    expect(result.current.streamingAnswer).toBe('');
    expect(result.current.streamingReasoning).toBe('');

    const turns = result.current.items.filter((i) => i.kind === 'turn');
    expect(turns).toHaveLength(1);
    const committed = turns[0] as Extract<typeof turns[number], { kind: 'turn' }>;
    expect(committed.role).toBe('assistant');
    expect(committed.text).toBe('The answer.');
    expect(committed.reasoning).toBe('Let me think.');
  });

  it('a turn with NO deltas: EV_TURN appends a normal item with reasoning undefined (no regression)', async () => {
    const { result } = renderHook(() => useElmer());
    await resolveAllListens();

    await dispatch<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'Direct answer.' });

    const turns = result.current.items.filter((i) => i.kind === 'turn');
    expect(turns).toHaveLength(1);
    const committed = turns[0] as Extract<typeof turns[number], { kind: 'turn' }>;
    expect(committed.text).toBe('Direct answer.');
    expect(committed.reasoning).toBeUndefined();
    expect(result.current.streamingAnswer).toBe('');
    expect(result.current.streamingReasoning).toBe('');
  });

  it('a terminal outcome WITHOUT a finalizing EV_TURN (cancel/error mid-stream) clears the live buffers', async () => {
    const { result } = renderHook(() => useElmer());
    await resolveAllListens();

    // A streamed turn is in flight: reasoning + answer have accumulated.
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'Half a thought' });
    await dispatch<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Half an ans' });
    expect(result.current.streamingAnswer).toBe('Half an ans');
    expect(result.current.streamingReasoning).toBe('Half a thought');

    // The run is cancelled mid-stream → EV_OUTCOME fires with NO preceding EV_TURN.
    await dispatch<ElmerOutcomePayload>(EV_OUTCOME, { kind: 'outcome', outcomeKind: 'cancelled', detail: '' });

    // The transient live bubble must not linger after the run ended.
    expect(result.current.streamingAnswer).toBe('');
    expect(result.current.streamingReasoning).toBe('');
    // No partial item was committed (the answer never finalized).
    expect(result.current.items.filter((i) => i.kind === 'turn')).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// T8b — keyStatusForOrigins wrapper (Task 4-fe frontend shim)
// ---------------------------------------------------------------------------

describe('keyStatusForOrigins (T8b Task 4-fe wrapper)', () => {
  const mockInvoke = invoke as ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockInvoke.mockClear();
  });

  it('calls elmer_key_status_for_origins with the provided origins and returns the map', async () => {
    const expectedMap = {
      'https://api.openai.com': 'present' as const,
      'https://generativelanguage.googleapis.com': 'absent' as const,
    };
    // mockImplementationOnce so the teardown no-arg call doesn't interfere.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_key_status_for_origins') return expectedMap;
      return undefined;
    });

    const origins = ['https://api.openai.com', 'https://generativelanguage.googleapis.com'];
    const result = await keyStatusForOrigins(origins);

    // The correct command was called.
    expect(mockInvoke).toHaveBeenCalledWith('elmer_key_status_for_origins', { origins });
    // The map is returned verbatim.
    expect(result).toEqual(expectedMap);
  });

  it('passes the origins array through to the invoke call unchanged', async () => {
    const testOrigins = ['https://api.groq.com', 'https://api.anthropic.com'];
    mockInvoke.mockImplementationOnce(async (_cmd?: string) => ({}));

    await keyStatusForOrigins(testOrigins);

    // Teardown no-arg call from vitest: the mock was called at least once with the origins.
    const calls = mockInvoke.mock.calls;
    const realCall = calls.find((c) => c[0] === 'elmer_key_status_for_origins');
    expect(realCall).toBeTruthy();
    expect(realCall![1]).toEqual({ origins: testOrigins });
  });
});

// ---------------------------------------------------------------------------
// T7 — EV_CONTEXT updates context state; cleanup unsubscribes (tuxlink-65qhn)
// ---------------------------------------------------------------------------

describe('useElmer EV_CONTEXT (T7)', () => {
  beforeEach(() => {
    h.unlistens.length = 0;
    h.resolvers.length = 0;
    h.handlers.clear();
  });

  const dispatch = async <T,>(channel: string, payload: T) => {
    await act(async () => {
      const handler = h.handlers.get(channel);
      if (handler) handler({ payload });
      await Promise.resolve();
    });
  };

  it('context is null before any EV_CONTEXT event', async () => {
    const { result } = renderHook(() => useElmer());
    await resolveAllListens();
    expect(result.current.context).toBeNull();
  });

  it('EV_CONTEXT event sets context with promptTokens + numCtx', async () => {
    const { result } = renderHook(() => useElmer());
    await resolveAllListens();

    await dispatch<ElmerContextPayload>(EV_CONTEXT, {
      kind: 'context',
      promptTokens: 12000,
      evalTokens: 512,
      numCtx: 32768,
    });

    expect(result.current.context).not.toBeNull();
    expect(result.current.context!.promptTokens).toBe(12000);
    expect(result.current.context!.numCtx).toBe(32768);
  });

  it('context is updated on subsequent EV_CONTEXT events (latest wins)', async () => {
    const { result } = renderHook(() => useElmer());
    await resolveAllListens();

    await dispatch<ElmerContextPayload>(EV_CONTEXT, {
      kind: 'context',
      promptTokens: 12000,
      evalTokens: 512,
      numCtx: 32768,
    });
    expect(result.current.context!.promptTokens).toBe(12000);

    await dispatch<ElmerContextPayload>(EV_CONTEXT, {
      kind: 'context',
      promptTokens: 20000,
      evalTokens: 768,
      numCtx: 32768,
    });
    expect(result.current.context!.promptTokens).toBe(20000);
    expect(result.current.context!.numCtx).toBe(32768);
  });

  it('EV_CONTEXT listener is included in the cleanup set (unlistened on unmount)', async () => {
    const { unmount } = renderHook(() => useElmer());
    await resolveAllListens();

    // All 5 listeners registered: EV_DELTA, EV_TURN, EV_CHIP, EV_OUTCOME, EV_CONTEXT.
    expect(h.unlistens).toHaveLength(5);
    for (const u of h.unlistens) expect(u).not.toHaveBeenCalled();

    unmount();

    // All five must be torn down exactly once — including the EV_CONTEXT unlisten.
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });

  it('EV_CONTEXT listener resolving post-cleanup is torn down immediately (no leak)', async () => {
    const { unmount } = renderHook(() => useElmer());

    // Unmount before listen() promises resolve — same pattern as hn5k6 cleanup test.
    unmount();
    await resolveAllListens();

    // All five (including EV_CONTEXT) resolved post-cleanup must be unlistened once.
    expect(h.unlistens).toHaveLength(5);
    for (const u of h.unlistens) expect(u).toHaveBeenCalledTimes(1);
  });
});
