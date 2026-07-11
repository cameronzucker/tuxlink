import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DecodeFeed, flattenDecodeFeed, DECODE_FEED_CAP } from './DecodeFeed';
import type { DecodeDto, SlotRecord } from './ft8Types';

const NOW = 1_000_000_000; // arbitrary epoch ms anchor

function mkDecode(over: Partial<DecodeDto> = {}): DecodeDto {
  return {
    slotUtcMs: NOW,
    snrDb: -10,
    dtS: 0,
    freqHz: 1500,
    message: 'CQ N0CALL EM12',
    fromCall: 'N0CALL',
    toCall: null,
    grid: null,
    partial: false,
    ...over,
  };
}

function mkSlot(slotUtcMs: number, decodes: DecodeDto[], over: Partial<SlotRecord> = {}): SlotRecord {
  return {
    slotUtcMs,
    band: '20m',
    dialHz: 14074000,
    bandSource: 'cat-confirmed',
    bandLabelConfirmedUtcMs: null,
    outcome: decodes.length ? { kind: 'decoded' } : { kind: 'band-dead' },
    decodes,
    partialSalvage: false,
    lostFrames: 0,
    boundarySkewFrames: 0,
    clipFraction: 0,
    rmsDbfs: -20,
    dwellSlotIndex: null,
    ...over,
  };
}

describe('flattenDecodeFeed', () => {
  it('flattens decoded slots into per-decode rows, newest first', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 30_000, [mkDecode({ fromCall: 'OLD', slotUtcMs: NOW - 30_000 })]),
      mkSlot(NOW - 15_000, [mkDecode({ fromCall: 'NEW', slotUtcMs: NOW - 15_000 })]),
    ];
    const rows = flattenDecodeFeed(ring);
    expect(rows.map((r) => r.message)).toEqual(['CQ N0CALL EM12', 'CQ N0CALL EM12']);
    expect(rows[0].slotUtcMs).toBe(NOW - 15_000); // newest first
    expect(rows[1].slotUtcMs).toBe(NOW - 30_000);
  });

  it('skips non-decoded outcomes (band-dead / failed / dropped / discarded never contribute rows)', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW, [], { outcome: { kind: 'band-dead' } }),
      mkSlot(NOW + 1000, [], { outcome: { kind: 'failed', failure: 'x' } }),
      mkSlot(NOW + 2000, [], { outcome: { kind: 'dropped-backpressure' } }),
      mkSlot(NOW + 3000, [], { outcome: { kind: 'discarded', class: 'qsy-transition' } }),
      mkSlot(NOW + 4000, [mkDecode({ slotUtcMs: NOW + 4000 })]),
    ];
    const rows = flattenDecodeFeed(ring);
    expect(rows).toHaveLength(1);
  });

  it('hard-caps the flattened feed at DECODE_FEED_CAP even when fed far more decodes', () => {
    const ring: SlotRecord[] = Array.from({ length: 300 }, (_, i) =>
      mkSlot(NOW + i * 1000, [mkDecode({ slotUtcMs: NOW + i * 1000, fromCall: `N0CALL${i}` })]),
    );
    const rows = flattenDecodeFeed(ring);
    expect(rows.length).toBe(DECODE_FEED_CAP);
    // Newest-first + capped: the retained rows are the 200 MOST RECENT, not
    // an arbitrary or oldest-biased slice.
    expect(rows[0].slotUtcMs).toBe(NOW + 299 * 1000);
    expect(rows[rows.length - 1].slotUtcMs).toBe(NOW + 100 * 1000);
  });

  it('keeps same-slot decodes in their original in-slot order (stable sort)', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW, [
        mkDecode({ fromCall: 'FIRST', freqHz: 500, slotUtcMs: NOW }),
        mkDecode({ fromCall: 'SECOND', freqHz: 1500, slotUtcMs: NOW }),
      ]),
    ];
    const rows = flattenDecodeFeed(ring);
    expect(rows.map((r) => r.freqHz)).toEqual([500, 1500]);
  });
});

describe('DecodeFeed component', () => {
  it('renders an empty state when the ring carries no evidence', () => {
    render(<DecodeFeed decodesRing={[]} />);
    expect(screen.getByTestId('decode-feed-empty')).toBeInTheDocument();
  });

  it('renders at most DECODE_FEED_CAP rows when fed 300 decodes — the feed is never unbounded', () => {
    const ring: SlotRecord[] = Array.from({ length: 300 }, (_, i) =>
      mkSlot(NOW + i * 1000, [mkDecode({ slotUtcMs: NOW + i * 1000, fromCall: `N0CALL${i}` })]),
    );
    render(<DecodeFeed decodesRing={ring} />);
    const container = screen.getByTestId('decode-feed');
    const rowEls = container.querySelectorAll('tbody tr');
    expect(rowEls.length).toBe(DECODE_FEED_CAP);
    expect(rowEls.length).toBeLessThanOrEqual(200);
  });

  it('untrusted-input hardening: a hostile decode message renders as escaped text — no <img> element, no throw', () => {
    const hostile = '<img src=x onerror=alert(1)>';
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ message: hostile, fromCall: 'HOSTILE', slotUtcMs: NOW })])];
    expect(() => render(<DecodeFeed decodesRing={ring} />)).not.toThrow();
    const container = screen.getByTestId('decode-feed');
    // React-escaped text node, not injected markup: no <img> element exists.
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent).toContain(hostile);
  });

  it('untrusted-input hardening: a hostile callsign/grid embedded in the message never throws or injects markup', () => {
    const hostile = 'CQ <script>alert(1)</script> DM43';
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ message: hostile, fromCall: '<b>X</b>', slotUtcMs: NOW })])];
    expect(() => render(<DecodeFeed decodesRing={ring} />)).not.toThrow();
    const container = screen.getByTestId('decode-feed');
    expect(container.querySelector('script')).toBeNull();
    expect(container.querySelector('b')).toBeNull();
    expect(container.textContent).toContain(hostile);
  });

  it('sanitized/stable keys: identical hostile decode text at two different slot times renders without a duplicate-key warning', () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const hostile = '<img src=x onerror=alert(1)>';
    const ring: SlotRecord[] = [
      mkSlot(NOW, [mkDecode({ message: hostile, slotUtcMs: NOW })]),
      mkSlot(NOW + 15_000, [mkDecode({ message: hostile, slotUtcMs: NOW + 15_000 })]),
    ];
    render(<DecodeFeed decodesRing={ring} />);
    const dupKeyWarning = errSpy.mock.calls.some((args) =>
      args.some((a) => typeof a === 'string' && a.includes('same key')),
    );
    expect(dupKeyWarning).toBe(false);
    errSpy.mockRestore();
  });

  it('renders UTC / dB / Freq / Message columns for a normal decode', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW, [mkDecode({ message: 'CQ W7GTE DM34', snrDb: -4, freqHz: 1240, slotUtcMs: NOW })]),
    ];
    render(<DecodeFeed decodesRing={ring} />);
    const row = screen.getByTestId(`decode-feed-row-${NOW}-0`);
    expect(row.textContent).toContain('CQ W7GTE DM34');
    expect(row.textContent).toContain('1240');
    expect(row.textContent).toContain('-04');
  });
});
