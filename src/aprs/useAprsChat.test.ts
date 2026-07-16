import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const handlers: Record<string, (e: { payload: unknown }) => void> = {};
// `emitMock` backs the snapshot-handshake tests (spec §7) — mirrors
// useAprsPositions.test.ts's listen/emit mock pattern.
const emitMock = vi.fn((_name: string, _payload?: unknown) => Promise.resolve());
vi.mock('@tauri-apps/api/event', () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb;
    return Promise.resolve(() => { delete handlers[name]; });
  },
  emit: (name: string, payload?: unknown) => emitMock(name, payload),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));

import { useAprsChat } from './useAprsChat';

beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k];
  emitMock.mockClear();
});

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

  // Backend own-send echo (tuxlink-dmwte task 10, spec §7). Every window
  // consumes `aprs-message:sent` so its feed is reconstructible from events
  // alone; the SENDING window deduplicates the echo against its optimistic
  // local append by msgid.
  describe('own-send echo (aprs-message:sent)', () => {
    it('dedupes the echo against a local optimistic append by msgid (send → 1 message)', async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('A1');
      const { result } = renderHook(() => useAprsChat());
      await act(async () => {});
      await act(async () => { await result.current.send('KK6XYZ', 'hi'); });
      // The backend echoes the same acceptance the optimistic append already recorded.
      act(() => {
        handlers['aprs-message:sent']?.({
          payload: { msgid: 'A1', addressee: 'KK6XYZ', text: 'hi', at_ms: 1_700_000_000_000 },
        });
      });
      expect(result.current.messages.filter((m) => m.msgid === 'A1')).toHaveLength(1);
    });

    it('dedupes when the echo arrives BEFORE send\'s invoke resolves (echo-first race)', async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      let resolveInvoke!: (v: string) => void;
      (invoke as ReturnType<typeof vi.fn>).mockImplementationOnce(
        () => new Promise<string>((resolve) => { resolveInvoke = resolve; }),
      );
      const { result } = renderHook(() => useAprsChat());
      await act(async () => {});
      let sendPromise!: Promise<string>;
      act(() => {
        sendPromise = result.current.send('KK6XYZ', 'hi');
      });
      // The backend's own-send echo lands on the wire and is handled BEFORE
      // send's `invoke` call resolves back into this window's continuation —
      // the exact ordering the msgid guard on the optimistic append exists to
      // close (see the guard's comment in useAprsChat.ts).
      act(() => {
        handlers['aprs-message:sent']?.({
          payload: { msgid: 'A1', addressee: 'KK6XYZ', text: 'hi', at_ms: 1_700_000_000_000 },
        });
      });
      await act(async () => {
        resolveInvoke('A1');
        await sendPromise;
      });
      expect(result.current.messages.filter((m) => m.msgid === 'A1')).toHaveLength(1);
    });

    it('appends an echo for a send that happened in another window (from: me, to, at from at_ms)', async () => {
      const { result } = renderHook(() => useAprsChat());
      await act(async () => {});
      act(() => {
        handlers['aprs-message:sent']?.({
          payload: { msgid: 'X9', addressee: 'W7RPT-9', text: 'roger', at_ms: 1_700_000_000_123 },
        });
      });
      expect(result.current.messages).toHaveLength(1);
      const m = result.current.messages[0];
      expect(m.direction).toBe('out');
      expect(m.from).toBe('me');
      expect(m.to).toBe('W7RPT-9');
      expect(m.state).toBe('sent');
      expect(m.text).toBe('roger');
      expect(m.msgid).toBe('X9');
      expect(m.at).toBe(1_700_000_000_123);
    });

    it('maps a blank addressee echo to a broadcast (to === null)', async () => {
      const { result } = renderHook(() => useAprsChat());
      await act(async () => {});
      act(() => {
        handlers['aprs-message:sent']?.({
          payload: { msgid: 'B1', addressee: '', text: 'CQ', at_ms: 42 },
        });
      });
      expect(result.current.messages[0].to).toBeNull();
    });

    it('a delivery-state event applies to an echo-appended message', async () => {
      const { result } = renderHook(() => useAprsChat());
      await act(async () => {});
      act(() => {
        handlers['aprs-message:sent']?.({
          payload: { msgid: 'C3', addressee: 'KK6XYZ', text: 'hi', at_ms: 1 },
        });
      });
      act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'C3', state: 'acked' } }); });
      const m = result.current.messages.find((x) => x.msgid === 'C3');
      expect(m?.state).toBe('acked');
      expect(typeof m?.ackedAt).toBe('number');
    });
  });

  // Cross-window snapshot handshake (tuxlink-dmwte task 10, spec §7) — mirrors
  // useAprsPositions' host/client mechanics with the same retry amendment.
  describe('snapshot handshake (spec §7)', () => {
    it('omits the handshake entirely when snapshotRole is not given (existing callers unaffected)', async () => {
      renderHook(() => useAprsChat());
      await act(async () => {});
      expect(handlers['aprs-chat:request-snapshot']).toBeUndefined();
      expect(handlers['aprs-chat:snapshot']).toBeUndefined();
      expect(emitMock).not.toHaveBeenCalled();
    });

    it('host answers a snapshot request with its current feed', async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('M1');
      const { result } = renderHook(() => useAprsChat({ snapshotRole: 'host' }));
      await act(async () => {});
      await act(async () => { await result.current.send('KK6XYZ', 'hi'); });
      await waitFor(() => expect(handlers['aprs-chat:request-snapshot']).toBeDefined());
      emitMock.mockClear();
      act(() => handlers['aprs-chat:request-snapshot']({ payload: undefined }));
      expect(emitMock).toHaveBeenCalledWith(
        'aprs-chat:snapshot',
        expect.arrayContaining([expect.objectContaining({ msgid: 'M1' })]),
      );
    });

    it('client requests a snapshot on mount and seeds from the reply', async () => {
      const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
      await waitFor(() => expect(handlers['aprs-chat:snapshot']).toBeDefined());
      expect(emitMock).toHaveBeenCalledWith('aprs-chat:request-snapshot', undefined);
      const snap = [
        { id: 'S1', direction: 'in', from: 'KE7ABC', to: null, text: 'hello', kind: 'message', msgid: 'S1', at: 10 },
      ];
      act(() => handlers['aprs-chat:snapshot']({ payload: snap }));
      expect(result.current.messages.map((m) => m.id)).toContain('S1');
    });

    it('merge keeps the more-progressed delivery state (a snapshot sent must not clobber a live acked)', async () => {
      const { invoke } = await import('@tauri-apps/api/core');
      (invoke as ReturnType<typeof vi.fn>).mockResolvedValueOnce('P7');
      const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
      await waitFor(() => expect(handlers['aprs-chat:snapshot']).toBeDefined());
      // A live send + ack lands BEFORE the (staler) snapshot reply arrives.
      await act(async () => { await result.current.send('KK6XYZ', 'hi'); });
      act(() => { handlers['aprs-message:state']?.({ payload: { msgid: 'P7', state: 'acked' } }); });
      act(() => handlers['aprs-chat:snapshot']({
        payload: [{ id: 'P7', direction: 'out', from: 'me', to: 'KK6XYZ', text: 'hi', kind: 'message', msgid: 'P7', state: 'sent', at: 5 }],
      }));
      const m = result.current.messages.find((x) => x.id === 'P7');
      expect(m?.state).toBe('acked'); // the live terminal state wins over the stale snapshot
      expect(result.current.messages.filter((x) => x.id === 'P7')).toHaveLength(1);
    });

    // Cross-window duplicate rows for msgid-less messages (review loop-4 F1).
    // An inbound message WITHOUT a msgid heard LIVE by a freshly-popped client
    // window ALSO arrives in the host's snapshot under the HOST's local id.
    // Deduping by `.id` alone can't collapse them (the two windows minted
    // different local ids), so mergeSnapshot adds a content-identity fallback
    // for msgid-less rows: same from/text/to/direction AND `at` within a small
    // tolerance ⇒ one row (both windows stamp `at` with their own Date.now()
    // when they heard the same broadcast, so the two stamps are close but not
    // identical — tolerance, not exact match).
    describe('msgid-less cross-window dedup (loop-4 F1)', () => {
      it('collapses a live msgid-less row and its snapshot twin (different local ids) into ONE', async () => {
        const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
        await waitFor(() => expect(handlers['aprs-chat:snapshot']).toBeDefined());
        // Heard LIVE by this client window (mints a per-window local id, at=now).
        act(() => {
          handlers['aprs-message:new']?.({
            payload: { sender: 'KE7ABC', addressee: '', text: 'net check-in', msgid: null },
          });
        });
        const liveAt = result.current.messages[0].at;
        // The SAME frame, as the host heard it: no msgid, host's own local id,
        // `at` a hair off the client's stamp (same broadcast, two windows).
        act(() =>
          handlers['aprs-chat:snapshot']({
            payload: [
              {
                id: 'host-local-42',
                direction: 'in',
                from: 'KE7ABC',
                to: null,
                text: 'net check-in',
                kind: 'message',
                msgid: null,
                at: liveAt + 40,
              },
            ],
          }),
        );
        expect(result.current.messages).toHaveLength(1);
      });

      it('keeps two genuinely-distinct msgid-less messages (same text, at beyond tolerance) as TWO rows', async () => {
        const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
        await waitFor(() => expect(handlers['aprs-chat:snapshot']).toBeDefined());
        act(() => {
          handlers['aprs-message:new']?.({
            payload: { sender: 'KE7ABC', addressee: '', text: 'CQ', msgid: null },
          });
        });
        const liveAt = result.current.messages[0].at;
        // Same text, but heard 60 s earlier (well past the dedup tolerance) — a
        // real repeat of an unacked frame, not the same frame double-counted.
        act(() =>
          handlers['aprs-chat:snapshot']({
            payload: [
              {
                id: 'host-local-7',
                direction: 'in',
                from: 'KE7ABC',
                to: null,
                text: 'CQ',
                kind: 'message',
                msgid: null,
                at: liveAt - 60_000,
              },
            ],
          }),
        );
        expect(result.current.messages).toHaveLength(2);
      });

      it('keeps two msgid-less messages with same text but different senders as TWO rows', async () => {
        const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
        await waitFor(() => expect(handlers['aprs-chat:snapshot']).toBeDefined());
        act(() => {
          handlers['aprs-message:new']?.({
            payload: { sender: 'KE7ABC', addressee: '', text: 'roger', msgid: null },
          });
        });
        const liveAt = result.current.messages[0].at;
        act(() =>
          handlers['aprs-chat:snapshot']({
            payload: [
              {
                id: 'host-local-9',
                direction: 'in',
                from: 'W7RPT-9',
                to: null,
                text: 'roger',
                kind: 'message',
                msgid: null,
                at: liveAt + 10,
              },
            ],
          }),
        );
        expect(result.current.messages).toHaveLength(2);
      });
    });

    describe('retry amendment (250ms cadence / 3s give-up)', () => {
      beforeEach(() => { vi.useFakeTimers(); });
      afterEach(() => { vi.useRealTimers(); });

      it('re-emits the request every 250ms until the reply arrives, then stops', async () => {
        const { result } = renderHook(() => useAprsChat({ snapshotRole: 'client' }));
        await act(async () => { await Promise.resolve(); await Promise.resolve(); });
        expect(handlers['aprs-chat:snapshot']).toBeDefined();
        await act(async () => { vi.advanceTimersByTime(600); });
        const requestCalls = emitMock.mock.calls.filter((c) => c[0] === 'aprs-chat:request-snapshot');
        expect(requestCalls.length).toBeGreaterThanOrEqual(2);
        act(() => handlers['aprs-chat:snapshot']({
          payload: [{ id: 'R1', direction: 'in', from: 'KE7ABC', to: null, text: 'x', kind: 'message', msgid: 'R1', at: 1 }],
        }));
        expect(result.current.messages.map((m) => m.id)).toContain('R1');
        emitMock.mockClear();
        await act(async () => { vi.advanceTimersByTime(1000); });
        expect(emitMock.mock.calls.filter((c) => c[0] === 'aprs-chat:request-snapshot')).toHaveLength(0);
      });

      it('gives up cleanly after 3s with no reply', async () => {
        renderHook(() => useAprsChat({ snapshotRole: 'client' }));
        await act(async () => { await Promise.resolve(); await Promise.resolve(); });
        await act(async () => { vi.advanceTimersByTime(3000); });
        emitMock.mockClear();
        await act(async () => { vi.advanceTimersByTime(1000); });
        expect(emitMock.mock.calls.filter((c) => c[0] === 'aprs-chat:request-snapshot')).toHaveLength(0);
      });
    });
  });
});
