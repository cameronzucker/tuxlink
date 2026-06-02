import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { MessageMeta } from './types';
import {
  type SortMode,
  DEFAULT_SORT_MODE,
  SORT_OPTIONS,
  SORT_MODE_STORAGE_KEY,
  isSortMode,
  loadSortMode,
  saveSortMode,
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

describe('SORT_OPTIONS', () => {
  it('lists six modes in the spec order (date desc/asc, sender, subject)', () => {
    expect(SORT_OPTIONS.map((o) => o.id)).toEqual([
      'date-desc',
      'date-asc',
      'sender-asc',
      'sender-desc',
      'subject-asc',
      'subject-desc',
    ]);
  });

  it('default matches the backend baseline (newest first)', () => {
    expect(DEFAULT_SORT_MODE).toBe('date-desc');
    expect(SORT_OPTIONS[0].id).toBe(DEFAULT_SORT_MODE);
  });

  it('every option has a non-empty label', () => {
    for (const opt of SORT_OPTIONS) {
      expect(opt.label.length).toBeGreaterThan(0);
    }
  });
});

describe('isSortMode', () => {
  it('accepts every defined mode', () => {
    for (const opt of SORT_OPTIONS) {
      expect(isSortMode(opt.id)).toBe(true);
    }
  });
  it('rejects unknown / non-string / null', () => {
    expect(isSortMode('garbage')).toBe(false);
    expect(isSortMode(null)).toBe(false);
    expect(isSortMode(undefined)).toBe(false);
    expect(isSortMode(42)).toBe(false);
    expect(isSortMode({})).toBe(false);
  });
});

describe('localStorage round-trip', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('loadSortMode returns default when storage is empty', () => {
    expect(loadSortMode()).toBe(DEFAULT_SORT_MODE);
  });

  it('loadSortMode returns default for unknown values', () => {
    localStorage.setItem(SORT_MODE_STORAGE_KEY, 'garbage');
    expect(loadSortMode()).toBe(DEFAULT_SORT_MODE);
  });

  it('saveSortMode then loadSortMode round-trips', () => {
    saveSortMode('subject-asc');
    expect(loadSortMode()).toBe('subject-asc');
  });

  it('loadSortMode falls back to default when localStorage throws', () => {
    const spy = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('storage unavailable');
    });
    expect(loadSortMode()).toBe(DEFAULT_SORT_MODE);
    spy.mockRestore();
  });

  it('saveSortMode swallows storage errors silently', () => {
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });
    expect(() => saveSortMode('date-asc')).not.toThrow();
    spy.mockRestore();
  });
});

describe('compareMessages', () => {
  const a = meta({ id: 'A', subject: 'Apples', from: 'kk4alpha', date: '2026-01-01T00:00:00Z' });
  const b = meta({ id: 'B', subject: 'Bananas', from: 'KK4BRAVO', date: '2026-02-01T00:00:00Z' });

  it('date-desc: newer < older (newer sorts first)', () => {
    expect(compareMessages(a, b, 'date-desc', 'inbox')).toBeGreaterThan(0);
    expect(compareMessages(b, a, 'date-desc', 'inbox')).toBeLessThan(0);
  });

  it('date-asc: older first', () => {
    expect(compareMessages(a, b, 'date-asc', 'inbox')).toBeLessThan(0);
  });

  it('sender-asc: case-insensitive (kk4alpha < KK4BRAVO)', () => {
    expect(compareMessages(a, b, 'sender-asc', 'inbox')).toBeLessThan(0);
  });

  it('sender-asc: ignores leading/trailing whitespace', () => {
    const padded = meta({ id: 'C', from: '  AAA  ' });
    const clean = meta({ id: 'D', from: 'BBB' });
    expect(compareMessages(padded, clean, 'sender-asc', 'inbox')).toBeLessThan(0);
  });

  it('sender-* in sent/outbox uses recipient (to[0]), not sender', () => {
    const sent1 = meta({ id: '1', from: 'me', to: ['ALPHA'] });
    const sent2 = meta({ id: '2', from: 'me', to: ['BRAVO'] });
    expect(compareMessages(sent1, sent2, 'sender-asc', 'sent')).toBeLessThan(0);
    expect(compareMessages(sent1, sent2, 'sender-asc', 'outbox')).toBeLessThan(0);
    // and in inbox, both are "me" — falls to id tiebreak (1 < 2)
    expect(compareMessages(sent1, sent2, 'sender-asc', 'inbox')).toBeLessThan(0);
  });

  it('sender-* in sent/outbox falls back to from when to is empty', () => {
    const sentNoTo = meta({ id: '1', from: 'ZULU', to: [] });
    const sentWithTo = meta({ id: '2', from: 'AAA', to: ['ALPHA'] });
    // ZULU > ALPHA, so sentNoTo sorts AFTER sentWithTo in asc
    expect(compareMessages(sentNoTo, sentWithTo, 'sender-asc', 'sent')).toBeGreaterThan(0);
  });

  it('subject-asc: lexicographic, case-insensitive', () => {
    expect(compareMessages(a, b, 'subject-asc', 'inbox')).toBeLessThan(0);
    expect(compareMessages(b, a, 'subject-asc', 'inbox')).toBeGreaterThan(0);
  });

  it('subject-desc: reverse of asc', () => {
    expect(compareMessages(a, b, 'subject-desc', 'inbox')).toBeGreaterThan(0);
  });

  it('ties on primary key fall back to id (ascending) for stable order', () => {
    const x = meta({ id: 'X', subject: 'same', from: 'same', date: '2026-01-01T00:00:00Z' });
    const y = meta({ id: 'Y', subject: 'same', from: 'same', date: '2026-01-01T00:00:00Z' });
    expect(compareMessages(x, y, 'date-desc', 'inbox')).toBeLessThan(0);
    expect(compareMessages(y, x, 'date-desc', 'inbox')).toBeGreaterThan(0);
    expect(compareMessages(x, x, 'date-desc', 'inbox')).toBe(0);
  });
});

describe('sortMessages', () => {
  const m1 = meta({ id: 'M1', subject: 'Charlie', from: 'kk4zulu', date: '2026-01-01T00:00:00Z' });
  const m2 = meta({ id: 'M2', subject: 'Alpha', from: 'kk4alpha', date: '2026-03-01T00:00:00Z' });
  const m3 = meta({ id: 'M3', subject: 'Bravo', from: 'KK4MIKE', date: '2026-02-01T00:00:00Z' });

  it('does not mutate the input array', () => {
    const input = [m1, m2, m3];
    const beforeIds = input.map((m) => m.id);
    sortMessages(input, 'date-desc', 'inbox');
    expect(input.map((m) => m.id)).toEqual(beforeIds);
  });

  it('date-desc: newest first', () => {
    expect(sortMessages([m1, m2, m3], 'date-desc', 'inbox').map((m) => m.id)).toEqual([
      'M2',
      'M3',
      'M1',
    ]);
  });

  it('date-asc: oldest first', () => {
    expect(sortMessages([m1, m2, m3], 'date-asc', 'inbox').map((m) => m.id)).toEqual([
      'M1',
      'M3',
      'M2',
    ]);
  });

  it('sender-asc: kk4alpha < KK4MIKE < kk4zulu (case-insensitive)', () => {
    expect(sortMessages([m1, m2, m3], 'sender-asc', 'inbox').map((m) => m.id)).toEqual([
      'M2',
      'M3',
      'M1',
    ]);
  });

  it('subject-asc: Alpha < Bravo < Charlie', () => {
    expect(sortMessages([m1, m2, m3], 'subject-asc', 'inbox').map((m) => m.id)).toEqual([
      'M2',
      'M3',
      'M1',
    ]);
  });

  it('handles an empty list', () => {
    expect(sortMessages([], 'date-desc', 'inbox')).toEqual([]);
  });

  it('handles a single-element list', () => {
    expect(sortMessages([m1], 'date-desc', 'inbox').map((m) => m.id)).toEqual(['M1']);
  });

  it('produces deterministic output for every defined sort mode', () => {
    const modes: SortMode[] = [
      'date-desc',
      'date-asc',
      'sender-asc',
      'sender-desc',
      'subject-asc',
      'subject-desc',
    ];
    for (const mode of modes) {
      const a = sortMessages([m1, m2, m3], mode, 'inbox').map((m) => m.id);
      const b = sortMessages([m3, m2, m1], mode, 'inbox').map((m) => m.id);
      expect(a).toEqual(b);
    }
  });
});
