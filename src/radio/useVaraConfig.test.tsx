// src/radio/useVaraConfig.test.tsx
//
// Unit tests for the VARA config hook. Mocks `@tauri-apps/api/core` so the
// hook's `invoke` calls return deterministic test values without needing a
// Tauri runtime.

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
    // Default: config_get_vara returns the canonical default; everything else undefined.
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') return defaultLoadedConfig;
      return undefined;
    });
  });

  it('renders the default config before the first load completes', () => {
    const { result } = renderHook(() => useVaraConfig());
    // Synchronous initial render: hook has not awaited config_get_vara yet.
    expect(result.current.config).toEqual(VARA_DEFAULT_CONFIG);
    expect(result.current.loading).toBe(true);
  });

  it('loads the persisted config and clears loading', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') {
        return { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2750 };
      }
      return undefined;
    });
    const { result } = renderHook(() => useVaraConfig());
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
    expect(result.current.config).toEqual({
      host: '10.0.0.5',
      cmd_port: 8400,
      data_port: 8401,
      bandwidth_hz: 2750,
    });
  });

  it('falls back to default when config_get_vara rejects (pre-wizard)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_vara') throw new Error('NotConfigured');
      return undefined;
    });
    const { result } = renderHook(() => useVaraConfig());
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });
    expect(result.current.config).toEqual(VARA_DEFAULT_CONFIG);
  });

  it('persists writes via config_set_vara and optimistically updates locally', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    const { result } = renderHook(() => useVaraConfig());
    await waitFor(() => expect(result.current.loading).toBe(false));

    const next = { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2300 };
    act(() => {
      result.current.setConfig(next);
    });

    // Optimistic local update — config reflects the new value immediately.
    expect(result.current.config).toEqual(next);

    // Backend write must have been invoked with the new value.
    await waitFor(() => {
      const setCalls = invokeSpy.mock.calls.filter((c) => c[0] === 'config_set_vara');
      expect(setCalls).toHaveLength(1);
      expect(setCalls[0][1]).toEqual({ value: next });
    });
  });

  it('broadcasts a same-window CustomEvent so peer hooks stay in sync', async () => {
    const { result: a } = renderHook(() => useVaraConfig());
    const { result: b } = renderHook(() => useVaraConfig());
    await waitFor(() => {
      expect(a.current.loading).toBe(false);
      expect(b.current.loading).toBe(false);
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
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.config.bandwidth_hz).toBeNull();
  });
});
