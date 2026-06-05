// src/connections/useAuthDiagnostic.ts
//
// React hook that subscribes to the Tauri `b2f-event` channel, tracks
// auth-failure classification state, and exposes the public API the
// `AuthDiagnosticBanner` component (Task 21) will consume.
//
// Spec references: §4.3, §4.5, §8.2, R1 #12, R2 #8, R3 #5, R4 #15.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AttemptId,
  B2fEvent,
  FailureMode,
  TransportFailureKind,
} from './sessionTypes';

export interface AuthDiagnosticState {
  mode: FailureMode | null;
  attemptId: AttemptId | null;
  retryCount: number;
  rawWireResponse: string | null;
  transportKind: TransportFailureKind | null;
  postAuthExchangeStarted: boolean;
  testingInFlight: boolean;
  testRateLimit: {
    disabledUntil: number | null;     // epoch ms; null if not currently disabled
    circuitBroken: boolean;            // true if 3-in-60s tripped; auto-clears after 2 min
    recentTestTimestamps: number[];    // for the 3-in-60s window calculation
  };
}

const INITIAL_STATE: AuthDiagnosticState = {
  mode: null,
  attemptId: null,
  retryCount: 0,
  rawWireResponse: null,
  transportKind: null,
  postAuthExchangeStarted: false,
  testingInFlight: false,
  testRateLimit: {
    disabledUntil: null,
    circuitBroken: false,
    recentTestTimestamps: [],
  },
};

const RETRY_IDLE_RESET_MS = 5 * 60 * 1000;    // 5 minutes
const RATE_LIMIT_DEBOUNCE_MS = 10 * 1000;     // 10 seconds
const CIRCUIT_BREAK_WINDOW_MS = 60 * 1000;   // 60 seconds
const CIRCUIT_BREAK_THRESHOLD = 3;
const CIRCUIT_BREAK_DURATION_MS = 2 * 60 * 1000; // 2 minutes

export function useAuthDiagnostic(): {
  state: AuthDiagnosticState;
  dismiss: () => Promise<void>;
  testCredentials: () => Promise<void>;
} {
  const [state, setState] = useState<AuthDiagnosticState>(INITIAL_STATE);
  // Tracks the wall-clock time of the last AuthClassified event for the
  // 5-minute idle-reset heuristic (R4 #15). Using a ref avoids including
  // it in the effect dependency array.
  const lastAuthAtRef = useRef<number>(Date.now());

  useEffect(() => {
    let mounted = true;
    let unlistenB2f: (() => void) | undefined;
    let unlistenClear: (() => void) | undefined;

    listen<B2fEvent>('b2f-event', (event) => {
      if (!mounted) return;
      setState((prev) => applyB2fEvent(prev, event.payload, lastAuthAtRef));
    }).then((un) => {
      if (!mounted) {
        un();
        return;
      }
      unlistenB2f = un;
    }).catch(() => {
      // listen() unavailable (test env without Tauri — mocked separately).
    });

    listen<void>('auth-diagnostic-clear', () => {
      if (!mounted) return;
      setState(INITIAL_STATE);
    }).then((un) => {
      if (!mounted) {
        un();
        return;
      }
      unlistenClear = un;
    }).catch(() => {
      // listen() unavailable (test env without Tauri — mocked separately).
    });

    return () => {
      mounted = false;
      unlistenB2f?.();
      unlistenClear?.();
    };
  }, []);

  // Auto-clear rate-limit + circuit-break when disabledUntil elapses.
  // Without this, circuitBroken=true is permanent — the button stays
  // disabled forever (Codex MAJOR #5).
  useEffect(() => {
    const target = state.testRateLimit.disabledUntil;
    if (target === null || target <= Date.now()) return;
    const id = setTimeout(() => {
      setState((prev) => {
        // Re-check at fire time — may have already updated via another path.
        if (prev.testRateLimit.disabledUntil !== target) return prev;
        return {
          ...prev,
          testRateLimit: {
            disabledUntil: null,
            circuitBroken: false,
            recentTestTimestamps: prev.testRateLimit.recentTestTimestamps,
          },
        };
      });
    }, target - Date.now());
    return () => clearTimeout(id);
  }, [state.testRateLimit.disabledUntil]);

  const dismiss = async () => {
    try {
      await invoke('auth_diagnostic_clear');
    } catch {
      // Backend absent or offline — local clear still applies.
    } finally {
      setState(INITIAL_STATE);
    }
  };

  const testCredentials = async () => {
    setState((prev) => ({ ...prev, testingInFlight: true }));
    try {
      await invoke('cms_connect_test');
    } catch {
      // The failure surfaces as a b2f-event → AuthClassified; no
      // additional handling needed here. testingInFlight is cleared below.
    } finally {
      // Apply rate-limit accounting: 10s post-test debounce and
      // 3-in-60s circuit-break per spec §4.3 iii + R2 #8.
      setState((prev) => {
        const now = Date.now();
        const recentInWindow = prev.testRateLimit.recentTestTimestamps.filter(
          (t) => now - t < CIRCUIT_BREAK_WINDOW_MS,
        );
        const updated = [...recentInWindow, now];
        const isCircuitBreak = updated.length >= CIRCUIT_BREAK_THRESHOLD;
        // Circuit-break: disable for CIRCUIT_BREAK_DURATION_MS (2 min); normal
        // debounce: disable for RATE_LIMIT_DEBOUNCE_MS (10 s).
        const disabledUntil = isCircuitBreak
          ? now + CIRCUIT_BREAK_DURATION_MS
          : now + RATE_LIMIT_DEBOUNCE_MS;
        return {
          ...prev,
          testingInFlight: false,
          testRateLimit: {
            disabledUntil,
            circuitBroken: isCircuitBreak,
            recentTestTimestamps: updated,
          },
        };
      });
    }
  };

  return { state, dismiss, testCredentials };
}

/**
 * Pure reducer: apply one B2fEvent to the current AuthDiagnosticState.
 * Exported for unit tests that exercise the state machine without
 * hooking into the React lifecycle.
 *
 * AttemptId ordering: numeric comparison — events for superseded
 * (lower) attempt ids are silently dropped. AttemptIds come from a
 * monotonically-incrementing Rust counter so this is safe (R1 #12,
 * R3 #5).
 */
export function applyB2fEvent(
  prev: AuthDiagnosticState,
  event: B2fEvent,
  lastAuthAtRef: React.MutableRefObject<number>,
): AuthDiagnosticState {
  switch (event.kind) {
    case 'auth_classified': {
      // Stale-event filter: ignore events for superseded AttemptIds.
      if (prev.attemptId !== null && event.attempt_id < prev.attemptId) {
        return prev;
      }
      const now = Date.now();
      // Idle reset: if >5 minutes since last auth event, start the
      // counter fresh regardless of mode (R4 #15).
      const idleReset = now - lastAuthAtRef.current > RETRY_IDLE_RESET_MS;
      lastAuthAtRef.current = now;
      // Counter increments when: same mode AND not idle-reset.
      // Resets to 1 on mode change or idle-reset.
      const retryCount =
        idleReset || prev.mode !== event.mode ? 1 : prev.retryCount + 1;
      // Clear postAuthExchangeStarted on a fresh attempt (new AttemptId).
      const postAuthExchangeStarted =
        event.attempt_id !== prev.attemptId
          ? false
          : prev.postAuthExchangeStarted;
      return {
        ...prev,
        mode: event.mode,
        attemptId: event.attempt_id,
        retryCount,
        rawWireResponse: event.raw,
        postAuthExchangeStarted,
        // Clear testingInFlight if this classified event matches the
        // AttemptId of an in-flight test (the test completed).
        testingInFlight: prev.testingInFlight
          ? event.attempt_id !== prev.attemptId
          : false,
      };
    }

    case 'connection_closed': {
      // Stale-event filter.
      if (prev.attemptId !== null && event.attempt_id < prev.attemptId) {
        return prev;
      }
      return {
        ...prev,
        transportKind: event.transport_kind,
      };
    }

    case 'post_auth_exchange_started': {
      // Stale-event filter.
      if (prev.attemptId !== null && event.attempt_id < prev.attemptId) {
        return prev;
      }
      return {
        ...prev,
        postAuthExchangeStarted: true,
      };
    }

    default:
      // All other B2fEvent variants (tcp_connected, tls_*, remote_sid_received,
      // etc.) are not relevant to auth-failure classification state.
      return prev;
  }
}
