// src/aprs/aprsUnread.ts
//
// Pure unread-count helper for the APRS status-strip control. "Unread" = inbound
// channel messages received after the operator last viewed the chat (the seen
// watermark, an epoch-ms held in AppShell, reset when the APRS dock tab is
// opened).

import type { ChannelMessage } from './aprsTypes';

/// Count inbound messages in the flat channel feed with `at` strictly greater
/// than `sinceMs`. Outbound messages never count as unread.
export function countUnread(messages: ChannelMessage[], sinceMs: number): number {
  let n = 0;
  for (const m of messages) {
    if (m.direction === 'in' && m.at > sinceMs) n += 1;
  }
  return n;
}
