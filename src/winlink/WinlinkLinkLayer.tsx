// src/winlink/WinlinkLinkLayer.tsx
//
// Canvas2D overlay that draws an animated curved arc representing the LIVE
// Winlink link on the Leaflet map.  Render-less component — all DOM work
// happens imperatively in effects.
//
// NOT unit-tested (jsdom has no Canvas2D context); acceptance is operator
// grim-smoke, identical to DigipeatPathLayer's policy.
//
// Canvas-lifecycle shell is copied verbatim from DigipeatPathLayer:
//   • canvas created once per map (z-index 451 — one above DigipeatPathLayer)
//   • bounded rAF that stops when inactive (no perpetual loop)
//   • safe() wrapper for every draw operation
//   • resize-to-container on each frame
//
// Truthful-now grammar — packet shapes:
//   connecting   → dashed #8fb3ff arc + ping pulse (hollow ring riding the head)
//   data-out     → solid arc + green  #5ce08a comet origin→peer
//   data-in      → solid arc + cyan   #7fd0ff comet peer→origin
//   busy         → amber #d9b13a dashed shimmer (marching offset)
//   error        → #e0683a flash (arc + central burst)
//   closing      → arc fading out (no packet)
//   idle         → nothing drawn
//
// ack/retry are NOT drawn — they are not in the truthful-now grammar for
// the current modem status event stream.
//
// Visual reference: dev/scratch/2026-06-22-winlink-link-animation-mock.html
// comet(), pingPulse(), busyShimmer() ported from that file.

import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from '../map/LeafletMapContext';
import { reportFrontendError } from '../frontendErrorLog';
import { linkDrawState } from './winlinkLinkAnim';
import { useModemStatus } from '../modem/useModemStatus';
import type { ModemStatus } from '../modem/types';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Mirror the safe() wrapper from DigipeatPathLayer / AprsPositionsMap. */
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

const ERROR_SOURCE = 'winlink-link-layer';

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

interface Pt { x: number; y: number }

/** Quadratic Bézier point at parameter t (0=from, 1=to). */
function bez(from: Pt, ctrl: Pt, to: Pt, t: number): Pt {
  const u = 1 - t;
  return {
    x: u * u * from.x + 2 * u * t * ctrl.x + t * t * to.x,
    y: u * u * from.y + 2 * u * t * ctrl.y + t * t * to.y,
  };
}

/**
 * Compute the perpendicular-bow control point for the arc between two
 * container points.  The bow is always to the "left" of the from→to vector
 * (consistent orientation regardless of direction).
 */
function arcCtrl(from: Pt, to: Pt, bow = 46): Pt {
  const mx = (from.x + to.x) / 2;
  const my = (from.y + to.y) / 2;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy);
  if (len < 1) return { x: mx, y: my }; // degenerate: same point
  // Perpendicular unit vector (rotated 90° CCW)
  const nx = -dy / len;
  const ny = dx / len;
  return { x: mx + nx * bow, y: my + ny * bow };
}

// ---------------------------------------------------------------------------
// Color helpers (ported from mock hexA())
// ---------------------------------------------------------------------------

function hexA(hex: string, a: number): string {
  const h = hex.replace('#', '');
  const n = parseInt(h.length === 3 ? h.split('').map((c) => c + c).join('') : h, 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  return `rgba(${r},${g},${b},${a})`;
}

// ---------------------------------------------------------------------------
// Packet drawing primitives (ported from mock: comet, pingPulse, busyShimmer)
// ---------------------------------------------------------------------------

/**
 * Comet packet — a bright head with a fading tail along the Bézier arc.
 * dir: 1 = from→to (data-out), -1 = to→from (data-in, pass t as 1-local).
 * Ported from mock comet().
 */
function drawComet(
  ctx: CanvasRenderingContext2D,
  from: Pt, ctrl: Pt, to: Pt,
  t: number,
  color: string,
  size: number,
  tailFrac: number,
  dir: 1 | -1,
): void {
  const head = bez(from, ctrl, to, t);
  const steps = 8;
  for (let i = 1; i <= steps; i++) {
    const tt = t - dir * (tailFrac * i / steps);
    if (tt < 0 || tt > 1) continue;
    const p = bez(from, ctrl, to, tt);
    const a = (1 - i / steps) * 0.6;
    ctx.beginPath();
    ctx.arc(p.x, p.y, size * (1 - (i / steps) * 0.6), 0, 2 * Math.PI);
    ctx.fillStyle = hexA(color, a);
    ctx.fill();
  }
  // Head glow
  ctx.beginPath();
  ctx.arc(head.x, head.y, size * 2.4, 0, 2 * Math.PI);
  ctx.fillStyle = hexA(color, 0.16);
  ctx.fill();
  // Head core
  ctx.beginPath();
  ctx.arc(head.x, head.y, size, 0, 2 * Math.PI);
  ctx.fillStyle = color;
  ctx.fill();
}

/**
 * Ping pulse — dashed probe line drawn out to parameter t, with a hollow
 * ring riding the head.  Ported from mock pingPulse().
 */
function drawPingPulse(
  ctx: CanvasRenderingContext2D,
  from: Pt, ctrl: Pt, to: Pt,
  t: number,
  color: string,
): void {
  ctx.setLineDash([4, 4]);
  ctx.strokeStyle = hexA(color, 0.7);
  ctx.lineWidth = 1.4;
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  // Approximate partial arc by sampling N segments up to t
  const N = 24;
  for (let i = 1; i <= N * t; i++) {
    const p = bez(from, ctrl, to, i / N);
    ctx.lineTo(p.x, p.y);
  }
  ctx.stroke();
  ctx.setLineDash([]);
  // Hollow ring at the head
  const head = bez(from, ctrl, to, t);
  ctx.beginPath();
  ctx.arc(head.x, head.y, 3.4, 0, 2 * Math.PI);
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.6;
  ctx.stroke();
}

/**
 * Busy shimmer — whole-arc amber dashed line with a marching offset, pulsing
 * alpha.  Ported from mock busyShimmer().
 */
function drawBusyShimmer(
  ctx: CanvasRenderingContext2D,
  from: Pt, ctrl: Pt, to: Pt,
  t: number,
  color: string,
): void {
  const pulse = 0.45 + 0.35 * Math.sin(t * Math.PI * 4);
  ctx.setLineDash([5, 3]);
  ctx.lineDashOffset = -t * 40;
  ctx.strokeStyle = hexA(color, pulse);
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  ctx.quadraticCurveTo(ctrl.x, ctrl.y, to.x, to.y);
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.lineDashOffset = 0;
}

/**
 * Error flash — full arc in error color + a central burst dot.
 * Fades via globalAlpha based on the intra-phase progress t.
 */
function drawErrorFlash(
  ctx: CanvasRenderingContext2D,
  from: Pt, ctrl: Pt, to: Pt,
  t: number,
  color: string,
): void {
  const alpha = Math.max(0, 1 - t);
  ctx.globalAlpha = alpha;
  ctx.setLineDash([2, 4]);
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  ctx.quadraticCurveTo(ctrl.x, ctrl.y, to.x, to.y);
  ctx.stroke();
  ctx.setLineDash([]);
  // Central burst
  const mid = bez(from, ctrl, to, 0.5);
  ctx.beginPath();
  ctx.arc(mid.x, mid.y, 5.5 * (1 - t * 0.5), 0, 2 * Math.PI);
  ctx.fillStyle = hexA(color, 0.6 * alpha);
  ctx.fill();
  ctx.globalAlpha = 1;
}

// ---------------------------------------------------------------------------
// Resting arc spine (always drawn when active, tinted by quality)
// ---------------------------------------------------------------------------

function drawArcSpine(
  ctx: CanvasRenderingContext2D,
  from: Pt, ctrl: Pt, to: Pt,
  quality: number,
  dashed: boolean,
  color: string,
): void {
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  ctx.quadraticCurveTo(ctrl.x, ctrl.y, to.x, to.y);
  ctx.strokeStyle = hexA(color, 0.15 + 0.35 * quality);
  ctx.lineWidth = dashed ? 1.2 : 1.8;
  if (dashed) {
    ctx.setLineDash([4, 4]);
  } else {
    ctx.setLineDash([]);
  }
  ctx.stroke();
  ctx.setLineDash([]);
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * Draws an animated Canvas2D arc over the Leaflet map for the LIVE Winlink
 * link.  Returns null — renders nothing into React's tree.
 *
 * Canvas z-index 451 (one above DigipeatPathLayer's 450).
 *
 * The rAF loop is BOUNDED: it stops when `linkDrawState` returns
 * `active === false` or when origin/peer are null, and only restarts when
 * those conditions change.  Never perpetually loops when there is nothing
 * to draw.
 *
 * Status is read via `statusRef` inside the long-lived rAF closure so it
 * always sees the latest value, never a stale capture.
 */
export function WinlinkLinkLayer({
  origin,
  peer,
}: {
  origin: { lat: number; lon: number } | null;
  peer: { lat: number; lon: number } | null;
}): null {
  const map = useLeafletMap();
  const { status } = useModemStatus();

  // Keep the latest status in a ref so the rAF closure always reads the
  // current value without needing to restart the loop.
  const statusRef = useRef<ModemStatus>(status);
  statusRef.current = status;

  // Keep the latest origin/peer in refs for the same reason.
  const originRef = useRef(origin);
  originRef.current = origin;
  const peerRef = useRef(peer);
  peerRef.current = peer;

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rafRef = useRef<number>(0);

  // -------------------------------------------------------------------------
  // Canvas lifetime — created once per map, removed on unmount / map change.
  // -------------------------------------------------------------------------
  useEffect(() => {
    if (!map) return;

    // Guard against StrictMode double-invoke: re-use existing canvas if present.
    if (!canvasRef.current) {
      const canvas = document.createElement('canvas');
      canvas.style.position = 'absolute';
      canvas.style.left = '0';
      canvas.style.top = '0';
      canvas.style.pointerEvents = 'none';
      // z-index 451: one above DigipeatPathLayer (450), below marker pane (600).
      canvas.style.zIndex = '451';
      map.getContainer().appendChild(canvas);
      canvasRef.current = canvas;
    }

    return () => {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      const canvas = canvasRef.current;
      if (canvas) {
        canvas.remove();
        canvasRef.current = null;
      }
    };
  }, [map]);

  // -------------------------------------------------------------------------
  // Animation driver — (re)starts when map, origin, peer, or status changes
  // while there is something to draw.  Stops itself when inactive.
  // -------------------------------------------------------------------------
  useEffect(() => {
    if (!map) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    // If the loop is already running, it will pick up the new refs on the
    // next frame — no need to restart it.
    if (rafRef.current) return;

    // Quick pre-check: don't even start the loop if currently inactive.
    const d0 = linkDrawState(statusRef.current);
    if (!d0.active || !originRef.current || !peerRef.current) return;

    const loop = (): void => {
      const currentCanvas = canvasRef.current;
      if (!currentCanvas) {
        rafRef.current = 0;
        return; // unmounted — bail entirely
      }

      // Keep canvas sized to the container (handles window resize / zoom).
      safe(ERROR_SOURCE, 'resize canvas', () => {
        const size = map.getSize();
        if (currentCanvas.width !== size.x || currentCanvas.height !== size.y) {
          currentCanvas.width = size.x;
          currentCanvas.height = size.y;
        }
      });

      safe(ERROR_SOURCE, 'draw frame', () => {
        const w = currentCanvas.width;
        const h = currentCanvas.height;
        ctx.clearRect(0, 0, w, h);

        // Read the LATEST status and props from refs — never stale captures.
        const d = linkDrawState(statusRef.current);
        const orig = originRef.current;
        const pr = peerRef.current;

        if (!d.active || !orig || !pr) {
          // Nothing to draw — stop the loop.
          rafRef.current = 0;
          return;
        }

        // Project lat/lon to container pixel coords.
        // Note: Leaflet wants { lat, lng } — our props use .lon; convert here.
        const fromPt = map.latLngToContainerPoint(
          L.latLng(orig.lat, orig.lon),
        );
        const toPt = map.latLngToContainerPoint(
          L.latLng(pr.lat, pr.lon),
        );

        const from: Pt = { x: fromPt.x, y: fromPt.y };
        const to: Pt = { x: toPt.x, y: toPt.y };
        const ctrl = arcCtrl(from, to, 46);

        // Parametric position for moving packets (0..1 cycling over time).
        const now = performance.now();

        ctx.save();
        ctx.lineCap = 'round';

        switch (d.phase) {
          case 'connecting': {
            // Dashed blue arc spine
            drawArcSpine(ctx, from, ctrl, to, d.quality, true, '#8fb3ff');
            // Ping pulse traveling origin→peer (cycle every 1200 ms)
            const pingT = (now % 1200) / 1200;
            drawPingPulse(ctx, from, ctrl, to, pingT, '#8fb3ff');
            break;
          }
          case 'data-out': {
            // Solid green arc spine
            drawArcSpine(ctx, from, ctrl, to, d.quality, false, '#5ce08a');
            // Comet origin→peer; size and speed scaled by flow
            const speedOut = 0.8 + d.flow * 0.4; // cycles per second
            const cometTOut = (now * speedOut / 1000) % 1;
            drawComet(ctx, from, ctrl, to, cometTOut, '#5ce08a', 3.6 + d.flow * 1.2, 0.16, 1);
            break;
          }
          case 'data-in': {
            // Solid cyan arc spine
            drawArcSpine(ctx, from, ctrl, to, d.quality, false, '#7fd0ff');
            // Comet peer→origin (direction = -1, so drive t from 1→0)
            const speedIn = 0.8 + d.flow * 0.4;
            const cometTIn = 1 - (now * speedIn / 1000) % 1;
            drawComet(ctx, from, ctrl, to, cometTIn, '#7fd0ff', 3.6 + d.flow * 1.2, 0.16, -1);
            break;
          }
          case 'busy': {
            // Amber dashed shimmer — no comet
            const shimmerT = (now % 2000) / 2000;
            drawBusyShimmer(ctx, from, ctrl, to, shimmerT, '#d9b13a');
            break;
          }
          case 'error': {
            // Red flash decaying over 1.5 s cycle
            const errorT = (now % 1500) / 1500;
            drawErrorFlash(ctx, from, ctrl, to, errorT, '#e0683a');
            break;
          }
          case 'closing': {
            // Arc fading out — alpha ramps down over a 1.2 s cycle
            const closeT = (now % 1200) / 1200;
            ctx.globalAlpha = Math.max(0, 1 - closeT);
            drawArcSpine(ctx, from, ctrl, to, d.quality, false, '#8fb3ff');
            ctx.globalAlpha = 1;
            break;
          }
          default:
            break;
        }

        ctx.restore();
      });

      // Re-check active state AFTER the draw call (the draw call itself may
      // have already bailed via early return above — that case sets rafRef=0
      // before returning, so this line is unreachable for it).
      if (rafRef.current === 0) return; // stopped inside safe() draw above

      rafRef.current = requestAnimationFrame(loop);
    };

    rafRef.current = requestAnimationFrame(loop);
  }, [map, status, origin, peer]);
  //         ^^^^^^^^^^^^^^^^^^^^^^^^^^^
  // When status/origin/peer change and the loop has stopped (active→inactive
  // transition), this effect re-runs and the pre-check at the top will either
  // restart the loop (now active) or leave it stopped (still inactive).

  return null;
}
