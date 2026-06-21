// src/aprs/DigipeatPathLayer.tsx
//
// Canvas2D overlay that animates cn84 digipeat paths on the Leaflet map.
// Render-less component — all DOM work happens imperatively in effects.
// NOT unit-tested (jsdom has no Canvas2D context); acceptance is operator
// grim-smoke. Keep every frame cheap: clearRect + stroke + arc only.
//
// CONCURRENT traces (tuxlink-qnu6): the layer owns a Map of active traces keyed
// by station call. A new trigger ADDS (or, for the same call, restarts) a trace
// without disturbing the others — multiple stations' paths animate side by side
// and each fades + self-prunes on its own schedule. A single bounded rAF draws
// every active trace per frame and stops once the set empties (never perpetual).

import { useEffect, useRef } from 'react';
import { useLeafletMap } from '../map/LeafletMapContext';
import { reportFrontendError } from '../frontendErrorLog';
import { traceProgress, trimPath } from './digipeatAnim';
import type { PathSegment } from './digipeatPath';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Mirror the safe() wrapper from AprsPositionsMap: log and skip on throw. */
function safe(source: string, what: string, fn: () => void): void {
  try {
    fn();
  } catch (e) {
    reportFrontendError(
      source,
      `${what}: ${e instanceof Error ? e.message : String(e)}`,
      e instanceof Error ? e.stack : undefined,
    );
  }
}

const ERROR_SOURCE = 'digipeat-path-anim';

/**
 * A request to (re)start one station's trace. `key` bumps on every fire so React
 * sees a new identity even when the same station re-triggers; `call` keys the
 * trace so a station's re-beacon restarts ITS animation while others keep
 * running.
 */
export interface TraceTrigger {
  key: number;
  call: string;
  segments: PathSegment[];
}

interface ActiveTrace {
  segments: PathSegment[];
  start: number; // performance.now() when this trace began
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * Each non-null `trigger` (new `key`) adds a concurrent bounded Canvas2D trace
 * over the Leaflet map container, keyed by `trigger.call`. Returns null —
 * renders nothing into React's tree.
 *
 * Perf contract: each frame is only clearRect + stroke + arc ops — no Leaflet
 * re-tessellation — and cost scales with the (small, bounded) number of active
 * traces. The loop is BOUNDED: it stops once every trace has reached phase
 * 'done' and never schedules another frame after that.
 */
export function DigipeatPathLayer({ trigger }: { trigger: TraceTrigger | null }): null {
  const map = useLeafletMap();

  // The canvas element (created once per map), the rAF handle (0 = not running),
  // and the set of currently-animating traces keyed by station call.
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rafRef = useRef<number>(0);
  const tracesRef = useRef<Map<string, ActiveTrace>>(new Map());

  // ---------------------------------------------------------------------------
  // Canvas lifetime — created once, removed on component unmount / map change.
  // Separate effect with [map] deps so it outlives individual triggers.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    if (!map) return;

    // Guard against StrictMode double-invoke: re-use existing canvas if present.
    if (!canvasRef.current) {
      const canvas = document.createElement('canvas');
      canvas.style.position = 'absolute';
      canvas.style.left = '0';
      canvas.style.top = '0';
      canvas.style.pointerEvents = 'none';
      // z-index 450: above tile pane (400), below marker pane (600) and popup pane (700).
      canvas.style.zIndex = '450';
      map.getContainer().appendChild(canvas);
      canvasRef.current = canvas;
    }

    return () => {
      // Cancel any in-flight rAF, drop all traces, and remove the canvas.
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      tracesRef.current.clear();
      const canvas = canvasRef.current;
      if (canvas) {
        canvas.remove();
        canvasRef.current = null;
      }
    };
  }, [map]);

  // ---------------------------------------------------------------------------
  // Trigger registration — each new trigger adds/restarts a trace (keyed by
  // call) and kicks the shared loop if it is idle. A trigger that arrives while
  // the loop is running is simply picked up on the next frame (the loop reads
  // the live trace map), so concurrent traces never cancel one another.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    if (!map || !trigger) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    tracesRef.current.set(trigger.call, { segments: trigger.segments, start: performance.now() });

    // Loop already running — it will draw the freshly-added trace next frame.
    if (rafRef.current) return;

    const loop = (): void => {
      const currentCanvas = canvasRef.current;
      if (!currentCanvas) {
        rafRef.current = 0;
        return; // unmounted — bail entirely
      }

      // Keep the canvas sized to the container (handles window resize).
      safe(ERROR_SOURCE, 'resize canvas', () => {
        const size = map.getSize();
        if (currentCanvas.width !== size.x || currentCanvas.height !== size.y) {
          currentCanvas.width = size.x;
          currentCanvas.height = size.y;
        }
      });

      // Draw every active trace; prune the ones that have finished. Wrapped in
      // safe() so a transient throw mid zoom/pan is logged + skipped, never
      // crashed to the ErrorBoundary.
      safe(ERROR_SOURCE, 'draw frame', () => {
        const w = currentCanvas.width;
        const h = currentCanvas.height;
        ctx.clearRect(0, 0, w, h);
        ctx.lineCap = 'round';

        for (const [call, t] of tracesRef.current) {
          const { phase, drawProgress, opacity } = traceProgress(performance.now() - t.start);
          if (phase === 'done') {
            tracesRef.current.delete(call);
            continue;
          }

          ctx.globalAlpha = opacity;
          const { drawn, dot } = trimPath(t.segments, drawProgress);

          // Stroke each segment (solid through located hops, dashed across
          // unlocatable ones). Coordinate gotcha: our LatLon has .lon; Leaflet
          // wants { lat, lng }.
          ctx.strokeStyle = '#f0c24a';
          ctx.lineWidth = 2.5;
          for (const seg of drawn) {
            const fromPt = map.latLngToContainerPoint({ lat: seg.from.lat, lng: seg.from.lon });
            const toPt = map.latLngToContainerPoint({ lat: seg.to.lat, lng: seg.to.lon });
            ctx.beginPath();
            ctx.setLineDash(seg.kind === 'solid' ? [] : [6, 6]);
            ctx.moveTo(fromPt.x, fromPt.y);
            ctx.lineTo(toPt.x, toPt.y);
            ctx.stroke();
          }

          // The leading-edge packet dot: white fill + amber glow ring.
          if (dot) {
            const dotPt = map.latLngToContainerPoint({ lat: dot.lat, lng: dot.lon });
            ctx.beginPath();
            ctx.arc(dotPt.x, dotPt.y, 4, 0, 2 * Math.PI);
            ctx.fillStyle = '#ffffff';
            ctx.fill();
            ctx.beginPath();
            ctx.arc(dotPt.x, dotPt.y, 7, 0, 2 * Math.PI);
            ctx.strokeStyle = '#f0c24a';
            ctx.lineWidth = 1.5;
            ctx.setLineDash([]);
            ctx.stroke();
          }

          ctx.globalAlpha = 1;
        }
      });

      // Stop once every trace has finished — do NOT schedule another frame.
      if (tracesRef.current.size === 0) {
        safe(ERROR_SOURCE, 'clear on empty', () => {
          const size = map.getSize();
          ctx.clearRect(0, 0, size.x, size.y);
        });
        rafRef.current = 0;
        return;
      }

      rafRef.current = requestAnimationFrame(loop);
    };

    rafRef.current = requestAnimationFrame(loop);
  }, [trigger, map]);

  return null;
}
