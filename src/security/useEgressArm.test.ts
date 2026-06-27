/**
 * useEgressArm — egress ARM data hook tests (MCP phase 3.6).
 *
 * Verifies the hook polls egress_status, that arm() calls egress_arm with the
 * chosen duration and pokes the cache with the returned DTO, that disarm()
 * calls egress_disarm, and that a backend error surfaces on `error` rather than
 * throwing. Mirrors src/shell/useIdentities.test.ts's invoke-mock convention.
 */

import { createElement, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useEgressArm, EGRESS_STATUS_QUERY_KEY } from './useEgressArm';
import type { EgressStatusDto } from './egressTypes';

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

function freshClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

const DISARMED: EgressStatusDto = { armed: false, armedRemainingSecs: 0, tainted: false };
const ARMED: EgressStatusDto = { armed: true, armedRemainingSecs: 3600, tainted: false };

beforeEach(() => {
  invokeMock.mockReset();
});

describe('useEgressArm — status poll', () => {
  it('calls invoke("egress_status") and exposes the snapshot', async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === 'egress_status' ? Promise.resolve(ARMED) : Promise.resolve(),
    );
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });

    await waitFor(() => expect(result.current.status.armed).toBe(true));
    expect(invokeMock).toHaveBeenCalledWith('egress_status');
    expect(result.current.status.armedRemainingSecs).toBe(3600);
  });

  it('falls back to the disarmed baseline before the first poll resolves', () => {
    invokeMock.mockReturnValue(new Promise(() => {})); // never resolves
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });
    expect(result.current.status).toEqual(DISARMED);
  });
});

describe('useEgressArm — arm', () => {
  it('calls egress_arm with the chosen duration and reflects the armed state', async () => {
    // The status poll reflects post-action reality so the assertion is not racy
    // against an in-flight refetch resolving after the cache poke.
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'egress_status') return Promise.resolve(ARMED);
      if (cmd === 'egress_arm') return Promise.resolve(ARMED);
      return Promise.resolve();
    });
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });

    await act(async () => {
      await result.current.arm(3600);
    });

    expect(invokeMock).toHaveBeenCalledWith('egress_arm', { durationSecs: 3600 });
    // Cache reflects the armed DTO (poked immediately; poll re-confirms).
    await waitFor(() => expect(result.current.status.armed).toBe(true));
    expect(result.current.status.armedRemainingSecs).toBe(3600);
    expect(result.current.error).toBeNull();
  });

  it('pokes the cache with the returned DTO synchronously after arm resolves', async () => {
    // Status poll never resolves, so the only write to the cache is the poke.
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'egress_status') return new Promise(() => {});
      if (cmd === 'egress_arm') return Promise.resolve(ARMED);
      return Promise.resolve();
    });
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });

    await act(async () => {
      await result.current.arm(3600);
    });

    expect(qc.getQueryData(EGRESS_STATUS_QUERY_KEY)).toEqual(ARMED);
  });
});

describe('useEgressArm — disarm', () => {
  it('calls egress_disarm and reflects the disarmed state', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'egress_status') return new Promise(() => {}); // never resolves
      if (cmd === 'egress_disarm') return Promise.resolve(DISARMED);
      return Promise.resolve();
    });
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });

    await act(async () => {
      await result.current.disarm();
    });

    expect(invokeMock).toHaveBeenCalledWith('egress_disarm');
    expect(qc.getQueryData(EGRESS_STATUS_QUERY_KEY)).toEqual(DISARMED);
  });
});

describe('useEgressArm — error surfacing', () => {
  it('surfaces a backend arm error on `error` without throwing', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'egress_status') return Promise.resolve(DISARMED);
      if (cmd === 'egress_arm') return Promise.reject('arm duration must be greater than zero');
      return Promise.resolve();
    });
    const qc = freshClient();
    const { result } = renderHook(() => useEgressArm(), { wrapper: wrapperWith(qc) });

    await act(async () => {
      await result.current.arm(0);
    });

    expect(result.current.error).toContain('arm duration must be greater than zero');
    expect(result.current.busy).toBe(false);
  });
});
