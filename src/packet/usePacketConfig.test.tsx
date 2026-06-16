// src/packet/usePacketConfig.test.tsx
//
// Covers the shared usePacketConfig hook (operator smoke 2026-05-31): loads on
// mount, falls back to ssid=0 when load fails, setSsid persists via
// packet_config_set, and broadcasts a same-window CustomEvent so peer hooks
// stay in sync.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { usePacketConfig } from './usePacketConfig';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

const DEFAULT_CONFIG = {
  ssid: 7,
  listenDefault: true,
  linkKind: 'Tcp' as const,
  tcpHost: '127.0.0.1',
  tcpPort: 8001,
  serialDevice: null,
  serialBaud: null,
  txdelay: 30,
  persistence: 63,
  slotTime: 10,
  paclen: 128,
  maxframe: 4,
  t1Ms: 3000,
  n2Retries: 10,
};

describe('usePacketConfig', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
      return undefined;
    });
  });

  it('loads packet config on mount + exposes ssid', async () => {
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => {
      expect(result.current.config).not.toBeNull();
    });
    expect(result.current.ssid).toBe(7);
  });

  it('falls back to ssid=0 when packet_config_get rejects', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') throw new Error('NotConfigured');
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    // Give the load a chance to settle.
    await new Promise((r) => setTimeout(r, 10));
    expect(result.current.config).toBeNull();
    expect(result.current.ssid).toBe(0);
  });

  it('setSsid persists via packet_config_set with the merged DTO', async () => {
    const core = await import('@tauri-apps/api/core');
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.config).not.toBeNull());
    act(() => {
      result.current.setSsid(10);
    });
    expect(core.invoke).toHaveBeenCalledWith(
      'packet_config_set',
      expect.objectContaining({ dto: expect.objectContaining({ ssid: 10 }) }),
    );
    expect(result.current.ssid).toBe(10);
  });

  it('updates local state when a peer hook broadcasts a config change', async () => {
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.ssid).toBe(7));
    act(() => {
      window.dispatchEvent(
        new CustomEvent('tuxlink:packet-config:change', {
          detail: { ...DEFAULT_CONFIG, ssid: 3 },
        }),
      );
    });
    expect(result.current.ssid).toBe(3);
  });

  it('setLink merges the link fields and persists the full DTO', async () => {
    const core = await import('@tauri-apps/api/core');
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.config).not.toBeNull());
    act(() => {
      result.current.setLink({
        linkKind: 'Bluetooth',
        tcpHost: null,
        tcpPort: null,
        serialDevice: null,
        serialBaud: null,
        btMac: 'AA:BB:CC:DD:EE:FF',
      });
    });
    // Persists the merged DTO: preserves untouched fields (ssid 7), applies the
    // new link fields.
    expect(core.invoke).toHaveBeenCalledWith(
      'packet_config_set',
      expect.objectContaining({
        dto: expect.objectContaining({
          ssid: 7,
          linkKind: 'Bluetooth',
          btMac: 'AA:BB:CC:DD:EE:FF',
          tcpHost: null,
        }),
      }),
    );
    // Optimistic local update reflects the new link.
    expect(result.current.config?.linkKind).toBe('Bluetooth');
  });

  it('setLink is a no-op when config is unloaded', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') throw new Error('NotConfigured');
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    await new Promise((r) => setTimeout(r, 10));
    (core.invoke as ReturnType<typeof vi.fn>).mockClear();
    act(() => {
      result.current.setLink({
        linkKind: 'Tcp',
        tcpHost: '1.2.3.4',
        tcpPort: 8001,
        serialDevice: null,
        serialBaud: null,
        btMac: null,
      });
    });
    expect(core.invoke).not.toHaveBeenCalled();
  });

  it('setLink returns an awaitable promise that resolves once the persist settles', async () => {
    // The connect flow awaits this before aprs_listen_start so the backend reads
    // the just-persisted link, not a stale one (Codex adrev 2026-06-14 P1 race).
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.config).not.toBeNull());
    let p!: Promise<void>;
    act(() => {
      p = result.current.setLink({
        linkKind: 'Tcp',
        tcpHost: '1.2.3.4',
        tcpPort: 8001,
        serialDevice: null,
        serialBaud: null,
        btMac: null,
      });
    });
    await expect(p).resolves.toBeUndefined();
  });

  it('setLink rolls back the optimistic update when the persist rejects (B4)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
      if (cmd === 'packet_config_set') throw new Error('write failed');
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.config).not.toBeNull());
    let p!: Promise<void>;
    act(() => {
      p = result.current.setLink({
        linkKind: 'Bluetooth',
        tcpHost: null,
        tcpPort: null,
        serialDevice: null,
        serialBaud: null,
        btMac: 'AA:BB:CC:DD:EE:FF',
      });
    });
    await act(async () => {
      await p;
    });
    // A rejected persist must NOT leave the UI showing the un-saved Bluetooth
    // link — it reverts to the persisted Tcp truth (tuxlink-hoi1 B4).
    expect(result.current.config?.linkKind).toBe('Tcp');
  });

  it('setSsid rolls back the optimistic update when the persist rejects (B4)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
      if (cmd === 'packet_config_set') throw new Error('write failed');
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.ssid).toBe(7));
    act(() => {
      result.current.setSsid(10);
    });
    // The optimistic 10 reverts to the persisted 7 once the persist rejects.
    await waitFor(() => expect(result.current.ssid).toBe(7));
  });

  it('does NOT roll back a newer successful write when an older write rejects (B4 race guard)', async () => {
    // Codex + Claude adrev: an older rejected persist must not revert the UI to
    // its stale pre-write snapshot once a NEWER write has already landed —
    // otherwise the rollback re-opens the multi-writer clobber this fix targets.
    const core = await import('@tauri-apps/api/core');
    let rejectFirst!: () => void;
    let calls = 0;
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') return DEFAULT_CONFIG;
      if (cmd === 'packet_config_set') {
        calls += 1;
        if (calls === 1) {
          // Write A (link): stays pending until we reject it explicitly, AFTER B.
          return new Promise((_res, rej) => {
            rejectFirst = () => rej(new Error('write A failed'));
          });
        }
        return undefined; // Write B (ssid): succeeds.
      }
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    await waitFor(() => expect(result.current.config).not.toBeNull());
    // Write A — change the link (in flight, not yet rejected).
    act(() => {
      void result.current.setLink({
        linkKind: 'Bluetooth',
        tcpHost: null,
        tcpPort: null,
        serialDevice: null,
        serialBaud: null,
        btMac: 'AA:BB:CC:DD:EE:FF',
      });
    });
    // Write B — change the SSID; this persist succeeds.
    act(() => {
      result.current.setSsid(11);
    });
    await waitFor(() => expect(result.current.ssid).toBe(11));
    // Now A rejects: its rollback must be SUPPRESSED — reverting to the pre-A
    // snapshot would clobber B's ssid=11 and the Bluetooth link.
    await act(async () => {
      rejectFirst();
      await Promise.resolve();
    });
    expect(result.current.ssid).toBe(11);
    expect(result.current.config?.linkKind).toBe('Bluetooth');
  });

  it('setLink (unloaded config) still returns a resolved promise, not undefined', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'packet_config_get') throw new Error('NotConfigured');
      return undefined;
    });
    const { result } = renderHook(() => usePacketConfig());
    await new Promise((r) => setTimeout(r, 10));
    const p = result.current.setLink({
      linkKind: 'Tcp',
      tcpHost: '1.2.3.4',
      tcpPort: 8001,
      serialDevice: null,
      serialBaud: null,
      btMac: null,
    });
    await expect(p).resolves.toBeUndefined();
  });
});
