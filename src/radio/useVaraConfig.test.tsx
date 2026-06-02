// src/radio/useVaraConfig.test.tsx
//
// Unit tests for the VARA config hook. Mocks `@tauri-apps/api/core` so the
// hook's `invoke` calls return deterministic test values without needing a
// Tauri runtime.
//
// Post-tuxlink-6dzo: the hook no longer exposes `loading` — assertions now
// wait on `config` reaching the loaded value (success path) or staying at
// the default (error path).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useVaraConfig, VARA_DEFAULT_CONFIG } from './useVaraConfig';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const defaultLoadedConfig = {
  host: '127.0.0.1',
  cmd_port: 8300,
  data_port: 8301,
  bandwidth_hz: null as number | null,
};

describe('useVaraConfig', () => {
  beforeEach(async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') return defaultLoadedConfig;
      return undefined;
    });
  });

  it('renders the default config before the first load completes', () => {
    const { result } = renderHook(() => useVaraConfig());
    // Synchronous initial render: hook has not awaited config_get_vara yet.
    expect(result.current.config).toEqual(VARA_DEFAULT_CONFIG);
  });

  it('loads the persisted config (config updates to the loaded value)', async () => {
    const nextLoaded = { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2750 };
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') return nextLoaded;
      return undefined;
    });
    const { result } = renderHook(() => useVaraConfig());
    await waitFor(() => {
      expect(result.current.config).toEqual(nextLoaded);
    });
  });

  it('keeps the default when config_get_vara rejects (pre-wizard path)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') throw new Error('NotConfigured');
      return undefined;
    });
    const { result } = renderHook(() => useVaraConfig());
    // After a tick, config should still be the default (the catch path is a no-op).
    await waitFor(() => {
      // Force at least one re-render assertion to elapse the pending invoke
      expect(result.current.config).toEqual(VARA_DEFAULT_CONFIG);
    });
  });

  it('persists writes via config_set_vara and optimistically updates locally', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    const { result } = renderHook(() => useVaraConfig());
    // Wait for the initial load to settle so we can distinguish optimistic
    // updates from load-driven updates.
    await waitFor(() => expect(result.current.config).toEqual(defaultLoadedConfig));

    const next = { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2300 };
    act(() => {
      result.current.setConfig(next);
    });

    expect(result.current.config).toEqual(next);

    await waitFor(() => {
      const setCalls = invokeSpy.mock.calls.filter((c) => c[0] === 'config_set_vara');
      expect(setCalls).toHaveLength(1);
      expect(setCalls[0][1]).toEqual({ value: next });
    });
  });

  it('broadcasts a same-window CustomEvent so peer hooks stay in sync', async () => {
    const { result: a } = renderHook(() => useVaraConfig());
    const { result: b } = renderHook(() => useVaraConfig());
    // Let both hooks settle to the loaded default first.
    await waitFor(() => {
      expect(a.current.config).toEqual(defaultLoadedConfig);
      expect(b.current.config).toEqual(defaultLoadedConfig);
    });

    const next = { host: '192.168.1.50', cmd_port: 8400, data_port: 8401, bandwidth_hz: 500 };
    act(() => {
      a.current.setConfig(next);
    });

    // Hook B receives the local CustomEvent and re-seeds without a backend round-trip.
    await waitFor(() => {
      expect(b.current.config).toEqual(next);
    });
  });

  it('normalizes a missing bandwidth_hz field to null', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') {
        // Backend serialized config WITHOUT bandwidth_hz (skip_serializing_if=None).
        return { host: '127.0.0.1', cmd_port: 8300, data_port: 8301 };
      }
      return undefined;
    });
    const { result } = renderHook(() => useVaraConfig());
    await waitFor(() => expect(result.current.config.bandwidth_hz).toBeNull());
  });
});
