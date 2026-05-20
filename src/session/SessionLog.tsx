/**
 * SessionLog — bottom human-shaped session log (Mock B `.session-log.human`).
 *
 * Source: the MOCK B session-log block (human steps + Raw/Human toggle). Styling
 * lives in AppShell.css (`.layout-b .session-log`). Shown by default in Mock B;
 * View → Session Log toggles it.
 *
 * Tauri IPC (real backend): invoke('session_log_snapshot') seeds + listen(
 * 'session_log:line') tails. Human projection summarizes; Raw shows everything.
 * In the dev fixture (no backend), the mock's human steps are shown directly.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { humanProjection, rawProjection, type LogLineDto } from './logProjection';
import { DEV_FIXTURE, DEV_SESSION_LINES, type DevLogStep } from '../mailbox/devFixture';

const DEFAULT_HEIGHT_PX = 132;
const MIN_HEIGHT_PX = 60;
const HEIGHT_STORAGE_KEY = 'tuxlink.sessionLog.height';

type Projection = 'human' | 'raw';

/** HH:MM:SS from an ISO timestamp (mock step-ts format). */
function hhmmss(iso: string): string {
  try {
    return new Date(iso).toISOString().slice(11, 19);
  } catch {
    return iso;
  }
}

export function SessionLog() {
  const [lines, setLines] = useState<LogLineDto[]>([]);
  const [projection, setProjection] = useState<Projection>('human');
  const [stuckToBottom, setStuckToBottom] = useState(true);
  const [height, setHeight] = useState<number>(() => {
    const stored =
      typeof localStorage !== 'undefined' ? localStorage.getItem(HEIGHT_STORAGE_KEY) : null;
    return stored ? parseInt(stored, 10) : DEFAULT_HEIGHT_PX;
  });

  const scrollRef = useRef<HTMLDivElement>(null);
  const isResizingRef = useRef(false);

  // IPC seed + tail (real backend only; dev shows fixed mock steps).
  useEffect(() => {
    if (DEV_FIXTURE) return;
    let mounted = true;
    let unlistenFn: (() => void) | null = null;
    listen<LogLineDto>('session_log:line', (event) => {
      if (mounted) setLines((prev) => [...prev, event.payload]);
    }).then((un) => {
      if (mounted) unlistenFn = un;
      else un();
    });
    invoke<LogLineDto[]>('session_log_snapshot')
      .then((snap) => {
        if (mounted && snap.length > 0) {
          setLines((prev) => {
            const seen = new Set(prev.map((l) => l.timestampIso));
            const fresh = snap.filter((l) => !seen.has(l.timestampIso));
            return fresh.length > 0 ? [...fresh, ...prev] : prev;
          });
        }
      })
      .catch(() => {
        /* offline / pre-bootstrap — start empty */
      });
    return () => {
      mounted = false;
      unlistenFn?.();
    };
  }, []);

  useEffect(() => {
    if (stuckToBottom && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines, stuckToBottom]);

  const handleScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setStuckToBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 4);
  }, []);

  const onResizeMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      isResizingRef.current = true;
      const startY = e.clientY;
      const startH = height;
      e.preventDefault();
      const onMove = (ev: MouseEvent) => {
        if (!isResizingRef.current) return;
        const max = window.innerHeight * 0.5;
        const next = Math.max(MIN_HEIGHT_PX, Math.min(max, startH + (startY - ev.clientY)));
        setHeight(next);
      };
      const onUp = () => {
        isResizingRef.current = false;
        setHeight((h) => {
          if (typeof localStorage !== 'undefined') localStorage.setItem(HEIGHT_STORAGE_KEY, String(h));
          return h;
        });
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    },
    [height],
  );

  // Human steps to render: dev fixture supplies the mock's steps; otherwise the
  // human/raw projection of the real IPC lines.
  const devSteps: DevLogStep[] = DEV_FIXTURE ? DEV_SESSION_LINES : [];
  const projected = projection === 'human' ? humanProjection(lines) : rawProjection(lines);

  return (
    <div
      className={`session-log ${projection === 'human' ? 'human' : ''}`.trim()}
      data-testid="session-log-root"
      style={{ height: `${height}px`, minHeight: `${MIN_HEIGHT_PX}px` }}
    >
      <div
        data-testid="resize-handle"
        onMouseDown={onResizeMouseDown}
        style={{ height: '3px', cursor: 'row-resize', flexShrink: 0 }}
        aria-label="Resize session log"
      />
      <div className="log-header">
        <span className="live-dot" aria-hidden="true" />
        Session log
        <button
          type="button"
          className={`log-toggle ${projection === 'raw' ? 'active' : ''}`.trim()}
          data-testid="toggle-raw"
          aria-pressed={projection === 'raw'}
          onClick={() => setProjection('raw')}
        >
          Raw output
        </button>
        <button
          type="button"
          className={`log-toggle ${projection === 'human' ? 'active' : ''}`.trim()}
          data-testid="toggle-human"
          aria-pressed={projection === 'human'}
          onClick={() => setProjection('human')}
        >
          Human
        </button>
      </div>

      <div className="log-lines" data-testid="session-log-lines" ref={scrollRef} onScroll={handleScroll}>
        {DEV_FIXTURE
          ? devSteps.map((s, i) => (
              <div className="step" key={i}>
                <span className="step-ts">{s.ts}</span>
                <span className={s.ok ? 'ok' : undefined}>{s.message}</span>
              </div>
            ))
          : projected.map((line, i) =>
              projection === 'human' ? (
                <div className="step" key={`${line.timestampIso}-${i}`}>
                  <span className="step-ts">{hhmmss(line.timestampIso)}</span>
                  <span>{line.message}</span>
                </div>
              ) : (
                <div key={`${line.timestampIso}-${i}`} data-level={line.level} data-source={line.source}>
                  <span className="ts">{hhmmss(line.timestampIso)}</span> {line.message}
                </div>
              ),
            )}
      </div>
    </div>
  );
}
