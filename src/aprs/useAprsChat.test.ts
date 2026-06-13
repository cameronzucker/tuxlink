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

describe('useAprsChat (open channel)', () => {
  it('appends an inbound directed message to the flat feed', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    act(() => {
      handlers['aprs-message:new']?.({
        payload: { sender: 'KK6XYZ', addressee: 'NN7LE-9', text: 'ping', msgid: '04' },
      });
    });
    const last = result.current.messages.at(-1);
    expect(last?.text).toBe('ping');
    expect(last?.direction).toBe('in');
    expect(last?.from).toBe('KK6XYZ');
    expect(last?.to).toBe('NN7LE-9');
  });

  it('maps a blank addressee to a broadcast (to === null)', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    act(() => {
      handlers['aprs-message:new']?.({
        payload: { sender: 'KK6XYZ', addressee: '', text: 'CQ net', msgid: null },
      });
    });
    expect(result.current.messages.at(-1)?.to).toBeNull();
  });

  it('derives heardStations most-recently-heard first, deduped', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    act(() => {
      handlers['aprs-message:new']?.({ payload: { sender: 'AAA', addressee: '', text: '1', msgid: null } });
      handlers['aprs-message:new']?.({ payload: { sender: 'BBB', addressee: '', text: '2', msgid: null } });
      handlers['aprs-message:new']?.({ payload: { sender: 'AAA', addressee: '', text: '3', msgid: null } });
    });
    const calls = result.current.heardStations.map((s) => s.call);
    // AAA was heard most recently; both deduped to a single entry each.
    expect(calls).toEqual(['AAA', 'BBB']);
  });

  it('sends a directed message and reconciles state to acked', async () => {
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello'); });
    const sent = result.current.messages.find((m) => m.msgid === 'A1');
    expect(sent?.direction).toBe('out');
    expect(sent?.to).toBe('KK6XYZ');
    expect(sent?.state).toBe('sent');
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A1', state: 'acked' } }); });
    expect(result.current.messages.find((m) => m.msgid === 'A1')?.state).toBe('acked');
  });

  it('treats an empty recipient as a broadcast (call: null, to: null)', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('b7');
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('', 'CQ'); });
    expect(invoke).toHaveBeenCalledWith('aprs_send', { call: null, text: 'CQ' });
    const sent = result.current.messages.find((m) => m.id === 'b7');
    expect(sent?.to).toBeNull();
  });

  it('passes a callsign recipient through as call', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('A2');
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('W7RPT-9', 'hi'); });
    expect(invoke).toHaveBeenCalledWith('aprs_send', { call: 'W7RPT-9', text: 'hi' });
  });

  it('does NOT append a message when send is rejected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('too many messages pending'));
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello').catch(() => {}); });
    expect(result.current.messages).toHaveLength(0);
  });

  it('stamps ackedAt when a message transitions to acked', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('A3');
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    await act(async () => { await result.current.send('KK6XYZ', 'hello'); });
    act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'A3', state: 'acked' } }); });
    const msg = result.current.messages.find((m) => m.msgid === 'A3');
    expect(msg?.state).toBe('acked');
    expect(typeof msg?.ackedAt).toBe('number');
  });

  it('setConfig invokes aprs_config_set with the dto key', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockClear();
    const { result } = renderHook(() => useAprsChat());
    await act(async () => {});
    const dto = { sourceSsid: 9, tocall: 'APZTUX', path: 'WIDE1-1' };
    await act(async () => { await result.current.setConfig(dto); });
    expect(invoke).toHaveBeenCalledWith('aprs_config_set', { dto });
  });
});
