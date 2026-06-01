// src/radio/sections/useSessionLog.ts
//
// Live wiring for SessionLogSection. Subscribes to the backend's
// `session_log:line` event stream (spec §3.3 + §5.5) and seeds from
// `session_log_snapshot` on mount so late-mounting panels still see
// the buffered history.
//
// Returns SessionLogEntry[] (the shape SessionLogSection expects),
// projected from the backend's LogLineDto wire format via a level/
// source → SessionLogLevel mapping.
//
// Codex P2-R1: replaces the empty `initialLogEntries = []` placeholder
// in TelnetRadioPanel. Same pattern reused by Packet (P3) and ARDOP
// (P4) per spec §4.3.

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type { LogLineDto } from '../../session/logProjection';
import type { SessionLogEntry, SessionLogLevel } from './SessionLogSection';

/**
 * Project a backend LogLineDto onto the panel-facing SessionLogEntry
 * shape. The backend uses (level, source) pairs; the panel uses a flat
 * severity enum + an optional `raw` continuation block.
 *
 * Mapping (spec §4.3 + §5.5):
 *   - `*** Session ...` markers and *** annotations from any source
 *     surface as 'info' (operator-relevant boundary)
 *   - source='wire' → 'raw' (B2F protocol noise; hidden unless Show raw)
 *   - level='trace'|'debug' → 'raw' (verbose backend introspection)
 *   - level='error' → 'alert' (visually distinct; spec §4.3 ⊘ glyph)
 *   - level='warn' → 'warn' (spec §4.3 ⚠ glyph)
 *   - level='info' on 'backend' or 'transport' → 'info' or 'ok' depending
 *     on whether the message reads like a positive outcome
 */
export function toSessionLogEntry(line: LogLineDto): SessionLogEntry {
  return {
    ts: extractHms(line.timestampIso),
    level: projectLevel(line),
    message: line.message,
  };
}

export function toSessionLogEntries(lines: LogLineDto[]): SessionLogEntry[] {
  return lines.map(toSessionLogEntry);
}

/**
 * `2026-05-31T19:35:58.123Z` → `19:35:58`. The panel's timestamp column
 * is fixed at 56 px and renders HH:MM:SS only; sub-second precision is
 * dropped because it does not fit and is not operator-meaningful at the
 * radio-panel layer (debug-level detail goes via `Show raw`).
 */
function extractHms(timestampIso: string): string {
  const t = timestampIso.indexOf('T');
  if (t < 0) return timestampIso.slice(0, 8);
  return timestampIso.slice(t + 1, t + 9);
}

function projectLevel(line: LogLineDto): SessionLogLevel {
  // *** annotations are session-state boundaries from any source; they
  // pass through as operator-relevant 'info' regardless of level/source.
  // Without this, *** lines from 'wire' would get bucketed to 'raw' and
  // hidden by the default Show-raw=off filter.
  if (line.message.includes('***')) return 'info';

  if (line.source === 'wire') return 'raw';
  if (line.level === 'trace' || line.level === 'debug') return 'raw';
  if (line.level === 'error') return 'alert';
  if (line.level === 'warn') return 'warn';
  return 'info';
}

/**
 * Merge a single LogLineDto into an existing seq-sorted list. Used by
 * both the live-event handler and the snapshot ingestion path.
 *
 *  - seq > 0 deduplicates by seq (the backend's canonical line id; the
 *    snapshot buffer and the live stream share this id space). Re-adding
 *    a line already in the list is a no-op.
 *  - seq === 0 is a synthetic frontend-only line (no canonical identity);
 *    always appended to the end.
 *  - Real lines insert at the correct seq-ordered position so a snapshot
 *    that resolves AFTER live events have already streamed doesn't put
 *    older entries below newer ones.
 *
 * Exported for unit tests.
 */
export function mergeLogLine(prev: LogLineDto[], next: LogLineDto): LogLineDto[] {
  if (next.seq === 0) {
    return [...prev, next];
  }
  // Dedup: same seq already present — skip.
  for (let i = 0; i < prev.length; i++) {
    if (prev[i].seq === next.seq) return prev;
  }
  // Insert preserving seq order. seq=0 synthetics keep their tail
  // position; real seq comparisons against them are skipped here.
  let i = prev.length;
  while (i > 0 && prev[i - 1].seq > 0 && prev[i - 1].seq > next.seq) {
    i--;
  }
  return [...prev.slice(0, i), next, ...prev.slice(i)];
}

/** Merge a batch (e.g., session_log_snapshot) into the existing list. */
export function mergeLogLines(prev: LogLineDto[], batch: LogLineDto[]): LogLineDto[] {
  let result = prev;
  for (const line of batch) {
    result = mergeLogLine(result, line);
  }
  return result;
}

/**
 * Subscribe to the backend's `session_log:line` stream + seed from the
 * `session_log_snapshot` buffer. Returns the accumulated SessionLogEntry
 * list, suitable for SessionLogSection's `entries` prop.
 *
 * Codex R2 fix: subscribe FIRST, then fetch snapshot. Both paths merge
 * by `seq` so the late-resolving snapshot cannot overwrite a live event
 * that arrived during the fetch window (and so a slow snapshot fetch
 * cannot duplicate lines we already received via the live stream).
 *
 * Both Tauri calls degrade gracefully when the backend is absent
 * (pre-wizard, no Tauri context): snapshot resolves to [] silently,
 * listen attaches a no-op that cleans up on unmount.
 */
export function useSessionLog(): SessionLogEntry[] {
  const [lines, setLines] = useState<LogLineDto[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    // Helper: fetch the snapshot AFTER the listener is in place. Called
    // from the listen().then() callback so a live event delivered during
    // listen-registration cannot slip through unobserved.
    const fetchSnapshot = () => {
      invoke<LogLineDto[]>('session_log_snapshot')
        .then((snapshot) => {
          if (!cancelled && snapshot.length > 0) {
            setLines((prev) => mergeLogLines(prev, snapshot));
          }
        })
        .catch(() => {
          // Backend absent (offline / pre-bootstrap) — start empty.
        });
    };

    listen<LogLineDto>('session_log:line', (event) => {
      if (cancelled) return;
      setLines((prev) => mergeLogLine(prev, event.payload));
    })
      .then((u) => {
        if (cancelled) {
          u();
          return;
        }
        unlisten = u;
        fetchSnapshot();
      })
      .catch(() => {
        // listen() unavailable (test env without Tauri mock).
        // Still try the snapshot path so tests that mock invoke but not
        // listen don't silently lose seeded log lines.
        fetchSnapshot();
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  return toSessionLogEntries(lines);
}
