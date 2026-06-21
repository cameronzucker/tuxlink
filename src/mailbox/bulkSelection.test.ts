import { describe, it, expect } from 'vitest';
import type { MessageMeta } from './types';
import { selectionToFolderItems, dropId, dropIds } from './bulkSelection';

function meta(over: Partial<MessageMeta> = {}): MessageMeta {
  return {
    id: 'MID1',
    subject: 'Hello',
    from: 'W4PHS@winlink.org',
    to: [],
    date: '2026-06-09T14:05:00Z',
    unread: false,
    bodySize: 1024,
    hasAttachments: false,
    ...over,
  };
}

describe('selectionToFolderItems (tuxlink-l80q bulk move/archive mapping)', () => {
  it('maps each selected id to its own message.folder (cross-folder selection)', () => {
    const visible = [
      meta({ id: 'A', folder: 'inbox' }),
      meta({ id: 'B', folder: 'sent' }),
      meta({ id: 'C', folder: 'archive' }),
    ];
    const items = selectionToFolderItems(new Set(['A', 'B', 'C']), visible, 'inbox');
    expect(items).toEqual([
      { folder: 'inbox', id: 'A', identity: undefined },
      { folder: 'sent', id: 'B', identity: undefined },
      { folder: 'archive', id: 'C', identity: undefined },
    ]);
  });

  it('falls back to the active folder when a row carries no own folder', () => {
    const visible = [meta({ id: 'A', folder: undefined })];
    const items = selectionToFolderItems(new Set(['A']), visible, 'inbox');
    expect(items).toEqual([{ folder: 'inbox', id: 'A', identity: undefined }]);
  });

  it('filters stale ids absent from the visible list (Fix-3 pattern, #499)', () => {
    const visible = [meta({ id: 'A', folder: 'inbox' })];
    // 'GHOST' was selected then removed from the list before the action fired.
    const items = selectionToFolderItems(new Set(['A', 'GHOST']), visible, 'inbox');
    expect(items).toEqual([{ folder: 'inbox', id: 'A', identity: undefined }]);
  });

  it('returns an empty array for an empty selection', () => {
    const visible = [meta({ id: 'A', folder: 'inbox' })];
    expect(selectionToFolderItems(new Set(), visible, 'inbox')).toEqual([]);
  });

  // tuxlink-wl7n Task 13 (Part A + B): identity is now threaded through so
  // delete/restore target the correct per-identity namespace.
  it('forwards identity from MessageMeta when present (tuxlink-wl7n Task 13)', () => {
    const visible = [
      meta({ id: 'A', folder: 'inbox', identity: 'W1ABC' }),
      meta({ id: 'B', folder: 'inbox' }), // no identity — absent
    ];
    const items = selectionToFolderItems(new Set(['A', 'B']), visible, 'inbox');
    expect(items).toEqual([
      { folder: 'inbox', id: 'A', identity: 'W1ABC' },
      { folder: 'inbox', id: 'B', identity: undefined },
    ]);
  });
});

describe('dropId', () => {
  it('removes the id when present', () => {
    expect([...dropId(new Set(['A', 'B']), 'A')]).toEqual(['B']);
  });
  it('returns the SAME set reference when the id is absent (no churn)', () => {
    const set = new Set(['A', 'B']);
    expect(dropId(set, 'Z')).toBe(set);
  });
});

describe('dropIds', () => {
  it('removes every intersecting id (moved + stale targets)', () => {
    expect([...dropIds(new Set(['A', 'B', 'C']), new Set(['A', 'C', 'GHOST']))]).toEqual(['B']);
  });
  it('returns the SAME set reference when nothing intersects', () => {
    const set = new Set(['A', 'B']);
    expect(dropIds(set, new Set(['Y', 'Z']))).toBe(set);
  });
});
