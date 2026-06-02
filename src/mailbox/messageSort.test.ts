import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { MessageMeta } from './types';
import {
  type SortKey,
  type SortState,
  DEFAULT_SORT_STATE,
  SORT_KEY_OPTIONS,
  DIRECTION_LABELS,
  SORT_STATE_STORAGE_KEY,
  isSortKey,
  isSortDirection,
  isSortState,
  loadSortState,
  saveSortState,
  compareMessages,
  sortMessages,
} from './messageSort';

function meta(over: Partial<MessageMeta> = {}): MessageMeta {
  return {
    id: 'MID1',
    subject: 'Hello',
    from: 'KK4XYZ',
    to: [],
    date: '2026-05-19T14:05:00Z',
    unread: false,
    bodySize: 1024,
    hasAttachments: false,
    ...over,
  };
}

describe('SORT_KEY_OPTIONS', () => {
  it('lists five keys in popup order (date, sender, recipient, subject, size)', () => {
    expect(SORT_KEY_OPTIONS.map((o) => o.id)).toEqual([
      'date',
      'sender',
      'recipient',
      'subject',
      'size',
    ]);
  });

  it('default matches the backend baseline (newest first)', () => {
    expect(DEFAULT_SORT_STATE).toEqual({ key: 'date', direction: 'desc' });
  });

  it('every option has a non-empty label', () => {
    for (const opt of SORT_KEY_OPTIONS) {
      expect(opt.label.length).toBeGreaterThan(0);
    }
  });
});

describe('DIRECTION_LABELS', () => {
  it('covers every SortKey', () => {
    for (const opt of SORT_KEY_OPTIONS) {
      expect(DIRECTION_LABELS[opt.id]).toBeDefined();
      expect(DIRECTION_LABELS[opt.id].desc.length).toBeGreaterThan(0);
      expect(DIRECTION_LABELS[opt.id].asc.length).toBeGreaterThan(0);
    }
  });

  it('date uses time-anchored phrasing (Newest/Oldest)', () => {
    expect(DIRECTION_LABELS.date).toEqual({ desc: 'Newest first', asc: 'Oldest first' });
  });

  it('size uses magnitude-anchored phrasing (Largest/Smallest)', () => {
    expect(DIRECTION_LABELS.size).toEqual({ desc: 'Largest first', asc: 'Smallest first' });
  });

  it('lexicographic keys use A→Z phrasing', () => {
    for (const k of ['sender', 'recipient', 'subject'] as const) {
      expect(DIRECTION_LABELS[k].asc).toContain('A');
      expect(DIRECTION_LABELS[k].desc).toContain('Z');
    }
  });
});

describe('type guards', () => {
  it('isSortKey accepts every defined key', () => {
    for (const opt of SORT_KEY_OPTIONS) {
      expect(isSortKey(opt.id)).toBe(true);
    }
  });
  it('isSortKey rejects unknown values', () => {
    expect(isSortKey('garbage')).toBe(false);
    expect(isSortKey(null)).toBe(false);
    expect(isSortKey(undefined)).toBe(false);
    expect(isSortKey(42)).toBe(false);
  });
  it('isSortDirection accepts only asc/desc', () => {
    expect(isSortDirection('asc')).toBe(true);
    expect(isSortDirection('desc')).toBe(true);
    expect(isSortDirection('newest')).toBe(false);
    expect(isSortDirection(null)).toBe(false);
  });
  it('isSortState requires both fields valid', () => {
    expect(isSortState({ key: 'date', direction: 'desc' })).toBe(true);
    expect(isSortState({ key: 'size', direction: 'asc' })).toBe(true);
    expect(isSortState({ key: 'garbage', direction: 'desc' })).toBe(false);
    expect(isSortState({ key: 'date', direction: 'newest' })).toBe(false);
    expect(isSortState({ key: 'date' })).toBe(false);
    expect(isSortState(null)).toBe(false);
    expect(isSortState('date-desc')).toBe(false);
  });
});

describe('localStorage round-trip', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('loadSortState returns default when storage is empty', () => {
    expect(loadSortState()).toEqual(DEFAULT_SORT_STATE);
  });

  it('loadSortState returns default for non-JSON garbage', () => {
    localStorage.setItem(SORT_STATE_STORAGE_KEY, 'not-json-at-all');
    expect(loadSortState()).toEqual(DEFAULT_SORT_STATE);
  });

  it('loadSortState returns default for shape-wrong JSON', () => {
    localStorage.setItem(SORT_STATE_STORAGE_KEY, JSON.stringify({ key: 'garbage', direction: 'asc' }));
    expect(loadSortState()).toEqual(DEFAULT_SORT_STATE);
  });

  it('loadSortState ignores the legacy single-string format (PR #244 had been live only hours)', () => {
    localStorage.setItem(SORT_STATE_STORAGE_KEY, 'subject-asc');
    expect(loadSortState()).toEqual(DEFAULT_SORT_STATE);
  });

  it('saveSortState then loadSortState round-trips', () => {
    const state: SortState = { key: 'size', direction: 'asc' };
    saveSortState(state);
    expect(loadSortState()).toEqual(state);
  });

  it('loadSortState falls back to default when localStorage throws', () => {
    const spy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('storage unavailable');
    });
    expect(loadSortState()).toEqual(DEFAULT_SORT_STATE);
    spy.mockRestore();
  });

  it('saveSortState swallows storage errors silently', () => {
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });
    expect(() => saveSortState({ key: 'date', direction: 'asc' })).not.toThrow();
    spy.mockRestore();
  });
});

describe('compareMessages — direction', () => {
  const a = meta({ id: 'A', date: '2026-01-01T00:00:00Z' });
  const b = meta({ id: 'B', date: '2026-02-01T00:00:00Z' });

  it('asc and desc flip the result sign on the same primary delta', () => {
    const asc = compareMessages(a, b, { key: 'date', direction: 'asc' }, 'inbox');
    const desc = compareMessages(a, b, { key: 'date', direction: 'desc' }, 'inbox');
    expect(asc).toBeLessThan(0);
    expect(desc).toBeGreaterThan(0);
    expect(asc).toBe(-desc);
  });

  it('id tiebreak is ascending regardless of direction', () => {
    const x = meta({ id: 'X', date: '2026-01-01T00:00:00Z', subject: 'same', from: 'same' });
    const y = meta({ id: 'Y', date: '2026-01-01T00:00:00Z', subject: 'same', from: 'same' });
    for (const direction of ['asc', 'desc'] as const) {
      for (const key of SORT_KEY_OPTIONS.map((o) => o.id) as SortKey[]) {
        const result = compareMessages(x, y, { key, direction }, 'inbox');
        expect(result).toBeLessThan(0);
      }
    }
  });
});

describe('compareMessages — per-key semantics', () => {
  it('date: lexicographic ISO-8601 compare', () => {
    const a = meta({ id: 'A', date: '2026-01-01T00:00:00Z' });
    const b = meta({ id: 'B', date: '2026-02-01T00:00:00Z' });
    expect(compareMessages(a, b, { key: 'date', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });

  it('sender: case-insensitive, folder-aware (inbox = from)', () => {
    const a = meta({ id: 'A', from: 'kk4alpha' });
    const b = meta({ id: 'B', from: 'KK4BRAVO' });
    expect(compareMessages(a, b, { key: 'sender', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });

  it('sender: in sent/outbox uses recipient (to[0])', () => {
    const sent1 = meta({ id: '1', from: 'me', to: ['ALPHA'] });
    const sent2 = meta({ id: '2', from: 'me', to: ['BRAVO'] });
    expect(compareMessages(sent1, sent2, { key: 'sender', direction: 'asc' }, 'sent')).toBeLessThan(0);
    expect(compareMessages(sent1, sent2, { key: 'sender', direction: 'asc' }, 'outbox')).toBeLessThan(0);
  });

  it('recipient: always uses to[0] regardless of folder (falls back to from when to empty)', () => {
    const a = meta({ id: 'A', from: 'zulu-sender', to: ['ALPHA'] });
    const b = meta({ id: 'B', from: 'alpha-sender', to: ['BRAVO'] });
    // In inbox, sender-sort would put alpha-sender (B) before zulu-sender (A).
    // recipient-sort uses to[0]: ALPHA (A) < BRAVO (B) → A before B.
    for (const folder of ['inbox', 'sent', 'outbox', 'drafts'] as const) {
      expect(compareMessages(a, b, { key: 'recipient', direction: 'asc' }, folder)).toBeLessThan(0);
    }
  });

  it('recipient: bulletin (empty to) falls back to from so it still sorts deterministically', () => {
    const bulletin = meta({ id: 'B', from: 'ANNOUNCE', to: [] });
    const direct = meta({ id: 'D', from: 'sender', to: ['ZULU'] });
    // bulletin.from = 'ANNOUNCE', direct.to[0] = 'ZULU' → ANNOUNCE < ZULU
    expect(compareMessages(bulletin, direct, { key: 'recipient', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });

  it('subject: case-insensitive', () => {
    const a = meta({ id: 'A', subject: 'apples' });
    const b = meta({ id: 'B', subject: 'BANANAS' });
    expect(compareMessages(a, b, { key: 'subject', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });

  it('size: numeric (NOT lexicographic — "10 KB" must beat "2 KB")', () => {
    const small = meta({ id: 'S', bodySize: 2_048 });
    const big = meta({ id: 'B', bodySize: 1_048_576 });
    expect(compareMessages(small, big, { key: 'size', direction: 'asc' }, 'inbox')).toBeLessThan(0);
    expect(compareMessages(small, big, { key: 'size', direction: 'desc' }, 'inbox')).toBeGreaterThan(0);
    // A lexicographic compare on "2048".localeCompare("1048576") would be > 0
    // (string "2" > "1"). Numeric compare must give the opposite.
  });

  it('size: zero is a valid size that sorts at the bottom of asc', () => {
    const zero = meta({ id: 'Z', bodySize: 0 });
    const small = meta({ id: 'S', bodySize: 1024 });
    expect(compareMessages(zero, small, { key: 'size', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });

  it('sender: ignores leading/trailing whitespace', () => {
    const padded = meta({ id: 'C', from: '  AAA  ' });
    const clean = meta({ id: 'D', from: 'BBB' });
    expect(compareMessages(padded, clean, { key: 'sender', direction: 'asc' }, 'inbox')).toBeLessThan(0);
  });
});

describe('sortMessages', () => {
  const m1 = meta({ id: 'M1', subject: 'Charlie', from: 'kk4zulu', date: '2026-01-01T00:00:00Z', bodySize: 100 });
  const m2 = meta({ id: 'M2', subject: 'Alpha', from: 'kk4alpha', date: '2026-03-01T00:00:00Z', bodySize: 10_000 });
  const m3 = meta({ id: 'M3', subject: 'Bravo', from: 'KK4MIKE', date: '2026-02-01T00:00:00Z', bodySize: 1_000 });

  it('does not mutate the input array', () => {
    const input = [m1, m2, m3];
    const beforeIds = input.map((m) => m.id);
    sortMessages(input, { key: 'date', direction: 'desc' }, 'inbox');
    expect(input.map((m) => m.id)).toEqual(beforeIds);
  });

  it('date desc: newest first', () => {
    expect(
      sortMessages([m1, m2, m3], { key: 'date', direction: 'desc' }, 'inbox').map((m) => m.id),
    ).toEqual(['M2', 'M3', 'M1']);
  });

  it('size asc: smallest first (numeric)', () => {
    expect(
      sortMessages([m1, m2, m3], { key: 'size', direction: 'asc' }, 'inbox').map((m) => m.id),
    ).toEqual(['M1', 'M3', 'M2']);
  });

  it('size desc: largest first', () => {
    expect(
      sortMessages([m1, m2, m3], { key: 'size', direction: 'desc' }, 'inbox').map((m) => m.id),
    ).toEqual(['M2', 'M3', 'M1']);
  });

  it('subject asc: Alpha < Bravo < Charlie', () => {
    expect(
      sortMessages([m1, m2, m3], { key: 'subject', direction: 'asc' }, 'inbox').map((m) => m.id),
    ).toEqual(['M2', 'M3', 'M1']);
  });

  it('handles empty and single-element lists', () => {
    expect(sortMessages([], DEFAULT_SORT_STATE, 'inbox')).toEqual([]);
    expect(sortMessages([m1], DEFAULT_SORT_STATE, 'inbox').map((m) => m.id)).toEqual(['M1']);
  });

  it('produces deterministic output for every (key, direction) combo regardless of input order', () => {
    for (const opt of SORT_KEY_OPTIONS) {
      for (const direction of ['asc', 'desc'] as const) {
        const state: SortState = { key: opt.id, direction };
        const a = sortMessages([m1, m2, m3], state, 'inbox').map((m) => m.id);
        const b = sortMessages([m3, m2, m1], state, 'inbox').map((m) => m.id);
        expect(a).toEqual(b);
      }
    }
  });
});
