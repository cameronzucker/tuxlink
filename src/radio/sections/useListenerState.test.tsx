// src/radio/sections/useListenerState.test.tsx
//
// Task 12 — the hook snapshots the active identity's presented label AT ARM
// TIME into `boundIdentityLabel`, mirroring the Phase-6 backend invariant: an
// armed listener answers as whoever was active when it was armed, and a later
// active-identity switch must NOT move the bound label. Disarm clears it.

import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useListenerState, type ListenerCommandSet } from './useListenerState';

const commands: ListenerCommandSet = {
  listen: 'ardop_listen',
  setListen: 'ardop_set_listen',
  allowedGet: 'ardop_allowed_stations_get',
  allowedAddCallsign: 'ardop_allowed_stations_add',
  allowedAddCallsignArgKey: 'callsign',
  allowedRemoveCallsign: 'ardop_allowed_stations_remove',
  allowedRemoveCallsignArgKey: 'callsign',
  allowedSetAllowAll: 'ardop_allowed_stations_set_allow_all',
  allowedSetAllowAllArgKey: 'allowAll',
};

beforeEach(() => {
  invokeMock.mockReset();
  // Default: allowlist-get + arm/disarm all resolve.
  invokeMock.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('useListenerState — bound identity snapshot (Task 12)', () => {
  it('is null before the first arm', async () => {
    const { result } = renderHook(() =>
      useListenerState({ commands, activeIdentityLabel: 'W1ABC' }),
    );
    await waitFor(() => expect(result.current.boundIdentityLabel).toBeNull());
  });

  it('snapshots the active identity label when arm resolves', async () => {
    const { result } = renderHook(() =>
      useListenerState({ commands, activeIdentityLabel: 'EOC-3' }),
    );
    await act(async () => {
      await result.current.arm();
    });
    expect(result.current.armed).toBe(true);
    expect(result.current.boundIdentityLabel).toBe('EOC-3');
  });

  it('active_switch_does_not_change_armed_badge — bound label stays fixed across a later active switch', async () => {
    let activeLabel = 'W1ABC';
    const { result, rerender } = renderHook(
      ({ active }: { active: string }) =>
        useListenerState({ commands, activeIdentityLabel: active }),
      { initialProps: { active: activeLabel } },
    );
    // Arm while W1ABC is active.
    await act(async () => {
      await result.current.arm();
    });
    expect(result.current.boundIdentityLabel).toBe('W1ABC');

    // Operator switches the global active identity to W7XYZ while armed.
    activeLabel = 'W7XYZ';
    rerender({ active: activeLabel });

    // The bound label must NOT follow the active switch.
    expect(result.current.boundIdentityLabel).toBe('W1ABC');
  });

  it('clears the bound label on disarm', async () => {
    const { result } = renderHook(() =>
      useListenerState({ commands, activeIdentityLabel: 'W1ABC' }),
    );
    await act(async () => {
      await result.current.arm();
    });
    expect(result.current.boundIdentityLabel).toBe('W1ABC');
    await act(async () => {
      await result.current.disarm();
    });
    expect(result.current.armed).toBe(false);
    expect(result.current.boundIdentityLabel).toBeNull();
  });
});
