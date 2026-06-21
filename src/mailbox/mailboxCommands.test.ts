// Tests for tuxlink-wl7n Task 10 — TS invoke wrappers for delete/restore/trash.

import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

import {
  deleteMessages,
  restoreMessages,
  emptyTrash,
  purgeMessage,
} from './mailboxCommands';

describe('mailboxCommands — delete/restore/trash invoke wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
  });

  it('deleteMessages calls message_delete_bulk with id+folder pairs', async () => {
    await deleteMessages([{ id: 'MID1', folder: 'inbox' }]);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('message_delete_bulk', {
      items: [{ id: 'MID1', folder: 'inbox' }],
    });
  });

  it('deleteMessages strips identity from the wire call', async () => {
    await deleteMessages([{ id: 'MID2', folder: 'sent', identity: 'N0CALL' }]);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('message_delete_bulk', {
      items: [{ id: 'MID2', folder: 'sent' }],
    });
  });

  it('deleteMessages handles multiple items', async () => {
    await deleteMessages([
      { id: 'MID1', folder: 'inbox' },
      { id: 'MID2', folder: 'archive' },
    ]);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('message_delete_bulk', {
      items: [
        { id: 'MID1', folder: 'inbox' },
        { id: 'MID2', folder: 'archive' },
      ],
    });
  });

  it('restoreMessages calls message_restore_bulk with a flat id array', async () => {
    await restoreMessages(['MID1', 'MID2']);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('message_restore_bulk', {
      ids: ['MID1', 'MID2'],
    });
  });

  it('emptyTrash calls trash_empty and returns the count', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);
    const count = await emptyTrash();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('trash_empty');
    expect(count).toBe(7);
  });

  it('purgeMessage calls trash_purge_one with the given id', async () => {
    await purgeMessage('MID3');
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('trash_purge_one', { id: 'MID3' });
  });
});
