import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { ContactsFile } from './types';

// --- Mocks (MUST precede the module-under-test import) ---------------------
// invoke is the Tauri command bridge; listen is the cross-window event bridge
// (H9). We capture the `contacts:changed` handler so the test can fire it and
// assert the resulting query invalidation.
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
import { CONTACTS_CHANGED_EVENT, CONTACTS_QUERY_KEY, useContacts } from './useContacts';

const invokeMock = invoke as ReturnType<typeof vi.fn>;
const listenMock = listen as ReturnType<typeof vi.fn>;

const SAMPLE: ContactsFile = {
  schema_version: 1,
  contacts: [
    {
      id: 'c1',
      name: 'Vera Knox',
      callsign: 'KE7VAR',
      email: 'ke7var@winlink.org',
      created_at: '2026-06-01T00:00:00+00:00',
      updated_at: '2026-06-01T00:00:00+00:00',
    },
  ],
  groups: [
    {
      id: 'g1',
      name: 'ARES Net',
      members: [
        { type: 'contact', contact_id: 'c1' },
        { type: 'raw', callsign: 'W6ABC' },
      ],
      created_at: '2026-06-01T00:00:00+00:00',
      updated_at: '2026-06-01T00:00:00+00:00',
    },
  ],
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
  // Default: contacts_read returns the sample file; mutations resolve.
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'contacts_read') return Promise.resolve(SAMPLE);
    return Promise.resolve(undefined);
  });
});

describe('useContacts', () => {
  it('exposes contacts/groups derived from contacts_read', async () => {
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(newQc()) });

    await waitFor(() => expect(result.current.isLoading).toBe(false));

    expect(invokeMock).toHaveBeenCalledWith('contacts_read');
    expect(result.current.contacts).toEqual(SAMPLE.contacts);
    expect(result.current.groups).toEqual(SAMPLE.groups);
  });

  it('defaults contacts/groups to [] before the read resolves / on a null read', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'contacts_read') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(newQc()) });

    // Synchronously (first render, before the query resolves) the arrays are [].
    expect(result.current.contacts).toEqual([]);
    expect(result.current.groups).toEqual([]);

    await waitFor(() => expect(result.current.isLoading).toBe(false));
    expect(result.current.contacts).toEqual([]);
    expect(result.current.groups).toEqual([]);
  });

  it('upsertContact invokes contact_upsert with {contact} then invalidates [contacts]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    const contact = SAMPLE.contacts[0];
    await result.current.upsertContact(contact);

    expect(invokeMock).toHaveBeenCalledWith('contact_upsert', { contact });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY });
  });

  it('deleteContact invokes contact_delete with {id} then invalidates [contacts]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    await result.current.deleteContact('c1');

    expect(invokeMock).toHaveBeenCalledWith('contact_delete', { id: 'c1' });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY });
  });

  it('upsertGroup invokes group_upsert with {group} then invalidates [contacts]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    const group = SAMPLE.groups[0];
    await result.current.upsertGroup(group);

    expect(invokeMock).toHaveBeenCalledWith('group_upsert', { group });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY });
  });

  it('deleteGroup invokes group_delete with {id} then invalidates [contacts]', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(qc) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));
    invalidateSpy.mockClear();

    await result.current.deleteGroup('g1');

    expect(invokeMock).toHaveBeenCalledWith('group_delete', { id: 'g1' });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY });
  });

  it('does not reject when a mutation invoke fails (errors are non-blocking)', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'contacts_read') return Promise.resolve(SAMPLE);
      return Promise.reject(new Error('backend down'));
    });
    const { result } = renderHook(() => useContacts(), { wrapper: wrapperWith(newQc()) });
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    // The mutation swallows the rejection (.catch(() => {})) — it must resolve.
    await expect(result.current.upsertContact(SAMPLE.contacts[0])).resolves.toBeUndefined();
  });

  it('subscribes to contacts:changed and invalidates [contacts] when it fires (H9)', async () => {
    const qc = newQc();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    renderHook(() => useContacts(), { wrapper: wrapperWith(qc) });

    await waitFor(() => {
      expect(listenMock).toHaveBeenCalledWith(CONTACTS_CHANGED_EVENT, expect.any(Function));
      expect(capturedContactsChangedHandler).not.toBeNull();
    });

    invalidateSpy.mockClear();
    // Simulate the backend emitting contacts:changed (e.g. an edit in the main
    // window reaching an open Compose window).
    capturedContactsChangedHandler?.({ payload: undefined });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: CONTACTS_QUERY_KEY }),
    );
  });
});
