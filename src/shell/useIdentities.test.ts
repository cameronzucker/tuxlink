import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the Tauri `invoke` boundary (mirrors src/mailbox/useMailbox.test.ts).
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  useIdentityList,
  useActiveIdentity,
  useIdentitySwitch,
  IDENTITY_LIST_QUERY_KEY,
  IDENTITY_ACTIVE_QUERY_KEY,
} from './useIdentities';
import { parseIdentityError } from './identityTypes';
import type { IdentityListDto, ActiveIdentityDto } from './identityTypes';

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

function freshClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('useIdentityList', () => {
  it('calls invoke("identity_list") and exposes full / tactical arrays', async () => {
    const list: IdentityListDto = {
      full: [
        {
          callsign: 'W1ABC',
          label: 'Personal',
          has_cms_account: true,
          cms_registered: true,
          needs_auth: false,
        },
      ],
      tactical: [{ label: 'EOC-3', parent: 'W1ABC', cms_badge: 'registered' }],
      last_selected: 'EOC-3',
    };
    invokeMock.mockResolvedValueOnce(list);

    const { result } = renderHook(() => useIdentityList(), { wrapper: wrapperWith(freshClient()) });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(invokeMock).toHaveBeenCalledWith('identity_list');
    expect(result.current.data?.full[0].callsign).toBe('W1ABC');
    expect(result.current.data?.tactical[0].parent).toBe('W1ABC');
    expect(result.current.data?.last_selected).toBe('EOC-3');
  });
});

describe('useActiveIdentity', () => {
  it('calls invoke("identity_active") and exposes the active DTO', async () => {
    const active: ActiveIdentityDto = {
      mycall: 'W1ABC',
      address_as: 'EOC-3',
      is_tactical: true,
    };
    invokeMock.mockResolvedValueOnce(active);

    const { result } = renderHook(() => useActiveIdentity(), { wrapper: wrapperWith(freshClient()) });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(invokeMock).toHaveBeenCalledWith('identity_active');
    expect(result.current.data?.mycall).toBe('W1ABC');
    expect(result.current.data?.is_tactical).toBe(true);
  });

  it('tolerates a null active session', async () => {
    invokeMock.mockResolvedValueOnce(null);
    const { result } = renderHook(() => useActiveIdentity(), { wrapper: wrapperWith(freshClient()) });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });
});

describe('useIdentitySwitch', () => {
  it('authenticates a FULL identity via identity_authenticate with tacticalLabel null', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useIdentitySwitch(), { wrapper: wrapperWith(freshClient()) });

    await result.current.mutateAsync({ callsign: 'W7XYZ', credential: 'pw', tacticalLabel: null });

    expect(invokeMock).toHaveBeenCalledWith('identity_authenticate', {
      callsign: 'W7XYZ',
      credential: 'pw',
      tacticalLabel: null,
    });
  });

  it('authenticates a tactical via identity_authenticate with parent callsign + label', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useIdentitySwitch(), { wrapper: wrapperWith(freshClient()) });

    await result.current.mutateAsync({
      callsign: 'W1ABC',
      credential: 'pw',
      tacticalLabel: 'EOC-3',
    });

    expect(invokeMock).toHaveBeenCalledWith('identity_authenticate', {
      callsign: 'W1ABC',
      credential: 'pw',
      tacticalLabel: 'EOC-3',
    });
  });

  it('invalidates the list and active queries on success', async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    const qc = freshClient();
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { result } = renderHook(() => useIdentitySwitch(), { wrapper: wrapperWith(qc) });

    await result.current.mutateAsync({ callsign: 'W7XYZ', credential: 'pw', tacticalLabel: null });

    await waitFor(() => {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: IDENTITY_LIST_QUERY_KEY });
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: IDENTITY_ACTIVE_QUERY_KEY });
    });
  });

  it('surfaces a rejection (parseable as an identity error) on auth failure', async () => {
    invokeMock.mockRejectedValueOnce({ kind: 'AuthFailed', detail: { reason: 'bad credential' } });
    const { result } = renderHook(() => useIdentitySwitch(), { wrapper: wrapperWith(freshClient()) });

    await expect(
      result.current.mutateAsync({ callsign: 'W7XYZ', credential: 'nope', tacticalLabel: null }),
    ).rejects.toBeTruthy();

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(parseIdentityError(result.current.error)).toBe('bad credential');
  });
});

describe('parseIdentityError', () => {
  it('extracts the reason from AuthFailed', () => {
    expect(parseIdentityError({ kind: 'AuthFailed', detail: { reason: '401' } })).toBe('401');
  });
  it('extracts the detail from NotFound / Rejected / NotConfigured', () => {
    expect(parseIdentityError({ kind: 'NotFound', detail: 'no such tactical' })).toBe('no such tactical');
    expect(parseIdentityError({ kind: 'Rejected', detail: 'duplicate' })).toBe('duplicate');
    expect(parseIdentityError({ kind: 'NotConfigured', detail: 'no backend' })).toBe('no backend');
  });
  it('extracts the detail from Internal', () => {
    expect(parseIdentityError({ kind: 'Internal', detail: { detail: 'boom' } })).toBe('boom');
  });
  it('extracts the reason from Unavailable', () => {
    expect(parseIdentityError({ kind: 'Unavailable', detail: { reason: 'offline' } })).toBe('offline');
  });
  it('falls back to Error.message / String for non-UiError throws', () => {
    expect(parseIdentityError(new Error('plain'))).toBe('plain');
    expect(parseIdentityError('stringy')).toBe('stringy');
  });
});
