// src/aprs/useAprsChat.ts
//
// React hook backing the APRS tactical-chat panel. Subscribes to the three
// backend event channels (Task 10) and exposes a `send` action plus the
// per-callsign thread map.
//
// RF-honesty (round-1 hardened): `send` mints NO local msgid. It awaits
// `aprs_send`, and only on success — when the backend has accepted the message
// into its outbound queue and returned the minted msgid — inserts an optimistic
// `sent` bubble. On reject (capacity / not-listening), the promise rejects, no
// bubble is inserted, and the error propagates so the panel can toast it. This
// prevents a stuck "sent" bubble for a message that was never queued.

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AprsConfigDto,
  ChatMessage,
  InboundMsgDto,
  StateChangeDto,
  Thread,
} from './aprsTypes';

export interface UseAprsChat {
  /// Conversations keyed by remote callsign.
  threads: Record<string, Thread>;
  /// Whether the APRS listener is currently active (mirrors the backend).
  listening: boolean;
  /// Send `text` to `call`. Awaits the backend-minted msgid and inserts an
  /// optimistic outgoing bubble ONLY on success. Rejects (without inserting a
  /// bubble) when the backend refuses (capacity / not-listening).
  send: (call: string, text: string) => Promise<string>;
  /// Refresh the cached APRS station configuration from the backend.
  refreshConfig: () => Promise<void>;
}

let localIdSeq = 0;
function nextLocalId(): string {
  localIdSeq += 1;
  return `local-${localIdSeq}`;
}

/// Append `msg` to `call`'s thread, creating the thread if absent. Returns a
/// new threads object (immutable update) so React re-renders.
function appendMessage(
  threads: Record<string, Thread>,
  call: string,
  msg: ChatMessage,
): Record<string, Thread> {
  const existing = threads[call];
  const thread: Thread = existing
    ? { callsign: call, messages: [...existing.messages, msg] }
    : { callsign: call, messages: [msg] };
  return { ...threads, [call]: thread };
}

/// Set `.state` on the message whose `.msgid === msgid`, searching every
/// thread. Stamps `ackedAt` when transitioning to `acked`. Returns a new
/// threads object only for the thread that changed.
function applyState(
  threads: Record<string, Thread>,
  msgid: string,
  state: StateChangeDto['state'],
  at: number,
): Record<string, Thread> {
  const next: Record<string, Thread> = {};
  let changed = false;
  for (const [call, thread] of Object.entries(threads)) {
    const idx = thread.messages.findIndex((m) => m.msgid === msgid);
    if (idx === -1) {
      next[call] = thread;
      continue;
    }
    const messages = thread.messages.slice();
    messages[idx] = {
      ...messages[idx],
      state,
      ...(state === 'acked' ? { ackedAt: at } : {}),
    };
    next[call] = { callsign: thread.callsign, messages };
    changed = true;
  }
  return changed ? next : threads;
}

export function useAprsChat(): UseAprsChat {
  const [threads, setThreads] = useState<Record<string, Thread>>({});
  const [listening, setListening] = useState<boolean>(false);

  useEffect(() => {
    let mounted = true;
    const unlistens: Array<() => void> = [];

    const subscribe = <T,>(
      channel: string,
      handler: (payload: T) => void,
    ): void => {
      listen<T>(channel, (event) => {
        if (!mounted) return;
        handler(event.payload);
      })
        .then((un) => {
          if (!mounted) {
            un();
            return;
          }
          unlistens.push(un);
        })
        .catch(() => {
          // listen() unavailable (jsdom without Tauri — mocked in tests).
        });
    };

    subscribe<InboundMsgDto>('aprs-message:new', (payload) => {
      const msg: ChatMessage = {
        id: payload.msgid ?? nextLocalId(),
        direction: 'in',
        text: payload.text,
        msgid: payload.msgid,
        at: Date.now(),
      };
      setThreads((prev) => appendMessage(prev, payload.sender, msg));
    });

    subscribe<StateChangeDto>('aprs-message:state', (payload) => {
      setThreads((prev) => applyState(prev, payload.msgid, payload.state, Date.now()));
    });

    subscribe<boolean>('aprs-listening:change', (payload) => {
      setListening(payload);
    });

    return () => {
      mounted = false;
      for (const un of unlistens) un();
    };
  }, []);

  const send = useCallback(async (call: string, text: string): Promise<string> => {
    // Mint no local msgid. Await the backend; on reject, let it propagate
    // WITHOUT inserting a bubble (RF-honesty).
    const id = await invoke<string>('aprs_send', { call, text });
    const msg: ChatMessage = {
      id,
      direction: 'out',
      text,
      msgid: id,
      state: 'sent',
      at: Date.now(),
    };
    setThreads((prev) => appendMessage(prev, call, msg));
    return id;
  }, []);

  const refreshConfig = useCallback(async (): Promise<void> => {
    try {
      await invoke<AprsConfigDto>('aprs_config_get');
    } catch {
      // Backend absent (tests) — config refresh is best-effort.
    }
  }, []);

  return { threads, listening, send, refreshConfig };
}
