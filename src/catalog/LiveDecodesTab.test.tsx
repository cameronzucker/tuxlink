import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LiveDecodesTab, aggregateLiveDecodes } from './LiveDecodesTab';
import type { DecodeDto, SlotRecord } from '../ft8ui/ft8Types';

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

describe('aggregateLiveDecodes', () => {
  it('aggregates by callsign: count, best SNR, and last-heard slot', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 30_000, [mkDecode({ fromCall: 'W7GTE', snrDb: -12, slotUtcMs: NOW - 30_000 })]),
      mkSlot(NOW - 15_000, [mkDecode({ fromCall: 'W7GTE', snrDb: -4, slotUtcMs: NOW - 15_000 })]),
    ];
    const rows = aggregateLiveDecodes(ring, NOW);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ call: 'W7GTE', count: 2, bestSnrDb: -4, lastSlotUtcMs: NOW - 15_000 });
  });

  it('sorts by recency, most recently heard first', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 60_000, [mkDecode({ fromCall: 'K5MDX', slotUtcMs: NOW - 60_000 })]),
      mkSlot(NOW - 10_000, [mkDecode({ fromCall: 'W7GTE', slotUtcMs: NOW - 10_000 })]),
    ];
    const rows = aggregateLiveDecodes(ring, NOW);
    expect(rows.map((r) => r.call)).toEqual(['W7GTE', 'K5MDX']);
  });

  it('drops decodes older than the 10-minute window', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 700_000, [mkDecode({ fromCall: 'OLDCALL', slotUtcMs: NOW - 700_000 })]),
    ];
    expect(aggregateLiveDecodes(ring, NOW)).toHaveLength(0);
  });

  it('skips decodes with no attributable callsign (fromCall null)', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ fromCall: null })])];
    expect(aggregateLiveDecodes(ring, NOW)).toHaveLength(0);
  });

  it('skips band-dead / non-decoded slots (no decode payloads to aggregate)', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [], { outcome: { kind: 'band-dead' } })];
    expect(aggregateLiveDecodes(ring, NOW)).toHaveLength(0);
  });

  it('a later CQ carrying the grid upgrades the row in place (does not clobber with null)', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 30_000, [mkDecode({ fromCall: 'W7GTE', grid: null, slotUtcMs: NOW - 30_000 })]),
      mkSlot(NOW - 15_000, [mkDecode({ fromCall: 'W7GTE', grid: 'DM34oa', slotUtcMs: NOW - 15_000 })]),
      // A later ACK-style decode with no grid must not blank the grid already learned.
      mkSlot(NOW, [mkDecode({ fromCall: 'W7GTE', grid: null, message: 'W7GTE N0DAJ R-04', slotUtcMs: NOW })]),
    ];
    const rows = aggregateLiveDecodes(ring, NOW);
    expect(rows[0]).toMatchObject({ call: 'W7GTE', grid: 'DM34oa' });
  });
});

describe('LiveDecodesTab', () => {
  it('shows the empty state when no decodes are in the window', () => {
    render(<LiveDecodesTab decodesRing={[]} operatorGrid="DM43bp" nowMs={NOW} />);
    expect(screen.getByTestId('live-decodes-empty')).toBeTruthy();
  });

  it('a grid-less row is non-interactive and shows "—" for grid and mi·brg', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ fromCall: 'K5MDX', grid: null, slotUtcMs: NOW })])];
    const onPanTo = vi.fn();
    render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" onPanTo={onPanTo} nowMs={NOW} />);
    const row = screen.getByTestId('ld-row-K5MDX');
    expect(row.className).not.toMatch(/is-clickable/);
    expect(row.getAttribute('role')).toBeNull();
    expect(row.textContent).toContain('—'); // grid column
    fireEvent.click(row);
    expect(onPanTo).not.toHaveBeenCalled();
  });

  it('row click pans the map via the null-guarded gridToLatLon for a valid grid', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ fromCall: 'W7GTE', grid: 'DM34oa', slotUtcMs: NOW })])];
    const onPanTo = vi.fn();
    render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" onPanTo={onPanTo} nowMs={NOW} />);
    const row = screen.getByTestId('ld-row-W7GTE');
    expect(row.className).toMatch(/is-clickable/);
    fireEvent.click(row);
    expect(onPanTo).toHaveBeenCalledTimes(1);
    const ll = onPanTo.mock.calls[0][0];
    expect(typeof ll.lat).toBe('number');
    expect(typeof ll.lon).toBe('number');
  });

  it('untrusted-input hardening: a malformed/garbage grid never pans and never throws', () => {
    // Radio input is untrusted — a "grid" string can be anything the parser
    // let through: wrong length, markup-shaped text, whatever. The row is
    // still interactive (a non-null grid string WAS heard), but the click
    // must no-op rather than pan or crash.
    const ring: SlotRecord[] = [
      mkSlot(NOW, [mkDecode({ fromCall: 'GARBAGE1', grid: '<img src=x onerror=alert(1)>', slotUtcMs: NOW })]),
    ];
    const onPanTo = vi.fn();
    expect(() => {
      render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" onPanTo={onPanTo} nowMs={NOW} />);
    }).not.toThrow();
    const row = screen.getByTestId('ld-row-GARBAGE1');
    // React-escaped text node, not injected markup: no <img> element exists.
    expect(row.querySelector('img')).toBeNull();
    expect(row.textContent).toContain('<img src=x onerror=alert(1)>');
    expect(() => fireEvent.click(row)).not.toThrow();
    expect(onPanTo).not.toHaveBeenCalled();
  });

  it('untrusted-input hardening: a too-short grid (e.g. a mis-parsed SNR report) never pans', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ fromCall: 'K5MDX', grid: 'R73', slotUtcMs: NOW })])];
    const onPanTo = vi.fn();
    render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" onPanTo={onPanTo} nowMs={NOW} />);
    fireEvent.click(screen.getByTestId('ld-row-K5MDX'));
    expect(onPanTo).not.toHaveBeenCalled();
  });

  it('renders call, band tag, best SNR, and count columns', () => {
    const ring: SlotRecord[] = [
      mkSlot(NOW - 15_000, [mkDecode({ fromCall: 'W7GTE', snrDb: -12, slotUtcMs: NOW - 15_000 })], { band: '40m' }),
      mkSlot(NOW, [mkDecode({ fromCall: 'W7GTE', snrDb: -4, slotUtcMs: NOW })], { band: '20m' }),
    ];
    render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" nowMs={NOW} />);
    const row = screen.getByTestId('ld-row-W7GTE');
    expect(row.textContent).toContain('W7GTE');
    expect(row.textContent).toContain('20m'); // most-recent decode's band tag
    expect(row.textContent).toContain('-4 dB'); // best (least negative) SNR
    expect(row.textContent).toContain('2'); // count
  });

  it('is keyboard-activatable (Enter) for an interactive row', () => {
    const ring: SlotRecord[] = [mkSlot(NOW, [mkDecode({ fromCall: 'W7GTE', grid: 'DM34oa', slotUtcMs: NOW })])];
    const onPanTo = vi.fn();
    render(<LiveDecodesTab decodesRing={ring} operatorGrid="DM43bp" onPanTo={onPanTo} nowMs={NOW} />);
    fireEvent.keyDown(screen.getByTestId('ld-row-W7GTE'), { key: 'Enter' });
    expect(onPanTo).toHaveBeenCalledTimes(1);
  });
});
