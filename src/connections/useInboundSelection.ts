// src/connections/useInboundSelection.ts
//
// React hook that subscribes to the Tauri `b2f-event` channel for the
// `inbound_proposals_offered` variant (tuxlink-bsiy, WLE "Review Pending
// Messages" parity). On a proposal event it surfaces an active prompt the
// AppShell renders as the inline InboundSelectionPanel; the operator's
// selection is resolved back to the backend via `cms_resolve_inbound_selection`.
//
// AttemptId stale-filter: connects are correlated by a monotonically
// incrementing Rust counter (AttemptId). A proposal event for a superseded
// (lower) attempt is dropped — the backend then times out on its own
// recv_timeout (45s) and proceeds with accept-all for that stale attempt.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AttemptId, B2fEvent, InboundSelection, PendingProposalDto } from './sessionTypes';

/// The active pending-message prompt. Null when no proposal is awaiting a
/// decision. `attemptId` + `requestId` are the correlation keys echoed back to
/// the backend on resolve.
export interface InboundPrompt {
  attemptId: number;
  requestId: number;
  proposals: PendingProposalDto[];
}

export interface UseInboundSelection {
  prompt: InboundPrompt | null;
  /// Resolve the active prompt: send the operator's selection to the backend
  /// then clear the prompt. No-op if there is no active prompt.
  submit: (selection: InboundSelection) => Promise<void>;
  /// Clear the active prompt locally (ESC / panel close). The backend's own
  /// recv_timeout is the authoritative fallback — no resolve is sent here.
  close: () => void;
}

export function useInboundSelection(): UseInboundSelection {
  const [prompt, setPrompt] = useState<InboundPrompt | null>(null);
  // Highest attempt_id seen across the session, for the stale-event filter.
  // A ref (not state) so it doesn't enter the effect's dependency array and
  // is readable synchronously inside the listener callback.
  const latestSeenRef = useRef<AttemptId>(-1);

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;

    listen<B2fEvent>('b2f-event', (event) => {
      if (!mounted) return;
      const payload = event.payload;
      if (payload.kind !== 'inbound_proposals_offered') return;

      // Stale-event filter: drop proposals for a superseded (lower) attempt.
      // The backend will time out (recv_timeout) and proceed with accept-all
      // for that stale attempt, so dropping it here is safe.
      if (payload.attempt_id < latestSeenRef.current) {
        console.warn(
          '[inbound-selection] dropping stale prompt for superseded connect',
          { attempt_id: payload.attempt_id, latest_seen: latestSeenRef.current },
        );
        return;
      }
      latestSeenRef.current = payload.attempt_id;
      setPrompt({
        attemptId: payload.attempt_id,
        requestId: payload.request_id,
        proposals: payload.proposals,
      });
    })
      .then((un) => {
        if (!mounted) {
          un();
          return;
        }
        unlisten = un;
      })
      .catch(() => {
        // listen() unavailable (test env without Tauri — mocked separately).
      });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  const submit = async (selection: InboundSelection): Promise<void> => {
    // Snapshot the active prompt; bail if none (e.g. a double-submit race).
    const active = prompt;
    if (!active) return;
    try {
      // Top-level command args are camelCase (Tauri auto-converts to snake_case
      // for the Rust command params); the `selection` object's INNER fields stay
      // snake_case because it is a nested serde struct, not a command arg.
      await invoke('cms_resolve_inbound_selection', {
        attemptId: active.attemptId,
        requestId: active.requestId,
        selection,
      });
    } catch {
      // Backend absent/offline (tests) — the prompt is still cleared below so
      // the panel dismisses; the backend's recv_timeout covers the live case.
    } finally {
      setPrompt(null);
    }
  };

  const close = (): void => {
    setPrompt(null);
  };

  return { prompt, submit, close };
}
