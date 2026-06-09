import { describe, it, expect } from 'vitest';
import type { MessageMeta } from './types';
import { selectionToFolderItems } from './bulkSelection';

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
      { folder: 'inbox', id: 'A' },
      { folder: 'sent', id: 'B' },
      { folder: 'archive', id: 'C' },
    ]);
  });

  it('falls back to the active folder when a row carries no own folder', () => {
    const visible = [meta({ id: 'A', folder: undefined })];
    const items = selectionToFolderItems(new Set(['A']), visible, 'inbox');
    expect(items).toEqual([{ folder: 'inbox', id: 'A' }]);
  });

  it('filters stale ids absent from the visible list (Fix-3 pattern, #499)', () => {
    const visible = [meta({ id: 'A', folder: 'inbox' })];
    // 'GHOST' was selected then removed from the list before the action fired.
    const items = selectionToFolderItems(new Set(['A', 'GHOST']), visible, 'inbox');
    expect(items).toEqual([{ folder: 'inbox', id: 'A' }]);
  });

  it('returns an empty array for an empty selection', () => {
    const visible = [meta({ id: 'A', folder: 'inbox' })];
    expect(selectionToFolderItems(new Set(), visible, 'inbox')).toEqual([]);
  });
});
