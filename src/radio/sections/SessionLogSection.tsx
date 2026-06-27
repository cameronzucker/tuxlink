// src/radio/sections/SessionLogSection.tsx
//
// Per-session live log section. Spec §3.7 + §4.3.
// Layout:
//   - 56 px fixed timestamp column + flex message column
//   - hanging-indent wrap
//   - severity colors + glyphs (⚠ for warn, ⊘ for alert)
//   - multi-paragraph error: bold summary + faint italic raw block
//   - 130 px scrollable region with auto-scroll
//   - Show raw / Auto-scroll / Copy controls
//
// Used by every per-mode panel (Telnet, Packet, ARDOP, VARA).

import { useEffect, useRef, useState } from 'react';
import { usePersistedState } from '../../util/usePersistedState';
import './SessionLogSection.css';

export const SESSION_LOG_VISIBLE_ENTRY_LIMIT = 500;
const SESSION_LOG_AUTO_FOLLOW_THRESHOLD_PX = 16;

export type SessionLogLevel = 'info' | 'ok' | 'warn' | 'alert' | 'raw';

export interface SessionLogEntry {
  /** HH:MM:SS string, monospace. */
  ts: string;
  level: SessionLogLevel;
  /** Human-shaped one-line message (for alert: bold summary line). */
  message: string;
  /** Optional raw protocol-level detail (renders below message in faint italic). */
  raw?: string;
}

export interface SessionLogSectionProps {
  entries: SessionLogEntry[];
  /** Optional initial state for Show raw / Auto-scroll. */
  initialShowRaw?: boolean;
  initialAutoScroll?: boolean;
  /** Optional copy handler; defaults to copying the rendered text. */
  onCopy?: () => void;
  /** Optional clear handler (operator smoke 2026-05-31). When provided, a
   *  Clear button renders next to Copy; clicking it invokes onClear and the
   *  log view resets according to the owning hook. */
  onClear?: () => void;
}

export function SessionLogSection({
  entries,
  initialShowRaw = false,
  initialAutoScroll = true,
  onCopy,
  onClear,
}: SessionLogSectionProps) {
  // tuxlink-9hjw3: the raw-log preference is sticky across panel close/reopen
  // (it used to reset to off on every remount). One global key — the operator's
  // "Show raw" choice applies to every mode's session log.
  const [showRaw, setShowRaw] = usePersistedState<boolean>(
    'session-log:show-raw',
    initialShowRaw,
    (v): v is boolean => typeof v === 'boolean',
  );
  const [autoScroll, setAutoScroll] = useState(initialAutoScroll);
  const scrollRef = useRef<HTMLDivElement>(null);
  const shouldFollowScrollRef = useRef(initialAutoScroll);

  // Auto-follow only while the operator is already near the live tail. Once
  // they scroll upward, new log lines preserve their scrollback position until
  // they manually return to the bottom or explicitly re-enable Auto-scroll.
  useEffect(() => {
    const scrollEl = scrollRef.current;
    if (!autoScroll || !scrollEl) return;
    if (shouldFollowScrollRef.current || isScrolledNearBottom(scrollEl)) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
      shouldFollowScrollRef.current = true;
    }
  }, [entries, autoScroll, showRaw]);

  const handleScroll = () => {
    const scrollEl = scrollRef.current;
    if (!scrollEl) return;
    shouldFollowScrollRef.current = isScrolledNearBottom(scrollEl);
  };

  const handleAutoScrollChange = (checked: boolean) => {
    shouldFollowScrollRef.current = checked;
    setAutoScroll(checked);
  };

  const filtered = showRaw ? entries : entries.filter(e => e.level !== 'raw');
  const hiddenCount = Math.max(0, filtered.length - SESSION_LOG_VISIBLE_ENTRY_LIMIT);
  const visibleEntries = hiddenCount > 0
    ? filtered.slice(-SESSION_LOG_VISIBLE_ENTRY_LIMIT)
    : filtered;

  return (
    <section className="radio-panel-sec session-log-section"
             data-testid="session-log-section">
      <h5>
        Session log
        <span className="live">live tail</span>
      </h5>
      <div className="log-scroll" ref={scrollRef} onScroll={handleScroll}>
        {hiddenCount > 0 && (
          <div className="log-entry log-entry-info log-entry-limit-note"
               data-testid="session-log-limit-note">
            <span className="log-ts">...</span>
            <span className="log-msg">
              Showing latest {SESSION_LOG_VISIBLE_ENTRY_LIMIT} of {filtered.length} lines. Copy includes all filtered lines.
            </span>
          </div>
        )}
        {visibleEntries.map((e, i) => (
          <div key={i} className={`log-entry log-entry-${e.level}`}>
            <span className="log-ts">{e.ts}</span>
            <span className="log-msg">
              {e.level === 'alert' ? <strong>{e.message}</strong> : e.message}
              {e.raw && (
                <span className="log-raw"><br />{e.raw}</span>
              )}
            </span>
          </div>
        ))}
      </div>
      <div className="log-controls">
        <label>
          <input type="checkbox" checked={showRaw}
                 onChange={(ev) => setShowRaw(ev.target.checked)} />
          Show raw
        </label>
        <label>
          <input type="checkbox" checked={autoScroll}
                 onChange={(ev) => handleAutoScrollChange(ev.target.checked)} />
          Auto-scroll
        </label>
        <button type="button" className="log-copy"
                data-testid="log-copy-btn"
                onClick={onCopy ?? (() => copyEntries(filtered))}>
          Copy ↗
        </button>
        {onClear && (
          <button type="button" className="log-clear"
                  data-testid="log-clear-btn"
                  onClick={onClear}>
            Clear
          </button>
        )}
      </div>
    </section>
  );
}

function isScrolledNearBottom(el: HTMLElement): boolean {
  const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
  return distanceFromBottom <= SESSION_LOG_AUTO_FOLLOW_THRESHOLD_PX;
}

function copyEntries(entries: SessionLogEntry[]): void {
  const text = entries
    .map(e => `${e.ts}  ${e.message}${e.raw ? '\n            ' + e.raw : ''}`)
    .join('\n');
  void navigator.clipboard.writeText(text).catch(() => {});
}
