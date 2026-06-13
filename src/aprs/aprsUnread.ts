// src/aprs/aprsUnread.ts
//
// Pure unread-count helper for the APRS status-strip control. "Unread" = inbound
// messages received after the operator last viewed the chat (the seen watermark,
// an epoch-ms held in AppShell, reset when the APRS dock tab is opened).

import type { Thread } from './aprsTypes';

/// Count inbound messages across all threads with `at` strictly greater than
/// `sinceMs`. Outbound messages never count as unread.
export function countUnread(threads: Record<string, Thread>, sinceMs: number): number {
  let n = 0;
  for (const thread of Object.values(threads)) {
    for (const m of thread.messages) {
      if (m.direction === 'in' && m.at > sinceMs) n += 1;
    }
  }
  return n;
}
