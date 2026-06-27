/**
 * useEgressArm — live egress-grant state + arm/disarm actions for the operator
 * ARM surface (MCP phase 3.6).
 *
 * Polls egress_status (2s, matching useStatus.ts's backend poll cadence) via
 * react-query so the ribbon chip + countdown stay live, and exposes arm/disarm
 * that call the backend then poke the cache with the command's returned DTO so
 * the chip flips within one render cycle (no wait for the next poll tick).
 *
 * Backend commands: src-tauri/src/ui_core/security_commands.rs.
 *
 * Errors are surfaced via a returned `error` string (the operator must know an
 * arm failed — unlike fire-and-forget favorites mutations). The backend rejects
 * a zero duration; the presets here are all > 0 so that path is defensive only.
 */

import { useCallback, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { DEV_FIXTURE } from '../mailbox/devFixture';
import { EGRESS_STATUS_DISARMED, type EgressStatusDto } from './egressTypes';

/** Query key for the egress-status poll. Exported so callers can invalidate. */
export const EGRESS_STATUS_QUERY_KEY = ['egress_status'] as const;

export interface UseEgressArm {
  /** Live egress-grant snapshot (disarmed baseline before the first poll). */
  status: EgressStatusDto;
  /** Arm send-authority for `durationSecs`. Resolves once the cache reflects
   *  the new state. */
  arm: (durationSecs: number) => Promise<void>;
  /** Disarm send-authority immediately. */
  disarm: () => Promise<void>;
  /** True while an arm/disarm round-trip is in flight (disables the controls). */
  busy: boolean;
  /** Last arm/disarm error message, or null. Cleared on the next attempt. */
  error: string | null;
}

export function useEgressArm(): UseEgressArm {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const statusQuery = useQuery({
    queryKey: EGRESS_STATUS_QUERY_KEY,
    queryFn: () => invoke<EgressStatusDto>('egress_status'),
    refetchInterval: 2000,
    enabled: !DEV_FIXTURE,
    retry: false,
  });

  // useQuery returns undefined pre-load; normalize to the disarmed baseline.
  const status: EgressStatusDto = statusQuery.data ?? EGRESS_STATUS_DISARMED;

  const arm = useCallback(
    async (durationSecs: number) => {
      setBusy(true);
      setError(null);
      try {
        const next = await invoke<EgressStatusDto>('egress_arm', {
          durationSecs,
        });
        // Poke the cache with the returned DTO so the chip + countdown flip
        // immediately, then let the 2s poll resume as the snapshot backstop.
        queryClient.setQueryData(EGRESS_STATUS_QUERY_KEY, next);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [queryClient],
  );

  const disarm = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<EgressStatusDto>('egress_disarm');
      queryClient.setQueryData(EGRESS_STATUS_QUERY_KEY, next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [queryClient]);

  return { status, arm, disarm, busy, error };
}
