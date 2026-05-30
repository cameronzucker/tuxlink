import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useModemStatus } from './useModemStatus';
import { STOPPED, type ModemStatus } from './types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const listenMock = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, cb: (e: { payload: ModemStatus }) => void) =>
    listenMock(event, cb),
}));

describe('useModemStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockResolvedValue(() => {}); // unsubscribe fn
  });

  it('starts with STOPPED and loading=true, fetches initial via modem_get_status', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(STOPPED);
    const { result } = renderHook(() => useModemStatus());
    expect(result.current.status.state).toBe('stopped');
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(invoke).toHaveBeenCalledWith('modem_get_status');
    expect(listenMock).toHaveBeenCalledWith('modem:status', expect.any(Function));
  });

  it('updates on modem:status events', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(STOPPED);
    let captured: ((e: { payload: ModemStatus }) => void) | null = null;
    listenMock.mockImplementation((_event: string, cb) => {
      captured = cb;
      return Promise.resolve(() => {});
    });
    const { result } = renderHook(() => useModemStatus());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(listenMock).toHaveBeenCalledWith('modem:status', expect.any(Function));
    act(() => {
      captured!({ payload: { ...STOPPED, state: 'connecting' } });
    });
    expect(result.current.status.state).toBe('connecting');
  });

  it('sets error and clears loading when modem_get_status rejects', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('backend not ready'));
    const { result } = renderHook(() => useModemStatus());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.status.state).toBe('stopped');
    expect(result.current.error).toContain('backend not ready');
  });
});
