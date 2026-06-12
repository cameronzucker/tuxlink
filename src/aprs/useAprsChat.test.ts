import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => { delete handlers[name]; });
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));

import { useAprsChat } from './useAprsChat';

describe('useAprsChat', () => {
  it('adds an inbound message into the sender thread', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    act(() => { handlers['aprs-message:new']?.({ payload: { sender: 'KK6XYZ', text: 'ping', msgid: '04' } }); });
    expect(result.current.threads['KK6XYZ']).toBeDefined();
    expect(result.current.threads['KK6XYZ'].messages.at(-1)?.text).toBe('ping');
    expect(result.current.threads['KK6XYZ'].messages.at(-1)?.direction).toBe('in');
  });

  it('inserts an outgoing bubble keyed by the backend-minted msgid and transitions it to acked', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello'); });
    expect(result.current.threads['KK6XYZ'].messages.find((x) => x.msgid === 'A1')?.state).toBe('sent');
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    expect(result.current.threads['KK6XYZ'].messages.find((x) => x.msgid === 'A1')?.state).toBe('acked');
  });

  it('does NOT insert a bubble when send is rejected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('too many messages pending'));
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello').catch(() => {}); });
    expect(result.current.threads['KK6XYZ']?.messages ?? []).toHaveLength(0);
  });
});
