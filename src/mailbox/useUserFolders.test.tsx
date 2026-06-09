import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  useUserFolders,
  useCreateUserFolder,
  useDeleteUserFolder,
  useMoveUserFolder,
} from './useUserFolders';
import type { UserFolder } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
}

const SAMPLE: UserFolder[] = [
  { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-02T12:00:00Z' },
];

describe('useUserFolders', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('fetches the user-folder list via the user_folders_list command', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(SAMPLE);
    const { result } = renderHook(() => useUserFolders(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.folders).toEqual(SAMPLE));
    expect(invoke).toHaveBeenCalledWith('user_folders_list');
  });

  it('returns [] before the first response', () => {
    vi.mocked(invoke).mockReturnValueOnce(new Promise(() => {})); // never resolves
    const { result } = renderHook(() => useUserFolders(), { wrapper: wrapper() });
    expect(result.current.folders).toEqual([]);
    expect(result.current.isLoading).toBe(true);
  });
});

describe('useCreateUserFolder', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('invokes folder_create with displayName + parentSlug (top-level)', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(SAMPLE[0]);
    const { result } = renderHook(() => useCreateUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      const folder = await result.current.mutateAsync({ displayName: 'ARES Drills' });
      expect(folder).toEqual(SAMPLE[0]);
    });
    expect(invoke).toHaveBeenCalledWith('folder_create', {
      displayName: 'ARES Drills',
      parentSlug: undefined,
    });
  });

  it('forwards parentSlug when creating a subfolder', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...SAMPLE[0], parentSlug: 'nets' });
    const { result } = renderHook(() => useCreateUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      await result.current.mutateAsync({ displayName: 'ARES', parentSlug: 'nets' });
    });
    expect(invoke).toHaveBeenCalledWith('folder_create', { displayName: 'ARES', parentSlug: 'nets' });
  });

  it('propagates a UiError::Rejected from the backend', async () => {
    const err = { kind: 'Rejected', detail: "'Inbox' is reserved for a system folder" };
    vi.mocked(invoke).mockRejectedValueOnce(err);
    const { result } = renderHook(() => useCreateUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      await expect(result.current.mutateAsync({ displayName: 'Inbox' })).rejects.toEqual(err);
    });
  });
});

describe('useMoveUserFolder', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('invokes folder_move with slug + parentSlug (nest)', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...SAMPLE[0], parentSlug: 'nets' });
    const { result } = renderHook(() => useMoveUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      await result.current.mutateAsync({ slug: 'weather', parentSlug: 'nets' });
    });
    expect(invoke).toHaveBeenCalledWith('folder_move', { slug: 'weather', parentSlug: 'nets' });
  });

  it('promotes to top level with undefined parentSlug', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(SAMPLE[0]);
    const { result } = renderHook(() => useMoveUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      await result.current.mutateAsync({ slug: 'ares' });
    });
    expect(invoke).toHaveBeenCalledWith('folder_move', { slug: 'ares', parentSlug: undefined });
  });
});

describe('useDeleteUserFolder', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('invokes folder_delete with slug + onMessages', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useDeleteUserFolder(), { wrapper: wrapper() });
    await act(async () => {
      await result.current.mutateAsync({ slug: 'ares-drills', onMessages: 'move_to_inbox' });
    });
    expect(invoke).toHaveBeenCalledWith('folder_delete', {
      slug: 'ares-drills',
      onMessages: 'move_to_inbox',
    });
  });
});
