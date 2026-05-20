/**
 * SessionLog — bottom human-shaped session log (Mock B `.session-log.human`).
 *
 * Source: the MOCK B session-log block (human steps + Raw/Human toggle). Styling
 * lives in AppShell.css (`.layout-b .session-log`). Shown by default in Mock B;
 * View → Session Log toggles it.
 *
 * Tauri IPC (real backend): subscribe-then-snapshot pattern (adrev #3):
 *   1. Attach listen('session_log:line') FIRST so no events are lost in the
 *      window between subscribe and first listen.
 *   2. THEN fetch invoke('session_log_snapshot') to seed history.
 *   3. Merge both streams, deduplicating on `seq` (adrev #4 — timestamp
 *      collisions can drop lines if deduped on timestampIso).
 *
 * Human projection summarizes; Raw shows everything. In the dev fixture (no
 * backend), the mock's human steps are shown directly.
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

/**
 * Merge `incoming` lines into the existing `Map<seq, LogLineDto>`.
 * Returns a new Map if any lines were added; returns the same reference if
 * nothing changed (allows React to skip re-renders on no-op updates).
 */
function mergeLines(
  prev: Map<number, LogLineDto>,
  incoming: LogLineDto[],
): Map<number, LogLineDto> {
  let changed = false;
  let next = prev;
  for (const line of incoming) {
    if (!next.has(line.seq)) {
      if (!changed) {
        next = new Map(prev); // copy-on-first-write
        changed = true;
      }
      next.set(line.seq, line);
    }
  }
  return next;
}

/** HH:MM:SS from an ISO timestamp (mock step-ts format). */
function hhmmss(iso: string): string {
  try {
    return new Date(iso).toISOString().slice(11, 19);
  } catch {
    return iso;
  }
}

export function SessionLog() {
  // Internal state is a Map<seq, LogLineDto> for O(1) dedupe on seq.
  // Sorted array for rendering is derived below.
  const [lineMap, setLineMap] = useState<Map<number, LogLineDto>>(new Map());
  const [projection, setProjection] = useState<Projection>('human');
  const [stuckToBottom, setStuckToBottom] = useState(true);
  const [height, setHeight] = useState<number>(() => {
    const stored =
      typeof localStorage !== 'undefined' ? localStorage.getItem(HEIGHT_STORAGE_KEY) : null;
    return stored ? parseInt(stored, 10) : DEFAULT_HEIGHT_PX;
  });

  const scrollRef = useRef<HTMLDivElement>(null);
  const isResizingRef = useRef(false);

  // IPC subscribe-then-snapshot (real backend only; dev shows fixed mock steps).
  //
  // Order is critical (adrev #3):
  //   Step 1 — attach the live listener FIRST so events in the window between
  //             subscribe and snapshot arrive are not lost.
  //   Step 2 — THEN fetch the snapshot (invoke after listen resolves).
  //   Both streams are deduped on `seq` (adrev #4) via mergeLines().
  useEffect(() => {
    if (DEV_FIXTURE) return;
    let mounted = true;
    let unlistenFn: (() => void) | null = null;

    // Step 1: subscribe. The Tauri listen() API returns Promise<UnlistenFn>;
    // the callback is registered synchronously inside listen() before the
    // promise resolves, so events in the overlap window are captured.
    const listenPromise = listen<LogLineDto>('session_log:line', (event) => {
      if (mounted) {
        setLineMap((prev) => mergeLines(prev, [event.payload]));
      }
    });

    listenPromise.then((un) => {
      if (mounted) {
        unlistenFn = un;
      } else {
        // Component unmounted before the listen resolved — release immediately.
        un();
        return;
      }

      // Step 2: fetch snapshot AFTER the listener is confirmed attached.
      invoke<LogLineDto[]>('session_log_snapshot')
        .then((snap) => {
          if (mounted && snap.length > 0) {
            setLineMap((prev) => mergeLines(prev, snap));
          }
        })
        .catch(() => {
          /* offline / pre-bootstrap — start empty */
        });
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
  }, [lineMap, stuckToBottom]);

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
  // Derive a sorted array from the Map, ordered by seq ascending.
  const devSteps: DevLogStep[] = DEV_FIXTURE ? DEV_SESSION_LINES : [];
  const sortedLines = Array.from(lineMap.values()).sort((a, b) => a.seq - b.seq);
  const projected = projection === 'human' ? humanProjection(sortedLines) : rawProjection(sortedLines);

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
                <div className="step" key={line.seq > 0 ? line.seq : `summary-${i}`}>
                  <span className="step-ts">{hhmmss(line.timestampIso)}</span>
                  <span>{line.message}</span>
                </div>
              ) : (
                <div key={line.seq > 0 ? line.seq : `summary-${i}`} data-level={line.level} data-source={line.source}>
                  <span className="ts">{hhmmss(line.timestampIso)}</span> {line.message}
                </div>
              ),
            )}
      </div>
    </div>
  );
}
