// src/ft8ui/Waterfall.test.tsx
//
// Tests for Waterfall.tsx (Task C8, plan tuxlink-b026z.4 §Waterfall).
//
// @tauri-apps/api/{core,event} are mocked at module level (repo idiom, see
// useFt8Listener.test.ts): the `listen` mock captures the registered handler
// into an outer `let` for manual dispatch, and `invoke` is GATED ON `cmd` so
// vitest's stray no-arg cleanup call (feedback_vitest_invoke_mock_cleanup_call)
// is inert.
//
// jsdom has no real canvas backend, so `HTMLCanvasElement.prototype.getContext`
// is mocked to return a spy context object (drawImage/putImageData/
// createImageData/fillRect/fillStyle) per-canvas, keyed off the element so
// multiple renders in one test file don't cross-contaminate.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, act } from '@testing-library/react';
import {
  Waterfall,
  WATERFALL_SUBSCRIBE_COMMAND,
  WATERFALL_UNSUBSCRIBE_COMMAND,
  WATERFALL_BINS,
  GAP_MARKER_COLOR,
  COLUMN_CADENCE_MS,
  detectGap,
  nextGapState,
  paintColumn,
  paintBatch,
  magnitudeToRgb,
  type WaterfallCtx,
  type WaterfallGapState,
} from './Waterfall';
import type { WaterfallBatch } from './ft8Types';

// ---------------------------------------------------------------------------
// Tauri mocks
// ---------------------------------------------------------------------------

let waterfallHandler: ((e: { payload: WaterfallBatch }) => void) | null = null;
let unlistenCalls = 0;

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));

import { listen } from '@tauri-apps/api/event';
const listenMock = vi.mocked(listen);

function installCapturingListen() {
  listenMock.mockImplementation(((_event: string, handler: (e: { payload: WaterfallBatch }) => void) => {
    waterfallHandler = handler;
    return Promise.resolve(() => {
      unlistenCalls += 1;
      waterfallHandler = null;
    });
  }) as unknown as typeof listen);
}

/** Flush pending microtasks (invoke/listen resolution) inside act(). */
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

// ---------------------------------------------------------------------------
// Canvas 2D context mock
// ---------------------------------------------------------------------------

function makeCtxSpy() {
  return {
    drawImage: vi.fn(),
    putImageData: vi.fn(),
    createImageData: vi.fn((w: number, h: number) => ({
      width: w,
      height: h,
      data: new Uint8ClampedArray(w * h * 4),
      colorSpace: 'srgb' as const,
    })),
    fillRect: vi.fn(),
    fillStyle: '',
    canvas: { width: 0, height: 0 },
  };
}

let lastCtxSpy: ReturnType<typeof makeCtxSpy> | null = null;
// Real canvases return the SAME 2D context object on every `getContext('2d')`
// call — memoize per-element so the mock matches that (avoids a fresh,
// call-history-less spy on every paint).
const ctxByCanvas = new WeakMap<HTMLCanvasElement, ReturnType<typeof makeCtxSpy>>();

beforeEach(() => {
  waterfallHandler = null;
  unlistenCalls = 0;
  listenMock.mockReset();
  installCapturingListen();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd?: string) => {
    if (cmd === WATERFALL_SUBSCRIBE_COMMAND) return { subscriptionId: 'sub-1' };
    return undefined;
  });

  lastCtxSpy = null;
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(function (
    this: HTMLCanvasElement,
  ) {
    let spy = ctxByCanvas.get(this);
    if (!spy) {
      spy = makeCtxSpy();
      spy.canvas.width = this.width;
      spy.canvas.height = this.height;
      ctxByCanvas.set(this, spy);
    }
    lastCtxSpy = spy;
    return spy as unknown as CanvasRenderingContext2D;
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function mkCol(fill = 128): number[] {
  return new Array(WATERFALL_BINS).fill(fill);
}

function mkBatch(over: Partial<WaterfallBatch> = {}): WaterfallBatch {
  return {
    seq: 1,
    firstColUtcMs: 1_700_000_000_000,
    cols: [mkCol(), mkCol(), mkCol(), mkCol()],
    ...over,
  };
}

// ---------------------------------------------------------------------------
// (a) subscribe-on-expand / unsubscribe-on-collapse / unmount
// ---------------------------------------------------------------------------

describe('Waterfall — subscription lifecycle', () => {
  it('subscribes (invoke + listen) when rendered expanded', async () => {
    const { unmount } = render(<Waterfall expanded />);
    await flush();

    expect(invokeMock).toHaveBeenCalledWith(WATERFALL_SUBSCRIBE_COMMAND, undefined);
    expect(listenMock).toHaveBeenCalledWith('ft8-waterfall:columns', expect.any(Function));
    unmount();
  });

  it('unsubscribes (invoke + unlisten) on collapse (expanded -> false while still mounted)', async () => {
    const { rerender } = render(<Waterfall expanded />);
    await flush();
    invokeMock.mockClear();

    rerender(<Waterfall expanded={false} />);
    await flush();

    expect(unlistenCalls).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith(WATERFALL_UNSUBSCRIBE_COMMAND, { subscriptionId: 'sub-1' });
  });

  it('unsubscribes (invoke + unlisten) on unmount', async () => {
    const { unmount } = render(<Waterfall expanded />);
    await flush();
    invokeMock.mockClear();

    unmount();
    await flush();

    expect(unlistenCalls).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith(WATERFALL_UNSUBSCRIBE_COMMAND, { subscriptionId: 'sub-1' });
  });

  it('does NOT subscribe while collapsed', async () => {
    const { unmount } = render(<Waterfall expanded={false} />);
    await flush();

    expect(invokeMock).not.toHaveBeenCalledWith(WATERFALL_SUBSCRIBE_COMMAND, expect.anything());
    expect(listenMock).not.toHaveBeenCalled();
    unmount();
  });

  it('does not leak a subscription when unmounted BEFORE the subscribe/listen promises resolve (disposed-before-resolved race)', async () => {
    // Render then unmount SYNCHRONOUSLY — no flush between — so the effect's
    // cleanup runs while `subscriptionId`/`unlisten` are still null (their
    // promises are pending). The post-dispose paths in the `.then` callbacks
    // are the only thing that can prevent a leaked FFT-thread token here.
    const { unmount } = render(<Waterfall expanded />);
    unmount();

    // Now let the pending subscribe + listen promises resolve, post-dispose.
    await flush();

    // The subscribe resolved after dispose → its own `.then` unsubscribes the
    // token rather than leaking it; the listen resolved after dispose → its
    // `.then` immediately unlistens.
    expect(invokeMock).toHaveBeenCalledWith(WATERFALL_UNSUBSCRIBE_COMMAND, { subscriptionId: 'sub-1' });
    expect(unlistenCalls).toBe(1);
  });

  it('re-subscribes with a fresh listen registration on a collapse -> re-expand cycle', async () => {
    const { rerender, unmount } = render(<Waterfall expanded />);
    await flush();
    expect(listenMock).toHaveBeenCalledTimes(1);

    rerender(<Waterfall expanded={false} />);
    await flush();

    rerender(<Waterfall expanded />);
    await flush();

    expect(listenMock).toHaveBeenCalledTimes(2);
    unmount();
  });
});

// ---------------------------------------------------------------------------
// (b) column-paint unit — synthetic batch -> putImageData/drawImage shape
// ---------------------------------------------------------------------------

describe('Waterfall — column paint', () => {
  it('paints an incoming batch: one drawImage self-copy + one putImageData per column, as rows', async () => {
    render(<Waterfall expanded width={300} height={120} />);
    await flush();

    act(() => {
      waterfallHandler?.({ payload: mkBatch({ cols: [mkCol(), mkCol()] }) });
    });
    const ctx = lastCtxSpy!;

    expect(ctx.drawImage).toHaveBeenCalledTimes(2); // one self-copy scroll per column
    expect(ctx.drawImage).toHaveBeenCalledWith(
      ctx.canvas,
      0,
      0,
      300,
      119, // canvasHeight - 1
      0,
      1,
      300,
      119,
    );
    expect(ctx.putImageData).toHaveBeenCalledTimes(2);
    expect(ctx.createImageData).toHaveBeenCalledWith(300, 1); // full-width, 1px-tall leading row
    // Written at the leading (top) edge.
    expect(ctx.putImageData).toHaveBeenCalledWith(expect.anything(), 0, 0);
  });

  it('maps u8 magnitude into RGBA pixel data (opaque, non-degenerate ramp)', () => {
    const [rLow, gLow, bLow] = magnitudeToRgb(0);
    const [rHigh, gHigh, bHigh] = magnitudeToRgb(255);
    // A real intensity ramp: the top of scale is strictly brighter than the
    // bottom on at least one channel (not a flat/constant color).
    expect(rHigh + gHigh + bHigh).toBeGreaterThan(rLow + gLow + bLow);
  });

  it('paintColumn writes a fully-opaque (alpha=255) row via a directly-mocked ctx', () => {
    const ctx: WaterfallCtx = {
      canvas: { width: 10, height: 4 },
      createImageData: (w, h) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
        colorSpace: 'srgb',
      }),
      putImageData: vi.fn(),
      drawImage: vi.fn(),
      fillStyle: '',
      fillRect: vi.fn(),
    };

    paintColumn(ctx, 10, 4, mkCol(200));

    const [imageData] = (ctx.putImageData as ReturnType<typeof vi.fn>).mock.calls[0];
    for (let col = 0; col < 10; col += 1) {
      expect(imageData.data[col * 4 + 3]).toBe(255); // alpha channel
    }
  });
});

// ---------------------------------------------------------------------------
// (c) gap marker on discontinuity
// ---------------------------------------------------------------------------

describe('Waterfall — gap detection (pure)', () => {
  it('is never a gap for the first batch (no prior state)', () => {
    expect(detectGap(null, mkBatch())).toBe(false);
  });

  it('is a gap when seq skips ahead', () => {
    const prev: WaterfallGapState = { seq: 1, expectedNextColUtcMs: 1_700_000_001_000 };
    expect(detectGap(prev, mkBatch({ seq: 3, firstColUtcMs: 1_700_000_001_000 }))).toBe(true);
  });

  it('is NOT a gap for a consecutive seq batch arriving exactly on time (0ms deviation)', () => {
    const prevBatch = mkBatch({ seq: 1, firstColUtcMs: 1_700_000_000_000 });
    const prev = nextGapState(prevBatch);
    // An on-time next batch begins exactly at the expected first-column time.
    const next = mkBatch({ seq: 2, firstColUtcMs: prev.expectedNextColUtcMs });
    expect(detectGap(prev, next)).toBe(false);
  });

  it('tolerates jitter up to (but not exceeding) the 2× cadence slack', () => {
    const prev = nextGapState(mkBatch({ seq: 1, firstColUtcMs: 1_700_000_000_000 }));
    // 2× cadence (500ms) late is at the threshold — NOT a gap (strict >).
    const atThreshold = mkBatch({
      seq: 2,
      firstColUtcMs: prev.expectedNextColUtcMs + COLUMN_CADENCE_MS * 2,
    });
    expect(detectGap(prev, atThreshold)).toBe(false);
    // One ms past the 2× slack IS a gap — proves a real, documented 2× headroom.
    const pastThreshold = mkBatch({
      seq: 2,
      firstColUtcMs: prev.expectedNextColUtcMs + COLUMN_CADENCE_MS * 2 + 1,
    });
    expect(detectGap(prev, pastThreshold)).toBe(true);
  });

  it('is a gap when the wall-clock delta far exceeds cadence + slack even with consecutive seq', () => {
    const prevBatch = mkBatch({ seq: 1, firstColUtcMs: 1_700_000_000_000 });
    const prev = nextGapState(prevBatch);
    const next = mkBatch({ seq: 2, firstColUtcMs: prev.expectedNextColUtcMs + 10_000 }); // 10s stall
    expect(detectGap(prev, next)).toBe(true);
  });
});

describe('Waterfall — gap marker paint', () => {
  it('paintBatch renders a distinct gap-marker ROW (fillRect + gap color), full canvas width, BEFORE the batch columns when isGap', () => {
    const calls: string[] = [];
    const ctx: WaterfallCtx = {
      canvas: { width: 10, height: 4 },
      createImageData: (w, h) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
        colorSpace: 'srgb',
      }),
      putImageData: vi.fn(() => calls.push('putImageData')),
      drawImage: vi.fn(() => calls.push('drawImage')),
      fillStyle: '',
      fillRect: vi.fn(() => {
        calls.push('fillRect');
      }),
    };

    paintBatch(ctx, 10, 4, mkBatch({ cols: [mkCol()] }), true);

    expect(ctx.fillRect).toHaveBeenCalledTimes(1);
    expect(ctx.fillStyle).toBe(GAP_MARKER_COLOR);
    // A full-width, 1px-tall row at the top — a HORIZONTAL gap marker, not a
    // vertical column (the pre-rotation shape).
    expect(ctx.fillRect).toHaveBeenCalledWith(0, 0, 10, 1);
    // Order: drawImage(scroll) -> fillRect(gap marker) -> drawImage(scroll) -> putImageData(real row).
    expect(calls).toEqual(['drawImage', 'fillRect', 'drawImage', 'putImageData']);
  });

  it('paintBatch renders NO gap marker when isGap is false', () => {
    const ctx: WaterfallCtx = {
      canvas: { width: 10, height: 4 },
      createImageData: (w, h) => ({
        width: w,
        height: h,
        data: new Uint8ClampedArray(w * h * 4),
        colorSpace: 'srgb',
      }),
      putImageData: vi.fn(),
      drawImage: vi.fn(),
      fillStyle: '',
      fillRect: vi.fn(),
    };

    paintBatch(ctx, 10, 4, mkBatch({ cols: [mkCol(), mkCol()] }), false);

    expect(ctx.fillRect).not.toHaveBeenCalled();
    expect(ctx.putImageData).toHaveBeenCalledTimes(2);
  });

  it('end-to-end: a live batch arriving after a seq jump paints a gap marker on the real component', async () => {
    render(<Waterfall expanded width={10} height={4} />);
    await flush();

    act(() => {
      waterfallHandler?.({ payload: mkBatch({ seq: 1, firstColUtcMs: 1_700_000_000_000, cols: [mkCol()] }) });
    });
    // getContext is acquired lazily on first paint (memoized-per-canvas mock,
    // matching real canvas behavior) — capture it after that first dispatch.
    const ctx = lastCtxSpy!;
    ctx.fillRect.mockClear();

    // Jump seq from 1 -> 5: a clear missed-batch discontinuity.
    act(() => {
      waterfallHandler?.({ payload: mkBatch({ seq: 5, firstColUtcMs: 1_700_000_100_000, cols: [mkCol()] }) });
    });

    expect(ctx.fillRect).toHaveBeenCalledTimes(1);
    expect(ctx.fillStyle).toBe(GAP_MARKER_COLOR);
  });
});
