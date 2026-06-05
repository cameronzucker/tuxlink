// src/connections/useAuthDiagnostic.test.ts
//
// Tests for useAuthDiagnostic hook. The Tauri @tauri-apps/api modules are
// mocked at the module level so tests run in jsdom without a real Tauri
// context. The listen mock captures event handlers for manual dispatch.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

// Capture the event handlers registered by the hook.
let b2fHandler: ((e: { payload: unknown }) => void) | null = null;
let clearHandler: ((e: { payload: unknown }) => void) | null = null;

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, handler: (e: { payload: unknown }) => void) => {
    if (event === 'b2f-event') {
      b2fHandler = handler;
    } else if (event === 'auth-diagnostic-clear') {
      clearHandler = handler;
    }
    return Promise.resolve(() => {
      if (event === 'b2f-event') b2fHandler = null;
      else if (event === 'auth-diagnostic-clear') clearHandler = null;
    });
  }),
}));

import { useAuthDiagnostic } from './useAuthDiagnostic';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function emitB2f(payload: unknown) {
  b2fHandler!({ payload });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('useAuthDiagnostic', () => {
  beforeEach(() => {
    b2fHandler = null;
    clearHandler = null;
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  // --- AttemptId correlation and basic event tracking ---

  it('starts with null mode and zero retryCount', () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    expect(result.current.state.mode).toBeNull();
    expect(result.current.state.attemptId).toBeNull();
    expect(result.current.state.retryCount).toBe(0);
  });

  it('renders the mode from an AuthClassified event', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: '*** Secure login failed',
        attempt_id: 1,
      });
    });

    expect(result.current.state.mode).toBe('password_rejected');
    expect(result.current.state.attemptId).toBe(1);
    expect(result.current.state.rawWireResponse).toContain('Secure login failed');
  });

  it('filters stale AttemptId events (older attempt_id ignored)', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 5,
      });
    });

    // Stale event — lower attempt_id; should be ignored.
    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'callsign_rejected',
        raw: null,
        attempt_id: 2,
      });
    });

    expect(result.current.state.mode).toBe('password_rejected');
    expect(result.current.state.attemptId).toBe(5);
  });

  it('accepts an equal attempt_id (same-attempt re-classification)', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 3,
      });
    });
    // Same attempt_id with a different mode (shouldn't happen in practice,
    // but the filter should accept it — equal id is not "stale").
    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'client_rejected',
        raw: 'server rejected',
        attempt_id: 3,
      });
    });

    expect(result.current.state.mode).toBe('client_rejected');
  });

  // --- Retry counter ---

  it('sets retryCount=1 on the first AuthClassified event', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 1,
      });
    });

    expect(result.current.state.retryCount).toBe(1);
  });

  it('increments retryCount on consecutive same-mode failures', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 1,
      });
    });
    expect(result.current.state.retryCount).toBe(1);

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 2,
      });
    });
    expect(result.current.state.retryCount).toBe(2);

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 3,
      });
    });
    expect(result.current.state.retryCount).toBe(3);
  });

  it('resets retryCount to 1 on a mode change', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 1,
      });
    });
    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 2,
      });
    });
    expect(result.current.state.retryCount).toBe(2);

    // Mode change → counter resets to 1.
    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'callsign_rejected',
        raw: null,
        attempt_id: 3,
      });
    });
    expect(result.current.state.retryCount).toBe(1);
  });

  // --- connection_closed + transportKind ---

  it('captures transportKind from ConnectionClosed', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'network_unreachable',
        raw: null,
        attempt_id: 1,
      });
    });
    act(() => {
      emitB2f({
        kind: 'connection_closed',
        phase: 'pre_handshake',
        transport_kind: 'tcp_refused',
        attempt_id: 1,
      });
    });

    expect(result.current.state.transportKind).toBe('tcp_refused');
  });

  it('filters stale ConnectionClosed events', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'network_unreachable',
        raw: null,
        attempt_id: 5,
      });
    });
    act(() => {
      emitB2f({
        kind: 'connection_closed',
        phase: 'pre_handshake',
        transport_kind: 'dns',
        attempt_id: 2,
      });
    });

    expect(result.current.state.transportKind).toBeNull();
  });

  // --- post_auth_exchange_started ---

  it('sets postAuthExchangeStarted on PostAuthExchangeStarted event', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'session_dropped_after_auth',
        raw: null,
        attempt_id: 1,
      });
    });
    act(() => {
      emitB2f({
        kind: 'post_auth_exchange_started',
        attempt_id: 1,
      });
    });

    expect(result.current.state.postAuthExchangeStarted).toBe(true);
  });

  // --- dismiss ---

  it('dismiss() clears state and calls auth_diagnostic_clear', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: 'bad password',
        attempt_id: 1,
      });
    });
    expect(result.current.state.mode).toBe('password_rejected');

    await act(async () => {
      await result.current.dismiss();
    });

    expect(invokeMock).toHaveBeenCalledWith('auth_diagnostic_clear', undefined);
    expect(result.current.state.mode).toBeNull();
    expect(result.current.state.attemptId).toBeNull();
    expect(result.current.state.retryCount).toBe(0);
  });

  it('dismiss() clears state even if invoke rejects', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    invokeMock.mockRejectedValue(new Error('backend offline'));

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 1,
      });
    });

    await act(async () => {
      await result.current.dismiss();
    });

    // State should be cleared despite the invoke failure.
    expect(result.current.state.mode).toBeNull();
  });

  // --- auth-diagnostic-clear event ---

  it('auth-diagnostic-clear event resets state', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());
    await waitFor(() => expect(clearHandler).not.toBeNull());

    act(() => {
      emitB2f({
        kind: 'auth_classified',
        mode: 'password_rejected',
        raw: null,
        attempt_id: 1,
      });
    });
    expect(result.current.state.mode).toBe('password_rejected');

    act(() => {
      clearHandler!({ payload: undefined });
    });

    expect(result.current.state.mode).toBeNull();
    expect(result.current.state.retryCount).toBe(0);
  });

  // --- testCredentials rate-limit ---

  it('testCredentials calls cms_connect_test and applies rate-limit debounce', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    await act(async () => {
      await result.current.testCredentials();
    });

    expect(invokeMock).toHaveBeenCalledWith('cms_connect_test', undefined);
    expect(result.current.state.testRateLimit.disabledUntil).not.toBeNull();
    expect(result.current.state.testRateLimit.disabledUntil).toBeGreaterThan(Date.now() - 1000);
    expect(result.current.state.testingInFlight).toBe(false);
  });

  it('testCredentials sets testingInFlight=true during the call', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    // Make invoke pend so we can check the in-flight state.
    let resolveTest!: () => void;
    invokeMock.mockImplementation(async () => {
      await new Promise<void>((resolve) => { resolveTest = resolve; });
    });

    let testPromise!: Promise<void>;
    act(() => {
      testPromise = result.current.testCredentials();
    });

    // In-flight — testingInFlight should be true immediately after the
    // synchronous setState call at the start of testCredentials.
    await waitFor(() => {
      expect(result.current.state.testingInFlight).toBe(true);
    });

    // Let the invoke resolve.
    resolveTest();
    await act(async () => { await testPromise; });
    expect(result.current.state.testingInFlight).toBe(false);
  });

  it('3-in-60s circuit-break trips after 3 tests within the window', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    expect(result.current.state.testRateLimit.circuitBroken).toBe(false);

    for (let i = 0; i < 3; i++) {
      await act(async () => {
        await result.current.testCredentials();
      });
    }

    expect(result.current.state.testRateLimit.circuitBroken).toBe(true);
    expect(result.current.state.testRateLimit.recentTestTimestamps).toHaveLength(3);
  });

  it('recentTestTimestamps accumulates timestamps across calls', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    await act(async () => { await result.current.testCredentials(); });
    await act(async () => { await result.current.testCredentials(); });

    expect(result.current.state.testRateLimit.recentTestTimestamps).toHaveLength(2);
  });

  // --- circuit breaker auto-clear (Codex MAJOR #5) ---

  it('circuit breaker auto-clears after 2 minutes', async () => {
    // Pre-register the b2f handler with real timers before engaging fakes.
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    vi.useFakeTimers();
    try {
      // Trip the circuit breaker with 3 tests in quick succession.
      for (let i = 0; i < 3; i++) {
        await act(async () => {
          await result.current.testCredentials();
        });
      }
      expect(result.current.state.testRateLimit.circuitBroken).toBe(true);

      // Advance past the 2-minute circuit-break duration.
      await act(async () => {
        vi.advanceTimersByTime(2 * 60 * 1000 + 1000);
      });

      expect(result.current.state.testRateLimit.circuitBroken).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  // --- Unrelated B2fEvent variants are ignored ---

  it('ignores B2fEvent variants unrelated to auth classification', async () => {
    const { result } = renderHook(() => useAuthDiagnostic());
    await waitFor(() => expect(b2fHandler).not.toBeNull());

    act(() => {
      emitB2f({ kind: 'tcp_connected', host: 'cms.winlink.org', port: 8772, attempt_id: 1 });
    });
    act(() => {
      emitB2f({ kind: 'tls_handshake_started', attempt_id: 1 });
    });
    act(() => {
      emitB2f({ kind: 'remote_sid_received', sid: '[WL2K-2-1.5.42.0-B2FIHMS.FBBX]', attempt_id: 1 });
    });

    // State should be unchanged from initial.
    expect(result.current.state.mode).toBeNull();
    expect(result.current.state.attemptId).toBeNull();
  });
});
