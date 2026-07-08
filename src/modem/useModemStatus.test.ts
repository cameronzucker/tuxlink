import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useModemStatus, connectionToPanelMode, useActiveModemMode } from './useModemStatus';
import { STOPPED, type ModemStatus } from './types';
import type { ConnectionKey } from '../connections/sessionTypes';

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

describe('connectionToPanelMode (tuxlink-7ppfq Contract 2)', () => {
  it('maps a vara-hf selection to a vara-hf panel mode', () => {
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'vara-hf' })).toEqual({
      kind: 'vara-hf',
      intent: 'cms',
    });
  });

  it('maps an ardop-hf selection, carrying the intent from sessionType', () => {
    expect(connectionToPanelMode({ sessionType: 'p2p', protocol: 'ardop-hf' })).toEqual({
      kind: 'ardop-hf',
      intent: 'p2p',
    });
  });

  it('returns null for non-radio protocols', () => {
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'telnet' })).toBeNull();
    expect(connectionToPanelMode({ sessionType: 'cms', protocol: 'packet' })).toBeNull();
  });
});

describe('useActiveModemMode (tuxlink-7ppfq Contract 2)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listenMock.mockResolvedValue(() => {});
  });

  const vara: ConnectionKey = { sessionType: 'cms', protocol: 'vara-hf' };

  it('returns the selected panel mode while the modem is live', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    // A non-stopped state means the modem is live.
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({ ...STOPPED, state: 'connecting' });
    const { result } = renderHook(() => useActiveModemMode(vara));
    await waitFor(() => expect(result.current).toEqual({ kind: 'vara-hf', intent: 'cms' }));
  });

  it('returns null when the modem is stopped, regardless of selection', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(STOPPED);
    const { result } = renderHook(() => useActiveModemMode(vara));
    // Stays null across the initial fetch.
    await waitFor(() => expect(result.current).toBeNull());
  });

  it('falls back to ardop-hf when live but the selection is not a radio protocol', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({ ...STOPPED, state: 'connecting' });
    const telnet: ConnectionKey = { sessionType: 'cms', protocol: 'telnet' };
    const { result } = renderHook(() => useActiveModemMode(telnet));
    // A live (ARDOP) modem with a non-radio selection still surfaces the ARDOP panel.
    await waitFor(() => expect(result.current).toEqual({ kind: 'ardop-hf', intent: 'cms' }));
  });
});
