import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ModemLinkFields } from '../radio/sections/ModemLinkSection';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

import { useAprsConnectSequence } from './useAprsConnectSequence';

// A minimal ModemLinkFields stand-in — the sequence only forwards it to setLink
// verbatim, so its exact shape is irrelevant to these tests.
const FIELDS = { linkKind: 'Tcp' } as unknown as ModemLinkFields;

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined);
});

describe('useAprsConnectSequence', () => {
  it('KISS connect (non-UvproNative) is a single aprs_listen_start step', async () => {
    const setLink = vi.fn(async () => {});
    const { result } = renderHook(() => useAprsConnectSequence('Tcp', setLink));
    await act(async () => {
      await result.current.connect();
    });
    expect(mockInvoke).toHaveBeenCalledWith('aprs_listen_start');
    expect(mockInvoke).not.toHaveBeenCalledWith('uvpro_connect', {});
  });

  it('UvproNative connect is a two-step uvpro_connect → aprs_listen_start', async () => {
    const setLink = vi.fn(async () => {});
    const { result } = renderHook(() => useAprsConnectSequence('UvproNative', setLink));
    await act(async () => {
      await result.current.connect();
    });
    const order = mockInvoke.mock.calls.map((c) => c[0]);
    expect(order).toEqual(['uvpro_connect', 'aprs_listen_start']);
  });

  it('rolls the UV-Pro session back when the second step (aprs_listen_start) fails', async () => {
    const setLink = vi.fn(async () => {});
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'aprs_listen_start') return Promise.reject(new Error('no active identity'));
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useAprsConnectSequence('UvproNative', setLink));
    await act(async () => {
      await expect(result.current.connect()).rejects.toThrow('no active identity');
    });
    // The rollback disconnects the session a failed listen-start would otherwise strand.
    expect(mockInvoke).toHaveBeenCalledWith('uvpro_disconnect');
  });

  it('teardown keys off the transport the listener actually came up on, not the current picker', async () => {
    const setLink = vi.fn(async () => {});
    const { result, rerender } = renderHook(
      ({ kind }) => useAprsConnectSequence(kind, setLink),
      { initialProps: { kind: 'UvproNative' as const } },
    );
    await act(async () => {
      await result.current.connect();
    });
    // The operator edits the picker to Tcp AFTER the UV-Pro listener is live.
    rerender({ kind: 'Tcp' as unknown as 'UvproNative' });
    mockInvoke.mockClear();
    await act(async () => {
      await result.current.disconnect();
    });
    // Disconnect still tears down the UV-Pro session (the live transport), even
    // though the picker now reads Tcp.
    expect(mockInvoke).toHaveBeenCalledWith('uvpro_disconnect');
  });

  it('KISS teardown does not touch the UV-Pro session', async () => {
    const setLink = vi.fn(async () => {});
    const { result } = renderHook(() => useAprsConnectSequence('Tcp', setLink));
    await act(async () => {
      await result.current.connect();
    });
    mockInvoke.mockClear();
    await act(async () => {
      await result.current.disconnect();
    });
    expect(mockInvoke).toHaveBeenCalledWith('aprs_listen_stop');
    expect(mockInvoke).not.toHaveBeenCalledWith('uvpro_disconnect');
  });

  it('awaits the pending link-persist before arming the listener (Codex 2026-06-14 P1 race)', async () => {
    let resolvePersist!: () => void;
    const persist = new Promise<void>((r) => {
      resolvePersist = r;
    });
    const setLink = vi.fn(() => persist);
    const { result } = renderHook(() => useAprsConnectSequence('Tcp', setLink));
    act(() => {
      result.current.onLinkChange(FIELDS);
    });
    let connectDone = false;
    await act(async () => {
      const p = result.current.connect().then(() => {
        connectDone = true;
      });
      await Promise.resolve();
      // The persist has NOT resolved yet — the listener must not be armed.
      expect(mockInvoke).not.toHaveBeenCalledWith('aprs_listen_start');
      resolvePersist();
      await p;
    });
    expect(connectDone).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('aprs_listen_start');
  });

  it('drives the connecting flag true while in flight and false when settled', async () => {
    const setLink = vi.fn(async () => {});
    const { result } = renderHook(() => useAprsConnectSequence('Tcp', setLink));
    expect(result.current.connecting).toBe(false);
    await act(async () => {
      await result.current.connect();
    });
    expect(result.current.connecting).toBe(false);
  });
});
