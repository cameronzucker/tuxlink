// src/aprs/DigipeatPathLayer.tsx
//
// Canvas2D overlay that animates the cn84 digipeat path on the Leaflet map.
// Render-less component — all DOM work happens imperatively in effects.
// NOT unit-tested (jsdom has no Canvas2D context); acceptance is operator
// grim-smoke. Keep every frame cheap: clearRect + stroke + arc only.

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

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * Whenever `path` is non-null, runs one bounded Canvas2D trace animation over
 * the Leaflet map container. Returns null — renders nothing into React's tree.
 *
 * Perf contract: each frame is only clearRect + stroke + arc ops — no Leaflet
 * re-tessellation. The loop is BOUNDED: it stops at phase 'done' and never
 * schedules another frame after that.
 */
export function DigipeatPathLayer({ path }: { path: PathSegment[] | null }): null {
  // Point 1: get the map; the outer effect bails if it is null.
  const map = useLeafletMap();

  // Stable ref for the canvas element so rAF closures access it without stale captures.
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  // Stable ref for the rAF handle so cleanup can cancel it.
  const rafRef = useRef<number>(0);

  // ---------------------------------------------------------------------------
  // Canvas lifetime — created once, removed on component unmount.
  // Separate effect with [] deps so it outlives any path change.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    // Point 2: bail if no map (context not yet ready).
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
      // Point 5 (cleanup): cancel any in-flight rAF and remove the canvas from the DOM.
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      const canvas = canvasRef.current;
      if (canvas) {
        canvas.remove();
        canvasRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map]);

  // ---------------------------------------------------------------------------
  // Animation loop — runs when path changes.
  // ---------------------------------------------------------------------------
  useEffect(() => {
    if (!map) return;
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // Cancel any in-flight animation from a previous path identity.
    cancelAnimationFrame(rafRef.current);
    rafRef.current = 0;

    if (!path || path.length === 0) {
      // Point 5 (null path): clear and stop.
      safe(ERROR_SOURCE, 'clear on null path', () => {
        const size = map.getSize();
        canvas.width = size.x;
        canvas.height = size.y;
        ctx.clearRect(0, 0, size.x, size.y);
      });
      return;
    }

    // Capture start time for this trace.
    const start = performance.now();

    // Point 3: the animation loop.
    // The phase check and rAF scheduling are OUTSIDE safe() so a transient
    // throw in draw ops never kills the loop or leaves it dangling.
    const loop = (): void => {
      const currentCanvas = canvasRef.current;
      if (!currentCanvas) return; // unmounted — bail entirely

      const elapsed = performance.now() - start;
      const { phase, drawProgress, opacity } = traceProgress(elapsed);

      // Point 3: resize canvas to container (handles window resize every frame).
      safe(ERROR_SOURCE, 'resize canvas', () => {
        const size = map.getSize();
        const w = size.x;
        const h = size.y;
        if (currentCanvas.width !== w || currentCanvas.height !== h) {
          currentCanvas.width = w;
          currentCanvas.height = h;
        }
      });

      if (phase === 'done') {
        // Point 3: clear and STOP — do NOT schedule another frame.
        safe(ERROR_SOURCE, 'clear on done', () => {
          const size = map.getSize();
          ctx.clearRect(0, 0, size.x, size.y);
        });
        return;
      }

      // Draw this frame inside safe() so a mid-zoom/pan throw is logged + skipped.
      safe(ERROR_SOURCE, 'draw frame', () => {
        const w = currentCanvas.width;
        const h = currentCanvas.height;

        // Clear.
        ctx.clearRect(0, 0, w, h);

        // Point 3: set opacity for the whole overlay.
        ctx.globalAlpha = opacity;

        // Shared stroke style (lineWidth/lineCap apply to all segments).
        ctx.strokeStyle = '#f0c24a';
        ctx.lineWidth = 2.5;
        ctx.lineCap = 'round';

        const { drawn, dot } = trimPath(path, drawProgress);

        // Stroke each segment.
        for (const seg of drawn) {
          // Point 3: project via latLngToContainerPoint.
          // Coordinate gotcha: our LatLon has .lon; Leaflet wants { lat, lng }.
          const fromPt = map.latLngToContainerPoint({ lat: seg.from.lat, lng: seg.from.lon });
          const toPt = map.latLngToContainerPoint({ lat: seg.to.lat, lng: seg.to.lon });

          ctx.beginPath();
          // Point 3: solid vs dashed style.
          if (seg.kind === 'solid') {
            ctx.setLineDash([]);
          } else {
            ctx.setLineDash([6, 6]);
          }
          ctx.moveTo(fromPt.x, fromPt.y);
          ctx.lineTo(toPt.x, toPt.y);
          ctx.stroke();
        }

        // Point 3: draw the leading-edge dot.
        if (dot) {
          const dotPt = map.latLngToContainerPoint({ lat: dot.lat, lng: dot.lon });
          const px = dotPt.x;
          const py = dotPt.y;

          // White fill circle.
          ctx.beginPath();
          ctx.arc(px, py, 4, 0, 2 * Math.PI);
          ctx.fillStyle = '#ffffff';
          ctx.fill();

          // Amber glow ring.
          ctx.beginPath();
          ctx.arc(px, py, 7, 0, 2 * Math.PI);
          ctx.strokeStyle = '#f0c24a';
          ctx.lineWidth = 1.5;
          ctx.setLineDash([]);
          ctx.stroke();
        }

        // Restore globalAlpha for next frame.
        ctx.globalAlpha = 1;
      });

      // Point 3: schedule next frame (only reached if phase !== 'done').
      rafRef.current = requestAnimationFrame(loop);
    };

    rafRef.current = requestAnimationFrame(loop);

    // Point 5: cleanup when path identity changes or map changes.
    return () => {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      // Clear the canvas on cleanup.
      safe(ERROR_SOURCE, 'clear on cleanup', () => {
        const size = map.getSize();
        ctx.clearRect(0, 0, size.x, size.y);
      });
    };
  }, [map, path]);

  return null;
}
