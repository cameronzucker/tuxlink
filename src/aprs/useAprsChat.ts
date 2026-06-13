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

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AprsConfigDto,
  ChannelMessage,
  HeardStation,
  InboundMsgDto,
  StateChangeDto,
} from './aprsTypes';

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

export function useAprsChat(): UseAprsChat {
  const [messages, setMessages] = useState<ChannelMessage[]>([]);
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
      const msg: ChannelMessage = {
        id: payload.msgid ?? nextLocalId(),
        direction: 'in',
        from: payload.sender,
        // Blank addressee ⇒ broadcast (`→ all`).
        to: payload.addressee === '' ? null : payload.addressee,
        text: payload.text,
        msgid: payload.msgid,
        at: Date.now(),
      };
      setMessages((prev) => [...prev, msg]);
    });

    subscribe<StateChangeDto>('aprs-message:state', (payload) => {
      setMessages((prev) => applyState(prev, payload.msgid, payload.state, Date.now()));
    });

    subscribe<boolean>('aprs-listening:change', (payload) => {
      setListening(payload);
    });

    return () => {
      mounted = false;
      for (const un of unlistens) un();
    };
  }, []);

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
