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
import './SessionLogSection.css';

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
   *  log view resets. The backend snapshot buffer is NOT cleared — clear is
   *  panel-local; new lines after the clear repopulate the view normally. */
  onClear?: () => void;
}

export function SessionLogSection({
  entries,
  initialShowRaw = false,
  initialAutoScroll = true,
  onCopy,
  onClear,
}: SessionLogSectionProps) {
  const [showRaw, setShowRaw] = useState(initialShowRaw);
  const [autoScroll, setAutoScroll] = useState(initialAutoScroll);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll on new entries when the toggle is on. The operator
  // scroll-back pauses auto-scroll via the onScroll handler.
  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries, autoScroll, showRaw]);

  const filtered = showRaw ? entries : entries.filter(e => e.level !== 'raw');

  return (
    <section className="radio-panel-sec session-log-section"
             data-testid="session-log-section">
      <h5>
        Session log
        <span className="live">live tail</span>
      </h5>
      <div className="log-scroll" ref={scrollRef}>
        {filtered.map((e, i) => (
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
                 onChange={(ev) => setAutoScroll(ev.target.checked)} />
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

function copyEntries(entries: SessionLogEntry[]): void {
  const text = entries
    .map(e => `${e.ts}  ${e.message}${e.raw ? '\n            ' + e.raw : ''}`)
    .join('\n');
  void navigator.clipboard.writeText(text).catch(() => {});
}
