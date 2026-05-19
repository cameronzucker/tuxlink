/**
 * SessionLog.tsx — Session log pane, Task 15 (spec §5.5).
 *
 * Bottom strip, resizable (default 120px, min 60, max 50% viewport height).
 * Persists height to localStorage.
 *
 * Header: session state label + [Human | Raw] toggle + Copy button.
 * Body:   scrollable log line list; live-tail auto-scroll; pauses on
 *         scroll-up; resumes on scroll-to-bottom or new-session start.
 *
 * Tauri IPC (mocked in tests):
 *   - listen('session_log:line', ...)  → appends lines to state
 *   - invoke('session_log_snapshot')  → seeds initial lines on mount
 *
 * Does NOT edit AppShell.tsx — the sessionlog region wiring is owned by
 * the orchestrator integration commit (spec §4.3).
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { humanProjection, rawProjection, type LogLineDto } from './logProjection';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_HEIGHT_PX = 120;
const MIN_HEIGHT_PX = 60;
const HEIGHT_STORAGE_KEY = 'tuxlink.sessionLog.height';

type Projection = 'human' | 'raw';

export type SessionState = 'Idle' | 'Connecting' | 'In-session' | 'Disconnecting';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface SessionLogProps {
  sessionState?: SessionState;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionLog({ sessionState = 'Idle' }: SessionLogProps) {
  // All raw log lines (both projections read from this)
  const [lines, setLines] = useState<LogLineDto[]>([]);
  const [projection, setProjection] = useState<Projection>('human');

  // Auto-scroll: "stuck to bottom" means we scroll on new lines
  const [stuckToBottom, setStuckToBottom] = useState(true);

  // Resizable height
  const [height, setHeight] = useState<number>(() => {
    const stored = typeof localStorage !== 'undefined'
      ? localStorage.getItem(HEIGHT_STORAGE_KEY)
      : null;
    return stored ? parseInt(stored, 10) : DEFAULT_HEIGHT_PX;
  });

  const scrollRef = useRef<HTMLDivElement>(null);
  const isResizingRef = useRef(false);
  const dragStartYRef = useRef(0);
  const dragStartHeightRef = useRef(0);

  // ---------------------------------------------------------------------------
  // Tauri IPC: seed from snapshot + subscribe to live events
  // ---------------------------------------------------------------------------

  useEffect(() => {
    // Seed from snapshot (late subscriber / pane re-open)
    invoke<LogLineDto[]>('session_log_snapshot')
      .then(snapshot => {
        if (snapshot.length > 0) {
          setLines(snapshot);
        }
      })
      .catch(() => {
        // Backend not available (offline mode / pre-bootstrap) — start empty
      });

    // Subscribe to live line events
    let unlistenFn: (() => void) | null = null;
    listen<LogLineDto>('session_log:line', event => {
      setLines(prev => [...prev, event.payload]);
    }).then(unlisten => {
      unlistenFn = unlisten;
    });

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, []);

  // ---------------------------------------------------------------------------
  // Auto-scroll: scroll to bottom when new lines arrive (if stuck)
  // ---------------------------------------------------------------------------

  useEffect(() => {
    if (stuckToBottom && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines, stuckToBottom]);

  // ---------------------------------------------------------------------------
  // Scroll event: detect when user scrolls up (pause) or back to bottom (resume)
  // ---------------------------------------------------------------------------

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const el = scrollRef.current;
    const isAtBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 4;
    setStuckToBottom(isAtBottom);
  }, []);

  const scrollToBottom = useCallback(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
    setStuckToBottom(true);
  }, []);

  // ---------------------------------------------------------------------------
  // Projected lines
  // ---------------------------------------------------------------------------

  const projectedLines =
    projection === 'human' ? humanProjection(lines) : rawProjection(lines);

  // ---------------------------------------------------------------------------
  // Copy
  // ---------------------------------------------------------------------------

  const handleCopy = useCallback(() => {
    const text = projectedLines
      .map(l => `${l.timestampIso} [${l.level}] ${l.source}: ${l.message}`)
      .join('\n');
    navigator.clipboard.writeText(text).catch(() => {
      // Clipboard not available (headless/test) — silently ignore
    });
  }, [projectedLines]);

  // ---------------------------------------------------------------------------
  // Resize drag
  // ---------------------------------------------------------------------------

  const persistHeight = useCallback((h: number) => {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(HEIGHT_STORAGE_KEY, String(h));
    }
  }, []);

  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      isResizingRef.current = true;
      dragStartYRef.current = e.clientY;
      dragStartHeightRef.current = height;
      e.preventDefault();

      const onMouseMove = (ev: MouseEvent) => {
        if (!isResizingRef.current) return;
        const delta = dragStartYRef.current - ev.clientY; // dragging up = bigger
        const maxHeight = window.innerHeight * 0.5;
        const newHeight = Math.max(
          MIN_HEIGHT_PX,
          Math.min(maxHeight, dragStartHeightRef.current + delta),
        );
        setHeight(newHeight);
      };

      const onMouseUp = () => {
        isResizingRef.current = false;
        setHeight(h => {
          persistHeight(h);
          return h;
        });
        window.removeEventListener('mousemove', onMouseMove);
        window.removeEventListener('mouseup', onMouseUp);
      };

      window.addEventListener('mousemove', onMouseMove);
      window.addEventListener('mouseup', onMouseUp);
    },
    [height, persistHeight],
  );

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div
      data-testid="session-log-root"
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: `${height}px`,
        minHeight: `${MIN_HEIGHT_PX}px`,
        borderTop: '1px solid #333',
        background: '#0d0d0d',
        color: '#d4d4d4',
        fontFamily: 'monospace',
        fontSize: '12px',
        position: 'relative',
        userSelect: isResizingRef.current ? 'none' : 'auto',
      }}
    >
      {/* Resize handle — drag upward to expand */}
      <div
        data-testid="resize-handle"
        onMouseDown={handleResizeMouseDown}
        style={{
          height: '4px',
          cursor: 'row-resize',
          background: '#222',
          flexShrink: 0,
        }}
        aria-label="Resize session log"
      />

      {/* Header bar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: '8px',
          padding: '2px 8px',
          borderBottom: '1px solid #222',
          flexShrink: 0,
          height: '28px',
        }}
      >
        <span
          data-testid="session-state-label"
          style={{ color: sessionStateColor(sessionState), marginRight: '4px' }}
        >
          {sessionState}
        </span>

        {/* Projection toggle */}
        <button
          data-testid="toggle-human"
          aria-pressed={projection === 'human'}
          onClick={() => setProjection('human')}
          style={toggleStyle(projection === 'human')}
        >
          Human
        </button>
        <button
          data-testid="toggle-raw"
          aria-pressed={projection === 'raw'}
          onClick={() => setProjection('raw')}
          style={toggleStyle(projection === 'raw')}
        >
          Raw
        </button>

        {/* Spacer */}
        <div style={{ flex: 1 }} />

        {/* Copy button */}
        <button
          data-testid="copy-button"
          onClick={handleCopy}
          title="Copy visible log to clipboard"
          style={copyButtonStyle}
        >
          Copy
        </button>
      </div>

      {/* Log body */}
      <div
        data-testid="session-log-lines"
        ref={scrollRef}
        onScroll={handleScroll}
        style={{
          flex: 1,
          overflow: 'auto',
          padding: '4px 8px',
          lineHeight: '1.4',
        }}
      >
        {projectedLines.map((line, idx) => (
          <div
            key={`${line.timestampIso}-${idx}`}
            data-level={line.level}
            data-source={line.source}
            style={{ color: levelColor(line.level) }}
          >
            <span style={{ color: '#555' }}>{formatTimestamp(line.timestampIso)}</span>
            {' '}
            <span style={{ color: sourceColor(line.source) }}>[{line.source}]</span>
            {' '}
            {line.message}
          </div>
        ))}
      </div>

      {/* Scroll-to-bottom button (shown when not stuck to bottom) */}
      {!stuckToBottom && (
        <button
          data-testid="scroll-to-bottom"
          onClick={scrollToBottom}
          title="Scroll to bottom (resume live tail)"
          style={{
            position: 'absolute',
            bottom: '8px',
            right: '8px',
            padding: '4px 8px',
            background: '#1a1a1a',
            border: '1px solid #444',
            color: '#aaa',
            cursor: 'pointer',
            fontSize: '11px',
          }}
        >
          ↓ Resume
        </button>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Style helpers (inline — no external CSS dep)
// ---------------------------------------------------------------------------

function sessionStateColor(state: SessionState): string {
  switch (state) {
    case 'Idle':          return '#888';
    case 'Connecting':    return '#f0b429';
    case 'In-session':    return '#3dd68c';
    case 'Disconnecting': return '#f0b429';
    default:              return '#888';
  }
}

function levelColor(level: LogLineDto['level']): string {
  switch (level) {
    case 'trace': return '#555';
    case 'debug': return '#777';
    case 'info':  return '#d4d4d4';
    case 'warn':  return '#f0b429';
    case 'error': return '#e05252';
    default:      return '#d4d4d4';
  }
}

function sourceColor(source: LogLineDto['source']): string {
  switch (source) {
    case 'backend':   return '#5fb3f9';
    case 'pat':       return '#9d75d8';
    case 'transport': return '#3dd68c';
    case 'wire':      return '#555';
    default:          return '#777';
  }
}

const toggleStyle = (active: boolean): React.CSSProperties => ({
  padding: '1px 8px',
  background: active ? '#2a2a2a' : 'transparent',
  border: active ? '1px solid #555' : '1px solid transparent',
  color: active ? '#eee' : '#777',
  cursor: 'pointer',
  fontSize: '11px',
  borderRadius: '2px',
});

const copyButtonStyle: React.CSSProperties = {
  padding: '1px 8px',
  background: 'transparent',
  border: '1px solid #333',
  color: '#888',
  cursor: 'pointer',
  fontSize: '11px',
  borderRadius: '2px',
};

function formatTimestamp(iso: string): string {
  // Show only the time portion (HH:MM:SS) to save horizontal space
  try {
    return new Date(iso).toISOString().slice(11, 19);
  } catch {
    return iso;
  }
}
