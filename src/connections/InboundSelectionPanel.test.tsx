// src/connections/InboundSelectionPanel.test.tsx
//
// (a) Panel render tests for the inline pending-message selection panel
// (tuxlink-bsiy). Pure-presentation: props in, onSubmit/onClose out. The
// wire contract is exercised end-to-end by the production-mount test in
// useInboundSelection.test.tsx.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { InboundSelectionPanel } from './InboundSelectionPanel';
import type { PendingProposalDto } from './sessionTypes';

// Raw CSS for the stacking-order invariant (tuxlink-76zy). The selection modal
// must paint above the app chrome or its header controls become unreachable.
const CSS_RAW = import.meta.glob(
  ['./InboundSelectionPanel.css', '../shell/chrome/chrome.css'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;
const inboundCss = CSS_RAW['./InboundSelectionPanel.css'];
const chromeCss = CSS_RAW['../shell/chrome/chrome.css'];

const maxZIndex = (css: string): number =>
  Math.max(...[...css.matchAll(/z-index:\s*(\d+)/g)].map((m) => Number(m[1])));
const overlayZIndex = (css: string): number => {
  const start = css.indexOf('.inbound-selection-overlay');
  const block = css.slice(start, css.indexOf('}', start));
  return Number(block.match(/z-index:\s*(\d+)/)?.[1]);
};

// Distinct byte values so every formatSize() result is unique — keeps the
// getByText size assertions unambiguous.
const PROPOSALS: PendingProposalDto[] = [
  { mid: 'AAA111BBB222', uncompressed_size: 2048, compressed_size: 1024 }, // 2.0 KB / 1.0 KB
  { mid: 'CCC333DDD444', uncompressed_size: 8192, compressed_size: 4096 }, // 8.0 KB / 4.0 KB
  { mid: 'EEE555FFF666', uncompressed_size: 512, compressed_size: 256 },   // 512 B / 256 B
];

describe('InboundSelectionPanel', () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it('renders one row per proposal with MID and both sizes (formatSize-d)', () => {
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    for (const p of PROPOSALS) {
      expect(screen.getByText(p.mid)).toBeInTheDocument();
    }
    expect(screen.getByText('2.0 KB')).toBeInTheDocument(); // uncompressed of row 1
    expect(screen.getByText('1.0 KB')).toBeInTheDocument(); // compressed of row 1
    expect(screen.getByText('8.0 KB')).toBeInTheDocument(); // uncompressed of row 2
    expect(screen.getByText('4.0 KB')).toBeInTheDocument(); // compressed of row 2
    expect(screen.getByText('512 B')).toBeInTheDocument(); // uncompressed of row 3
    expect(screen.getByText('256 B')).toBeInTheDocument(); // compressed of row 3
  });

  it('pre-checks every row on open', () => {
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    const rowChecks = screen.getAllByRole('checkbox', { name: /select message/i });
    expect(rowChecks).toHaveLength(PROPOSALS.length);
    for (const cb of rowChecks) expect(cb).toBeChecked();
  });

  it('defaults the disposition radio to Hold', () => {
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByRole('radio', { name: /hold/i })).toBeChecked();
    expect(screen.getByRole('radio', { name: /delete/i })).not.toBeChecked();
  });

  it('footer button reads "Download N Checked" with the right count', () => {
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByRole('button', { name: /download 3 checked/i })).toBeInTheDocument();
  });

  it('Deselect All clears every row; Select All re-checks them', () => {
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole('button', { name: /deselect all/i }));
    let rowChecks = screen.getAllByRole('checkbox', { name: /select message/i });
    for (const cb of rowChecks) expect(cb).not.toBeChecked();
    expect(screen.getByRole('button', { name: /download 0 checked/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /^select all$/i }));
    rowChecks = screen.getAllByRole('checkbox', { name: /select message/i });
    for (const cb of rowChecks) expect(cb).toBeChecked();
    expect(screen.getByRole('button', { name: /download 3 checked/i })).toBeInTheDocument();
  });

  it('submits all mids with disposition hold by default', () => {
    const onSubmit = vi.fn();
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={onSubmit} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole('button', { name: /download 3 checked/i }));
    expect(onSubmit).toHaveBeenCalledTimes(1);
    const arg = onSubmit.mock.calls[0][0];
    expect(arg.disposition).toBe('hold');
    expect([...arg.selected_mids].sort()).toEqual(PROPOSALS.map((p) => p.mid).sort());
  });

  it('submits disposition delete after selecting the Delete radio', () => {
    const onSubmit = vi.fn();
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={onSubmit} onClose={vi.fn()} />,
    );
    fireEvent.click(screen.getByRole('radio', { name: /delete/i }));
    fireEvent.click(screen.getByRole('button', { name: /download 3 checked/i }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ disposition: 'delete' }),
    );
  });

  it('submits a reduced selected_mids after unchecking a row', () => {
    const onSubmit = vi.fn();
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={onSubmit} onClose={vi.fn()} />,
    );
    // Uncheck the first row.
    const rowChecks = screen.getAllByRole('checkbox', { name: /select message/i });
    fireEvent.click(rowChecks[0]);
    expect(screen.getByRole('button', { name: /download 2 checked/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /download 2 checked/i }));
    const arg = onSubmit.mock.calls[0][0];
    expect(arg.selected_mids).not.toContain(PROPOSALS[0].mid);
    expect([...arg.selected_mids].sort()).toEqual(
      [PROPOSALS[1].mid, PROPOSALS[2].mid].sort(),
    );
  });

  it('ESC calls onClose', () => {
    const onClose = vi.fn();
    render(
      <InboundSelectionPanel proposals={PROPOSALS} onSubmit={vi.fn()} onClose={onClose} />,
    );
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('cosmetic countdown auto-submits the current checkbox state at zero', () => {
    vi.useFakeTimers();
    const onSubmit = vi.fn();
    try {
      render(
        <InboundSelectionPanel
          proposals={PROPOSALS}
          onSubmit={onSubmit}
          onClose={vi.fn()}
          countdownSeconds={3}
        />,
      );
      // Advance past the countdown — wrap in act so React flushes the
      // per-second state updates between timer fires (chained setTimeout).
      act(() => {
        vi.advanceTimersByTime(3500);
      });
    } finally {
      vi.useRealTimers();
    }
    expect(onSubmit).toHaveBeenCalledTimes(1);
    // Auto-submit carries the current (all pre-checked) selection + Hold.
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ disposition: 'hold' }),
    );
  });

  it('auto-submits exactly once even if the timer keeps ticking past zero', () => {
    vi.useFakeTimers();
    const onSubmit = vi.fn();
    try {
      render(
        <InboundSelectionPanel
          proposals={PROPOSALS}
          onSubmit={onSubmit}
          onClose={vi.fn()}
          countdownSeconds={2}
        />,
      );
      // Advance WELL past zero so a buggy interval would re-fire submit on
      // each subsequent tick (the autoSubmitted-guard regression).
      act(() => {
        vi.advanceTimersByTime(10_000);
      });
    } finally {
      vi.useRealTimers();
    }
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });
});

describe('InboundSelectionPanel.css stacking order (tuxlink-76zy)', () => {
  it('overlay stacks above every app-chrome z-index', () => {
    expect(overlayZIndex(inboundCss)).toBeGreaterThan(maxZIndex(chromeCss));
  });
});
