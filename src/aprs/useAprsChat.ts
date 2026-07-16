// src/aprs/useAprsChat.ts
//
// React hook backing the APRS tactical-chat panel. APRS is a single OPEN
// CHANNEL (party line): every message heard on the channel — directed or
// broadcast, to us or to anyone — plus our own sends lands in ONE flat,
// time-ordered feed (`messages`). There is no per-callsign thread grouping.
//
// Subscribes to the three backend event channels and exposes a `send` action,
// the derived `heardStations` list (for the recipient dropdown), and
// `aprs_config` get/set passthroughs for the composer's Path control.
//
// RF-honesty: `send` mints NO local id. It awaits `aprs_send`, and only on
// success — when the backend has accepted the message into its outbound queue
// and returned the tracking id — appends an optimistic `sent` message. On
// reject (capacity / not-listening), the promise rejects, no message is
// appended, and the error propagates so the panel can surface it. This prevents
// a stuck "sent" message for traffic that was never queued.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, emit } from '@tauri-apps/api/event';
import type {
  AprsConfigDto,
  ChannelMessage,
  DeliveryState,
  HeardStation,
  InboundMsgDto,
  SentMsgDto,
  StateChangeDto,
} from './aprsTypes';

/// Cross-window snapshot-handshake events (spec §7, tuxlink-dmwte task 10) —
/// mirrors useAprsPositions' `aprs-positions:*` pair.
const CHAT_SNAPSHOT_REQUEST = 'aprs-chat:request-snapshot';
const CHAT_SNAPSHOT_REPLY = 'aprs-chat:snapshot';
/// Retry cadence + give-up bound for the client's request (spec §7 retry
/// amendment), identical to useAprsPositions'.
const SNAPSHOT_RETRY_MS = 250;
const SNAPSHOT_GIVE_UP_MS = 3000;

export interface UseAprsChatOptions {
  /// `'host'` (the main shell) answers snapshot requests with its current feed;
  /// `'client'` (a pop-out — the popped AprsChatSurface, ChatStrip) requests +
  /// seeds on mount with the spec §7 retry amendment. Omitted ⇒ neither: no
  /// handshake listens or emits at all (existing callers stay untouched). The
  /// own-send echo subscription is UNCONDITIONAL — every instance consumes it
  /// regardless of role, so a feed is reconstructible from events alone.
  snapshotRole?: 'host' | 'client';
}

export interface UseAprsChat {
  /// The open channel: one flat, time-ordered feed of inbound + outbound
  /// messages (oldest first).
  messages: ChannelMessage[];
  /// Stations heard on the channel, most-recently-heard first, deduped by
  /// callsign. Backs the recipient dropdown.
  heardStations: HeardStation[];
  /// Whether the APRS listener is currently active (mirrors the backend).
  listening: boolean;
  /// Send `text` to `recipient`. `null` or `''` ⇒ broadcast (no addressee, no
  /// delivery ack); a callsign ⇒ directed (ack-tracked). Awaits the backend
  /// tracking id and appends an optimistic outgoing message ONLY on success.
  /// Rejects (without appending) when the backend refuses (capacity /
  /// not-listening).
  send: (recipient: string | null, text: string) => Promise<string>;
  /// Read the cached APRS station configuration from the backend.
  getConfig: () => Promise<AprsConfigDto>;
  /// Persist the APRS station configuration (read-modify-write of the full DTO;
  /// the backend command takes the whole `AprsConfigDto` under the `dto` key).
  setConfig: (dto: AprsConfigDto) => Promise<void>;
}

let localIdSeq = 0;
function nextLocalId(): string {
  localIdSeq += 1;
  return `local-${localIdSeq}`;
}

/// Set `.state` on the message whose `.msgid === msgid`. Stamps `ackedAt` when
/// transitioning to `acked`. Returns a new array only when something changed.
function applyState(
  messages: ChannelMessage[],
  msgid: string,
  state: StateChangeDto['state'],
  at: number,
): ChannelMessage[] {
  const idx = messages.findIndex((m) => m.msgid === msgid);
  if (idx === -1) return messages;
  const next = messages.slice();
  next[idx] = {
    ...next[idx],
    state,
    ...(state === 'acked' ? { ackedAt: at } : {}),
  };
  return next;
}

/// Lifecycle rank of a delivery state, so a snapshot merge keeps the
/// more-progressed one and never clobbers a fresher live state with a staler
/// snapshot (spec §7). Inbound (no state) = 0, `sent` = 1, terminal
/// (acked/timedOut/rejected) = 2.
function stateRank(state: DeliveryState | undefined): number {
  if (state === undefined) return 0;
  if (state === 'sent') return 1;
  return 2;
}

/// Merge a host snapshot into the client's feed, deduping on `id` and keeping
/// the more-progressed delivery state per id. Returns the same reference when
/// nothing changed. The merged feed is re-sorted by `at` so a snapshot's older
/// messages interleave correctly with any live events heard before it landed.
function mergeSnapshot(prev: ChannelMessage[], incoming: ChannelMessage[]): ChannelMessage[] {
  if (incoming.length === 0) return prev;
  const byId = new Map(prev.map((m) => [m.id, m]));
  let changed = false;
  for (const s of incoming) {
    const existing = byId.get(s.id);
    if (existing === undefined) {
      byId.set(s.id, s);
      changed = true;
    } else if (stateRank(s.state) > stateRank(existing.state)) {
      byId.set(s.id, s);
      changed = true;
    }
  }
  if (!changed) return prev;
  return [...byId.values()].sort((a, b) => a.at - b.at);
}

export function useAprsChat(opts?: UseAprsChatOptions): UseAprsChat {
  const role = opts?.snapshotRole;
  const [messages, setMessages] = useState<ChannelMessage[]>([]);
  const [listening, setListening] = useState<boolean>(false);
  // Latest feed, read by the host's snapshot responder without making the
  // subscription effect depend on `messages` (mirrors useAprsPositions).
  const messagesRef = useRef(messages);
  messagesRef.current = messages;

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
      const msg: ChannelMessage = {
        id: payload.msgid ?? nextLocalId(),
        direction: 'in',
        from: payload.sender,
        // Blank addressee ⇒ broadcast (`→ all`).
        to: payload.addressee === '' ? null : payload.addressee,
        text: payload.text,
        // Raw non-message frames are decoded into a monitor line by the panel;
        // legacy payloads without `kind` are treated as text messages.
        kind: payload.kind ?? 'message',
        msgid: payload.msgid,
        at: Date.now(),
      };
      setMessages((prev) => [...prev, msg]);
    });

    subscribe<StateChangeDto>('aprs-message:state', (payload) => {
      setMessages((prev) => applyState(prev, payload.msgid, payload.state, Date.now()));
    });

    // Backend own-send echo (spec §7, tuxlink-dmwte task 10). UNCONDITIONAL —
    // every instance appends the echo so its feed is reconstructible from
    // events alone. The SENDING window's optimistic append (in `send`, kept
    // EXACTLY as is for RF-honesty) already recorded this msgid, so dedupe by
    // msgid: the sender skips the echo, every other window appends it.
    subscribe<SentMsgDto>('aprs-message:sent', (payload) => {
      setMessages((prev) => {
        if (prev.some((m) => m.msgid === payload.msgid)) return prev;
        const msg: ChannelMessage = {
          id: payload.msgid,
          direction: 'out',
          from: 'me',
          // Blank addressee ⇒ broadcast (`→ all`).
          to: payload.addressee === '' ? null : payload.addressee,
          text: payload.text,
          kind: 'message',
          msgid: payload.msgid,
          state: 'sent',
          // Honest backend-clock stamp carried on the wire (snake_case field).
          at: payload.at_ms,
        };
        return [...prev, msg];
      });
    });

    subscribe<boolean>('aprs-listening:change', (payload) => {
      setListening(payload);
    });

    return () => {
      mounted = false;
      for (const un of unlistens) un();
    };
  }, []);

  // Cross-window snapshot handshake (spec §7, tuxlink-dmwte task 10), role-gated
  // and kept SEPARATE from the unconditional subscriptions above so a role
  // change never re-subscribes the echo/inbound/state listeners. Mirrors
  // useAprsPositions' host/client mechanics + the 250 ms / 3 s retry amendment.
  useEffect(() => {
    if (role !== 'host' && role !== 'client') return;
    let mounted = true;
    const unlisteners: Array<() => void> = [];

    if (role === 'host') {
      // Answer a new window's request with the current feed (idempotent).
      listen(CHAT_SNAPSHOT_REQUEST, () => {
        if (!mounted) return;
        void emit(CHAT_SNAPSHOT_REPLY, messagesRef.current).catch(() => {});
      })
        .then((un) => {
          if (!mounted) un();
          else unlisteners.push(un);
        })
        .catch(() => {});
    }

    if (role === 'client') {
      // Retry state (spec §7): re-emit the request every SNAPSHOT_RETRY_MS until
      // the first reply lands, giving up cleanly after SNAPSHOT_GIVE_UP_MS. Both
      // timers are cleared on reply, on give-up, and on unmount.
      let retryTimer: ReturnType<typeof setInterval> | null = null;
      let giveUpTimer: ReturnType<typeof setTimeout> | null = null;
      const stopRetry = () => {
        if (retryTimer !== null) {
          clearInterval(retryTimer);
          retryTimer = null;
        }
        if (giveUpTimer !== null) {
          clearTimeout(giveUpTimer);
          giveUpTimer = null;
        }
      };
      unlisteners.push(stopRetry);

      // Register the reply listener FIRST, then request — so the host's answer
      // (whenever it arrives) can't be missed.
      listen<ChannelMessage[]>(CHAT_SNAPSHOT_REPLY, (e) => {
        if (!mounted) return;
        stopRetry();
        const incoming = e.payload ?? [];
        setMessages((prev) => mergeSnapshot(prev, incoming));
      })
        .then((un) => {
          if (!mounted) {
            un();
            return;
          }
          unlisteners.push(un);
          const request = () => void emit(CHAT_SNAPSHOT_REQUEST).catch(() => {});
          request(); // fire immediately on mount
          retryTimer = setInterval(request, SNAPSHOT_RETRY_MS);
          giveUpTimer = setTimeout(stopRetry, SNAPSHOT_GIVE_UP_MS);
        })
        .catch(() => {});
    }

    return () => {
      mounted = false;
      for (const un of unlisteners) un();
    };
  }, [role]);

  // Most-recently-heard-first, deduped by callsign — derived from inbound
  // senders only (we don't list ourselves).
  const heardStations = useMemo<HeardStation[]>(() => {
    const lastHeard = new Map<string, number>();
    for (const m of messages) {
      if (m.direction !== 'in') continue;
      const prev = lastHeard.get(m.from);
      if (prev === undefined || m.at > prev) lastHeard.set(m.from, m.at);
    }
    return [...lastHeard.entries()]
      .map(([call, at]) => ({ call, lastHeard: at }))
      .sort((a, b) => b.lastHeard - a.lastHeard);
  }, [messages]);

  const send = useCallback(
    async (recipient: string | null, text: string): Promise<string> => {
      // Normalize empty/whitespace recipient to null ⇒ broadcast.
      const call = recipient && recipient.trim() ? recipient.trim() : null;
      // Mint no local id. Await the backend; on reject, let it propagate WITHOUT
      // appending a message (RF-honesty).
      const id = await invoke<string>('aprs_send', { call, text });
      const msg: ChannelMessage = {
        id,
        direction: 'out',
        // The sender is us; the backend stamps the wire callsign. We display
        // the addressee, so "from" is a local marker only.
        from: 'me',
        to: call,
        text,
        kind: 'message',
        msgid: id,
        state: 'sent',
        at: Date.now(),
      };
      setMessages((prev) => [...prev, msg]);
      return id;
    },
    [],
  );

  const getConfig = useCallback(
    (): Promise<AprsConfigDto> => invoke<AprsConfigDto>('aprs_config_get'),
    [],
  );

  const setConfig = useCallback(
    (dto: AprsConfigDto): Promise<void> => invoke('aprs_config_set', { dto }),
    [],
  );

  return { messages, heardStations, listening, send, getConfig, setConfig };
}
