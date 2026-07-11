// Waterfall.tsx — Canvas2D FT-8 waterfall strip (Task C8, plan
// tuxlink-b026z.4 §Waterfall). Consumes the A6 backend's
// `ft8_waterfall_subscribe`/`ft8_waterfall_unsubscribe` commands and the
// `ft8-waterfall:columns` event (`WaterfallBatch { seq, firstColUtcMs, cols }`,
// each `col` a 512-entry u8 magnitude array over 0-3000 Hz, per ft8Types.ts).
//
// **Subscription lifecycle (spec §Waterfall "Lifecycle (token-counted,
// pinned)")**: the backend FFT consumer thread exists ONLY while >=1 live
// subscription token exists — zero live tokens must mean zero FFT work. This
// component subscribes ONLY while `expanded` is true (mirrored by mount: a
// parent that unmounts the component when the strip collapses gets the same
// effect via the cleanup below). A single `useEffect` keyed on `expanded`
// covers all three teardown triggers the spec calls out (collapse, unmount,
// and — implicitly — a prop flip while still mounted): React runs the
// cleanup function on every dependency change AND on unmount, so one cleanup
// body serves both. Subscribe/unsubscribe use the async subscriptionId
// TOKEN (never a plain counter) so a stale unsubscribe from a remounted
// effect can never decrement another window's live subscription — the
// disposed-before-resolved race unsubscribes the token as soon as it
// resolves, rather than skipping the unsubscribe silently.
//
// **Canvas2D paint (spec §Waterfall "Frontend")**: `putImageData` writes a
// 1px-wide column with the freshest magnitudes; the EXISTING content scrolls
// via a self-copy `drawImage(canvas, ...)` — probe-validated in the real
// WebKitGTK 605.1.15 aarch64 software-GL engine
// (`dev/scratch/canvas2d-waterfall-probe.html`, 2026-07-11). CSS transform is
// NOT used for the scroll (that was the station map's Leaflet-specific
// decision; this is the Canvas2D-specific one, independently probe-checked).
//
// **Gap rendering (spec §Waterfall "Gap rendering + discontinuity signal")**:
// the tap carries no discontinuity marker, so the backend stamps every batch
// with a monotonic `seq` + the wall-clock of its first column. A seq jump OR
// a wall-clock gap beyond the expected column cadence renders an explicit
// gap-marker column BEFORE the batch's own columns — never a silent
// scroll-join across a discontinuity. `detectGap`/`nextGapState` are pure and
// exported for direct unit testing; `paintColumn`/`paintBatch` are the
// imperative Canvas2D seam, also exported so tests can assert on a mocked
// context without needing a real canvas backend (jsdom has none).

import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { FT8_WATERFALL_EVENT, type SubDto, type WaterfallBatch } from './ft8Types';
import './Waterfall.css';

/** Tauri command names (A6 backend, §NewCommands waterfall rows). */
export const WATERFALL_SUBSCRIBE_COMMAND = 'ft8_waterfall_subscribe';
export const WATERFALL_UNSUBSCRIBE_COMMAND = 'ft8_waterfall_unsubscribe';

/** Bins per column — 2048-pt FFT cropped to 0-3000 Hz (spec §Waterfall). */
export const WATERFALL_BINS = 512;
/** Column cadence — 4 columns/s (spec §Waterfall FFT window/hop). */
export const COLUMN_CADENCE_MS = 250;
/** Slack multiplier before a wall-clock gap counts as a discontinuity rather
 *  than ordinary batch-to-batch jitter. Empty-drain alone is NOT the trigger
 *  (spec: "is only a heuristic") — this threshold is the actual signal. */
const GAP_SLACK_MULTIPLIER = 2;

/** Default canvas dimensions: `width` is the scroll-history depth in
 *  columns/px; `height` is the visible frequency-axis resolution — rows are
 *  nearest-neighbor-mapped onto the 512 bins, so any height renders. */
export const DEFAULT_WATERFALL_WIDTH = 300;
export const DEFAULT_WATERFALL_HEIGHT = 120;

/** Distinct gap-marker fill — must read as "something is missing here", not
 *  as more signal. Deliberately outside the intensity ramp's hue range. */
export const GAP_MARKER_COLOR = '#ff2fd6';

export interface WaterfallProps {
  /** Subscribe/paint only while true (spec §Waterfall lifecycle). */
  expanded: boolean;
  width?: number;
  height?: number;
}

// ---------------------------------------------------------------------------
// Pure gap detection — no canvas, no Tauri. Directly unit-testable.
// ---------------------------------------------------------------------------

/** Carries just enough state to detect the next batch's discontinuity. */
export interface WaterfallGapState {
  seq: number;
  /** Wall-clock UTC ms of the END of the last painted column. */
  lastColEndUtcMs: number;
}

/**
 * True when `batch` does not continue cleanly from `prev` — either the
 * monotonic `seq` skipped (missed batch) or the wall-clock gap between the
 * last painted column and this batch's first column exceeds the expected
 * column cadence by more than jitter slack. `prev === null` (first batch
 * ever, or the first batch after a fresh (re)subscribe) is never a gap —
 * there is nothing yet to be discontinuous FROM.
 */
export function detectGap(prev: WaterfallGapState | null, batch: WaterfallBatch): boolean {
  if (prev === null) return false;
  if (batch.seq !== prev.seq + 1) return true; // missed / reordered batch
  const actualGapMs = batch.firstColUtcMs - prev.lastColEndUtcMs;
  return actualGapMs > COLUMN_CADENCE_MS * GAP_SLACK_MULTIPLIER;
}

/** The gap state to carry forward after painting `batch`. */
export function nextGapState(batch: WaterfallBatch): WaterfallGapState {
  const lastColEndUtcMs = batch.firstColUtcMs + Math.max(0, batch.cols.length - 1) * COLUMN_CADENCE_MS;
  return { seq: batch.seq, lastColEndUtcMs };
}

// ---------------------------------------------------------------------------
// Colormap — u8 magnitude (0-255) -> RGB intensity ramp. Simple by design
// (spec: "a simple intensity ramp is fine; the backend sends normalized
// 0-255"); backend magnitude normalization is a separate parent-owned change
// this component does not depend on for its own correctness.
// ---------------------------------------------------------------------------

/** Dark-navy noise floor -> cyan -> white hot, in that order. Kept out of the
 *  gap marker's magenta hue on purpose (§Waterfall gap rendering). */
export function magnitudeToRgb(v: number): [number, number, number] {
  const t = Math.max(0, Math.min(255, v)) / 255;
  if (t < 0.5) {
    const s = t / 0.5;
    return [Math.round(4 + s * 8), Math.round(8 + s * 40), Math.round(24 + s * 90)];
  }
  const s = (t - 0.5) / 0.5;
  return [Math.round(12 + s * 243), Math.round(48 + s * 207), Math.round(114 + s * 141)];
}

/** Nearest-neighbor row->bin mapping so any canvas height renders the fixed
 *  512-bin column. Row 0 = highest frequency (3000 Hz), last row = 0 Hz —
 *  conventional waterfall orientation (low frequencies at the bottom). */
function binForRow(row: number, canvasHeight: number, binCount: number): number {
  const fromTop = canvasHeight <= 1 ? 0 : row / (canvasHeight - 1);
  const idx = Math.round((1 - fromTop) * (binCount - 1));
  return Math.max(0, Math.min(binCount - 1, idx));
}

// ---------------------------------------------------------------------------
// Imperative Canvas2D paint — exported for direct testing against a mocked
// 2D context (jsdom has no real canvas backend).
// ---------------------------------------------------------------------------

/** A minimal structural subset of `CanvasRenderingContext2D` — the seam a
 *  test can satisfy with a plain mock object instead of a real canvas. */
export interface WaterfallCtx {
  canvas: { width: number; height: number };
  createImageData(w: number, h: number): ImageData;
  putImageData(data: ImageData, dx: number, dy: number): void;
  drawImage(
    image: CanvasImageSource,
    sx: number,
    sy: number,
    sw: number,
    sh: number,
    dx: number,
    dy: number,
    dw: number,
    dh: number,
  ): void;
  fillStyle: string;
  fillRect(x: number, y: number, w: number, h: number): void;
}

/**
 * Paint exactly one leading-edge column: scroll the existing content 1px via
 * a self-copy `drawImage`, then write the new column at the freed edge — a
 * gap-marker fill (`col === null`) or a real magnitude column via
 * `putImageData`. Never joins a gap and a real column into the same paint —
 * callers wanting a gap marker call this once with `col: null` before
 * painting the batch's own columns (see `paintBatch`).
 */
export function paintColumn(
  ctx: WaterfallCtx,
  canvasWidth: number,
  canvasHeight: number,
  col: number[] | null,
): void {
  // Self-copy scroll: shift everything 1px toward the trailing edge. This is
  // the probe-validated WebKitGTK-safe approach — NOT a CSS transform.
  ctx.drawImage(
    ctx.canvas as unknown as CanvasImageSource,
    1,
    0,
    canvasWidth - 1,
    canvasHeight,
    0,
    0,
    canvasWidth - 1,
    canvasHeight,
  );

  if (col === null) {
    ctx.fillStyle = GAP_MARKER_COLOR;
    ctx.fillRect(canvasWidth - 1, 0, 1, canvasHeight);
    return;
  }

  const imageData = ctx.createImageData(1, canvasHeight);
  const data = imageData.data;
  for (let row = 0; row < canvasHeight; row += 1) {
    const bin = binForRow(row, canvasHeight, col.length);
    const [r, g, b] = magnitudeToRgb(col[bin] ?? 0);
    const o = row * 4;
    data[o] = r;
    data[o + 1] = g;
    data[o + 2] = b;
    data[o + 3] = 255;
  }
  ctx.putImageData(imageData, canvasWidth - 1, 0);
}

/**
 * Paint one incoming batch: an explicit gap-marker column FIRST when
 * `isGap`, then every column in `batch.cols`, oldest to newest — the
 * discontinuity is visually obvious on the strip, never silently
 * concatenated into a continuous-looking scroll.
 */
export function paintBatch(
  ctx: WaterfallCtx,
  canvasWidth: number,
  canvasHeight: number,
  batch: WaterfallBatch,
  isGap: boolean,
): void {
  if (isGap) {
    paintColumn(ctx, canvasWidth, canvasHeight, null);
  }
  for (const col of batch.cols) {
    paintColumn(ctx, canvasWidth, canvasHeight, col);
  }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function Waterfall({
  expanded,
  width = DEFAULT_WATERFALL_WIDTH,
  height = DEFAULT_WATERFALL_HEIGHT,
}: WaterfallProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const gapStateRef = useRef<WaterfallGapState | null>(null);

  useEffect(() => {
    if (!expanded) return undefined;

    // A fresh subscription makes no continuity claim against whatever was
    // painted before (prior expand, prior mount) — start gap-clean.
    gapStateRef.current = null;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    let subscriptionId: string | null = null;

    invoke<SubDto>(WATERFALL_SUBSCRIBE_COMMAND)
      .then((sub) => {
        if (disposed) {
          // Cleanup already ran before the subscribe resolved — the token
          // still landed on the backend, so unsubscribe it explicitly
          // rather than leaking a live FFT-thread token.
          invoke(WATERFALL_UNSUBSCRIBE_COMMAND, { subscriptionId: sub.subscriptionId }).catch(() => {});
          return;
        }
        subscriptionId = sub.subscriptionId;
      })
      .catch(() => {
        // jsdom / no-Tauri / command unavailable — no waterfall, no crash.
      });

    listen<WaterfallBatch>(FT8_WATERFALL_EVENT, (e) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const batch = e.payload;
      const isGap = detectGap(gapStateRef.current, batch);
      paintBatch(ctx as unknown as WaterfallCtx, canvas.width, canvas.height, batch, isGap);
      gapStateRef.current = nextGapState(batch);
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {});

    return () => {
      disposed = true;
      if (unlisten) unlisten();
      if (subscriptionId) {
        invoke(WATERFALL_UNSUBSCRIBE_COMMAND, { subscriptionId }).catch(() => {});
      }
    };
  }, [expanded]);

  return (
    <canvas
      ref={canvasRef}
      width={width}
      height={height}
      className="ft8-waterfall"
      data-testid="ft8-waterfall-canvas"
    />
  );
}
