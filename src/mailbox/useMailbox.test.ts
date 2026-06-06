import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

type EventHandler<T> = (event: { payload: T }) => void;
let capturedMailboxChangedHandler: EventHandler<void> | null = null;

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: EventHandler<unknown>) => {
    if (event === 'mailbox:changed') {
      capturedMailboxChangedHandler = handler as EventHandler<void>;
    }
    return () => {};
  }),
}));

import { listen } from '@tauri-apps/api/event';
import {
  MAILBOX_CHANGED_EVENT,
  MAILBOX_QUERY_KEY,
  isBackendFolder,
  isUserFolderSlug,
  useMailboxChangeEvents,
} from './useMailbox';

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

beforeEach(() => {
  capturedMailboxChangedHandler = null;
  (listen as ReturnType<typeof vi.fn>).mockClear();
});

describe('isBackendFolder', () => {
  it('treats inbox/outbox/sent/archive as backend folders', () => {
    expect(isBackendFolder('inbox')).toBe(true);
    expect(isBackendFolder('outbox')).toBe(true);
    expect(isBackendFolder('sent')).toBe(true);
    // tuxlink-ca5x: Archive is wired through the same `mailbox_list` Tauri
    // command as the other system folders — it just dispatches with
    // folder="archive".
    expect(isBackendFolder('archive')).toBe(true);
  });

  it('treats drafts/deleted as NON-backend folders (no command dispatch)', () => {
    // Drafts is a local store; Deleted is a disabled placeholder (spec §2.2).
    expect(isBackendFolder('drafts')).toBe(false);
    expect(isBackendFolder('deleted')).toBe(false);
  });

  // tuxlink-f62f: user-folder slugs ride alongside system folder identifiers.
  // The Tauri backend dispatches at parse time; the frontend just needs to
  // recognize valid slugs as fetchable.
  it('treats valid user-folder slugs as backend folders', () => {
    expect(isBackendFolder('ares-drills')).toBe(true);
    expect(isBackendFolder('a')).toBe(true);
    expect(isBackendFolder('disaster-prep-2026')).toBe(true);
  });

  it('rejects invalid slug shapes as non-backend folders', () => {
    expect(isBackendFolder('ARES')).toBe(false); // uppercase
    expect(isBackendFolder('ares drills')).toBe(false); // space
    expect(isBackendFolder('-ares')).toBe(false); // leading dash
    expect(isBackendFolder('ares-')).toBe(false); // trailing dash
    expect(isBackendFolder('ares--drills')).toBe(false); // consecutive dashes
    expect(isBackendFolder('')).toBe(false);
  });
});

describe('isUserFolderSlug', () => {
  it('accepts canonical slugs', () => {
    expect(isUserFolderSlug('ares-drills')).toBe(true);
    expect(isUserFolderSlug('ke7var-thread-2026')).toBe(true);
    expect(isUserFolderSlug('a')).toBe(true);
  });

  it('rejects bad shapes', () => {
    expect(isUserFolderSlug('ARES')).toBe(false);
    expect(isUserFolderSlug('ares drills')).toBe(false);
    expect(isUserFolderSlug('ares.drills')).toBe(false);
    expect(isUserFolderSlug('ares/drills')).toBe(false);
    expect(isUserFolderSlug('-ares')).toBe(false);
    expect(isUserFolderSlug('ares-')).toBe(false);
    expect(isUserFolderSlug('ares--drills')).toBe(false);
    expect(isUserFolderSlug('')).toBe(false);
    expect(isUserFolderSlug('a'.repeat(41))).toBe(false);
  });
});

describe('useMailboxChangeEvents', () => {
  it('invalidates mailbox queries when the backend emits mailbox:changed', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    renderHook(() => useMailboxChangeEvents(), { wrapper: wrapperWith(qc) });

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith(MAILBOX_CHANGED_EVENT, expect.any(Function));
      expect(capturedMailboxChangedHandler).not.toBeNull();
    });

    capturedMailboxChangedHandler?.({ payload: undefined });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: MAILBOX_QUERY_KEY }),
    );
  });
});
