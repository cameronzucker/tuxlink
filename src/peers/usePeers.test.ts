import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { P2pCapabilities, PeersFile } from './types';

// --- Mocks (MUST precede the module-under-test import) ---------------------
// invoke is the Tauri command bridge; listen is the cross-window event bridge.
// We capture the `peers:changed` handler so the test can fire it and assert
// the resulting query invalidation.
type EventHandler<T> = (event: { payload: T }) => void;
let capturedPeersChangedHandler: EventHandler<void> | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: EventHandler<unknown>) => {
    if (event === 'peers:changed') {
      capturedPeersChangedHandler = handler as EventHandler<void>;
    }
    return () => {};
  }),
}));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  PEERS_CHANGED_EVENT,
  PEERS_QUERY_KEY,
  useP2pCapabilities,
  usePeers,
} from './usePeers';

// Note: the invoke mock's calls are asserted via toHaveBeenCalledWith (an
// existence check), never toHaveBeenCalledTimes — a teardown/unmount pass can
// invoke the mock again with no args, which would make a strict call-count
// assertion flaky (house vitest gotcha).
const invokeMock = invoke as ReturnType<typeof vi.fn>;
const listenMock = listen as ReturnType<typeof vi.fn>;

const SAMPLE: PeersFile = {
  schema_version: 1,
  peers: [
    {
      id: 'p1',
      canonical_base: 'W6ABC',
      presented_callsigns: ['W6ABC-7'],
      identity_kind: 'unknown',
      do_not_merge: false,
      conflict: false,
      source: 'auto',
      origin: 'outgoing',
      contact_id: null,
      grid: null,
      note: '',
      created_at: '2026-07-10T12:00:00-07:00',
      last_connected_at: null,
      channels: [],
      endpoints: [],
    },
  ],
};

const CAPS: P2pCapabilities = {
  peer_store: true,
  finder_peers: false,
  map_peers: false,
  settings_editor: false,
  agent_find_peers: true,
  agent_telnet_dial: true,
  vara_engine_split: true,
  favorites_peer_link: true,
};

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

function newQc() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

beforeEach(() => {
  capturedPeersChangedHandler = null;
  invokeMock.mockReset();
  listenMock.mockClear();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'peers_read') return Promise.resolve(SAMPLE);
    if (cmd === 'p2p_capabilities') return Promise.resolve(CAPS);
    return Promise.resolve(undefined);
  });
});

describe('usePeers', () => {
  it('exposes peers/schemaVersion derived from peers_read', async () => {
    const { result } = renderHook(() => usePeers(), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(invokeMock).toHaveBeenCalledWith('peers_read');
    expect(result.current.peers).toEqual(SAMPLE.peers);
    expect(result.current.schemaVersion).toBe(1);
  });

  it('defaults peers to [] before the read resolves', async () => {
    const { result } = renderHook(() => usePeers(), { wrapper: wrapperWith(newQc()) });
    expect(result.current.peers).toEqual([]);
    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });

  it('subscribes to peers:changed and invalidates the peers query when it fires', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => usePeers(), { wrapper: wrapperWith(qc) });

    await waitFor(() => {
      expect(listenMock).toHaveBeenCalledWith(PEERS_CHANGED_EVENT, expect.any(Function));
      expect(capturedPeersChangedHandler).not.toBeNull();
    });

    invalidateSpy.mockClear();
    capturedPeersChangedHandler?.({ payload: undefined });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: PEERS_QUERY_KEY }),
    );
  });

  it('PEERS_CHANGED_EVENT is the exact backend contract string', () => {
    expect(PEERS_CHANGED_EVENT).toBe('peers:changed');
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
