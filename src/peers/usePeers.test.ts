import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { ContactsFile } from '../contacts/types';
import type { P2pCapabilities } from './types';

// --- Mocks (MUST precede the module-under-test import) ---------------------
// invoke is the Tauri command bridge; listen is the cross-window event bridge.
// usePeers now delegates to useContacts, so it reads `contacts_read` and
// listens for `contacts:changed` (T-E: the peers store died — see
// `../contacts/useContacts.test.ts` for the hook this one now projects).
type EventHandler<T> = (event: { payload: T }) => void;
let capturedContactsChangedHandler: EventHandler<void> | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: EventHandler<unknown>) => {
    if (event === 'contacts:changed') {
      capturedContactsChangedHandler = handler as EventHandler<void>;
    }
    return () => {};
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { CONTACTS_CHANGED_EVENT, CONTACTS_QUERY_KEY } from '../contacts/useContacts';
import { useP2pCapabilities, usePeers } from './usePeers';

// Note: the invoke mock's calls are asserted via toHaveBeenCalledWith (an
// existence check), never toHaveBeenCalledTimes — a teardown/unmount pass can
// invoke the mock again with no args, which would make a strict call-count
// assertion flaky (house vitest gotcha).
const invokeMock = invoke as ReturnType<typeof vi.fn>;
const listenMock = listen as ReturnType<typeof vi.fn>;

const SAMPLE: ContactsFile = {
  schema_version: 2,
  contacts: [
    {
      id: 'c1',
      name: '',
      callsign: 'W6ABC-7',
      tier: 'unconfirmed',
      origin: 'outgoing',
      channels: [],
      endpoints: [],
      created_at: '2026-07-10T12:00:00-07:00',
      updated_at: '2026-07-10T12:00:00-07:00',
    },
  ],
  groups: [],
};

const CAPS: P2pCapabilities = {
  peer_store: true,
  finder_peers: false,
  map_peers: false,
  agent_find_peers: true,
  vara_engine_split: true,
  favorites_contact_link: true,
};

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

function newQc() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

beforeEach(() => {
  capturedContactsChangedHandler = null;
  invokeMock.mockReset();
  listenMock.mockClear();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'contacts_read') return Promise.resolve(SAMPLE);
    if (cmd === 'p2p_capabilities') return Promise.resolve(CAPS);
    return Promise.resolve(undefined);
  });
});

describe('usePeers', () => {
  it('exposes peers (raw Contact[]) derived from contacts_read', async () => {
    const { result } = renderHook(() => usePeers(), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(invokeMock).toHaveBeenCalledWith('contacts_read');
    expect(result.current.peers).toEqual(SAMPLE.contacts);
  });

  it('defaults peers to [] before the read resolves', async () => {
    const { result } = renderHook(() => usePeers(), { wrapper: wrapperWith(newQc()) });
    expect(result.current.peers).toEqual([]);
    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it('subscribes to contacts:changed and invalidates the shared [contacts] query when it fires', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => usePeers(), { wrapper: wrapperWith(qc) });

    await waitFor(() => {
      expect(listenMock).toHaveBeenCalledWith(CONTACTS_CHANGED_EVENT, expect.any(Function));
      expect(capturedContactsChangedHandler).not.toBeNull();
    });

    invalidateSpy.mockClear();
    capturedContactsChangedHandler?.({ payload: undefined });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY }),
    );
  });
});

describe('useP2pCapabilities', () => {
  it('exposes capabilities derived from p2p_capabilities', async () => {
    const { result } = renderHook(() => useP2pCapabilities(), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(invokeMock).toHaveBeenCalledWith('p2p_capabilities');
    expect(result.current.capabilities).toEqual(CAPS);
  });
});
