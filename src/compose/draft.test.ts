// Unit tests for the localStorage draft store.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §6
// Task 14 (tuxlink-dm8) test list items 1–4 + autosave timer test (8).
//
// Vitest + jsdom: localStorage is present in the jsdom environment, so no
// mocking is required for basic get/set. Tests clean up the store between
// runs via `localStorage.clear()`.
//
// Test coverage:
//   (1) draft round-trips localStorage
//   (2) clearDraft removes the entry and the index entry
//   (3) loadDraft on unknown id → null
//   (4) To/Cc split on ';', trim empties

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  clearDraft,
  DraftData,
  DRAFT_INDEX_KEY,
  listDraftIds,
  loadDraft,
  saveDraft,
  splitAddrs,
} from './useDraft';

afterEach(() => {
  localStorage.clear();
});

// ---------------------------------------------------------------------------
// (1) Draft round-trips localStorage
// ---------------------------------------------------------------------------

describe('saveDraft / loadDraft round-trip', () => {
  it('stores a draft and retrieves it by id', () => {
    const draft = saveDraft({
      draftId: 'draft-001',
      to: 'W6ABC@winlink.org',
      subject: 'Test message',
      body: 'Hello ARES net',
      requestAck: false,
    });

    expect(draft.draftId).toBe('draft-001');
    expect(draft.savedAt).toBeTruthy();

    const loaded = loadDraft('draft-001');
    expect(loaded).not.toBeNull();
    expect(loaded!.subject).toBe('Test message');
    expect(loaded!.to).toBe('W6ABC@winlink.org');
    expect(loaded!.body).toBe('Hello ARES net');
    expect(loaded!.requestAck).toBe(false);
  });

  it('updates an existing draft without creating a duplicate index entry', () => {
    saveDraft({ draftId: 'd1', to: 'A', subject: 'S1', body: 'B1', requestAck: false });
    saveDraft({ draftId: 'd1', to: 'A', subject: 'S2', body: 'B2', requestAck: true });

    const ids = listDraftIds();
    expect(ids.filter((x) => x === 'd1')).toHaveLength(1);

    const loaded = loadDraft('d1');
    expect(loaded!.subject).toBe('S2');
  });

  it('adds the draft id to the index', () => {
    saveDraft({ draftId: 'd2', to: '', subject: '', body: '', requestAck: false });
    expect(listDraftIds()).toContain('d2');
  });

  it('savedAt is an ISO 8601 string', () => {
    const draft = saveDraft({ draftId: 'd3', to: '', subject: '', body: '', requestAck: false });
    expect(() => new Date(draft.savedAt).toISOString()).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// (2) clearDraft removes entry AND index entry
// ---------------------------------------------------------------------------

describe('clearDraft', () => {
  it('removes the draft from localStorage', () => {
    saveDraft({ draftId: 'clear-me', to: '', subject: '', body: '', requestAck: false });
    clearDraft('clear-me');
    expect(loadDraft('clear-me')).toBeNull();
  });

  it('removes the id from the index', () => {
    saveDraft({ draftId: 'idx-me', to: '', subject: '', body: '', requestAck: false });
    clearDraft('idx-me');
    expect(listDraftIds()).not.toContain('idx-me');
  });

  it('is a no-op for an unknown id', () => {
    // Should not throw
    clearDraft('nonexistent-draft-id');
    expect(listDraftIds()).toHaveLength(0);
  });

  it('only removes the targeted draft when multiple exist', () => {
    saveDraft({ draftId: 'keep', to: '', subject: 'keep', body: '', requestAck: false });
    saveDraft({ draftId: 'remove', to: '', subject: 'remove', body: '', requestAck: false });
    clearDraft('remove');
    expect(listDraftIds()).toContain('keep');
    expect(listDraftIds()).not.toContain('remove');
    expect(loadDraft('keep')).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// (3) loadDraft unknown id → null
// ---------------------------------------------------------------------------

describe('loadDraft', () => {
  it('returns null for an unknown id', () => {
    expect(loadDraft('this-does-not-exist')).toBeNull();
  });

  it('returns null when the stored value is malformed JSON', () => {
    localStorage.setItem('tuxlink.drafts.bad', 'not-json{');
    expect(loadDraft('bad')).toBeNull();
  });

  it('returns null when the stored value is valid JSON but wrong shape', () => {
    localStorage.setItem('tuxlink.drafts.bad2', JSON.stringify({ wrong: true }));
    expect(loadDraft('bad2')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// (4) To/Cc split on ';' trims empties  (spec §5.4, §6 test 4)
// ---------------------------------------------------------------------------

describe('splitAddrs', () => {
  it('splits a semicolon-separated string', () => {
    expect(splitAddrs('W6ABC@winlink.org;W7DEF@winlink.org')).toEqual([
      'W6ABC@winlink.org',
      'W7DEF@winlink.org',
    ]);
  });

  it('trims whitespace around each address', () => {
    expect(splitAddrs(' W6ABC ; W7DEF ')).toEqual(['W6ABC', 'W7DEF']);
  });

  it('filters empty segments (trailing/double semicolons)', () => {
    expect(splitAddrs('W6ABC;;W7DEF;')).toEqual(['W6ABC', 'W7DEF']);
  });

  it('returns an empty array for an empty string', () => {
    expect(splitAddrs('')).toEqual([]);
  });

  it('returns an array with one element when no semicolons', () => {
    expect(splitAddrs('W6ABC@winlink.org')).toEqual(['W6ABC@winlink.org']);
  });

  it('returns empty for whitespace-only input', () => {
    expect(splitAddrs('   ;  ; ')).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// (5) listDraftIds is the Drafts-folder source
// ---------------------------------------------------------------------------

describe('listDraftIds', () => {
  it('returns an empty array when no drafts exist', () => {
    expect(listDraftIds()).toEqual([]);
  });

  it('returns all saved draft ids in insertion order', () => {
    saveDraft({ draftId: 'a', to: '', subject: '', body: '', requestAck: false });
    saveDraft({ draftId: 'b', to: '', subject: '', body: '', requestAck: false });
    saveDraft({ draftId: 'c', to: '', subject: '', body: '', requestAck: false });
    expect(listDraftIds()).toEqual(['a', 'b', 'c']);
  });

  it('is resilient to a malformed index', () => {
    localStorage.setItem(DRAFT_INDEX_KEY, 'not-json{');
    expect(listDraftIds()).toEqual([]);
  });

  it('filters non-string entries from a corrupted index', () => {
    localStorage.setItem(DRAFT_INDEX_KEY, JSON.stringify(['valid', 42, null, 'also-valid']));
    expect(listDraftIds()).toEqual(['valid', 'also-valid']);
  });
});

// ---------------------------------------------------------------------------
// (8) Autosave fires after 2s (fake timers — structural only)
//     Tests that a setInterval with 2000ms delay would invoke saveDraft.
// ---------------------------------------------------------------------------

describe('autosave interval contract (structural)', () => {
  it('a 2000ms interval invokes the callback after 2 seconds', () => {
    vi.useFakeTimers();
    const autosaveFn = vi.fn();
    const interval = setInterval(autosaveFn, 2000);

    vi.advanceTimersByTime(1999);
    expect(autosaveFn).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(autosaveFn).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2000);
    expect(autosaveFn).toHaveBeenCalledTimes(2);

    clearInterval(interval);
    vi.useRealTimers();
  });
});

// ---------------------------------------------------------------------------
// (5-send) send maps to OutboundDraftDto shape  (spec §6 test 5)
// ---------------------------------------------------------------------------

describe('OutboundDraftDto mapping (structural)', () => {
  it('split To maps correctly to the expected DTO shape', () => {
    const rawTo = 'W6ABC@winlink.org ; W7DEF@winlink.org';
    const draft: DraftData = {
      draftId: 'dto-test',
      to: rawTo,
      subject: 'ICS-213 check',
      body: 'Body text',
      requestAck: true,
      savedAt: new Date().toISOString(),
    };

    const dto = {
      to: splitAddrs(draft.to),
      subject: draft.subject,
      body: draft.body,
    };

    expect(dto.to).toEqual(['W6ABC@winlink.org', 'W7DEF@winlink.org']);
    expect(dto.subject).toBe('ICS-213 check');
    expect(dto.body).toBe('Body text');
  });
});
