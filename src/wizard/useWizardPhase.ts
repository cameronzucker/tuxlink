/**
 * useWizardPhase — wizard routing hook for App.tsx (tuxlink-9xy1 Task 4).
 *
 * Replaces the prior single-boolean `get_wizard_completed` probe with a
 * phase-aware lookup. Polls BOTH the new `get_wizard_phase` command (returns
 * `WizardPhase`) AND the legacy `get_wizard_completed` boolean in parallel so
 * the hook can distinguish a brand-new "Identity persisted but Location not
 * yet" mid-wizard state (the CODEX-1 fix) from a pre-9xy1 legacy config that
 * was migrated on disk with `wizard_completed=true` and no `wizard_phase` key.
 *
 * Routing rule (see App.tsx):
 *
 *   route to shell iff:    phase === 'complete'
 *                     OR   (phase === 'none' AND wizardCompleted === true)
 *                          ^^^ the legacy-migration compat path
 *   otherwise:             route to the wizard
 *
 * The legacy branch covers pre-9xy1 configs on disk that have
 * `wizard_completed: true` but NO `wizard_phase` key. With `#[serde(default)]`
 * on the Rust side, those deserialize as `wizard_phase = None` while
 * `wizard_completed` stays `true`. Without this carve-out, existing users
 * would be re-routed back to the wizard on upgrade. New installs (no config
 * file yet) read both as falsy (None + false), so they route to the wizard
 * as expected.
 *
 * Spec: docs/superpowers/specs/2026-06-04-gps-foundation-design.md (Task 4)
 * bd issue: tuxlink-9xy1
 */

import { useQuery } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';

/**
 * Mirrors `WizardPhase` from `src-tauri/src/wizard_phase.rs`. Serde
 * `rename_all = "snake_case"` produces the lowercase string variants.
 */
export type WizardPhase = 'none' | 'identity' | 'complete';

export interface WizardPhaseState {
  /** Persisted WizardPhase. `null` while either probe is still loading. */
  phase: WizardPhase | null;
  /** Legacy `wizard_completed` boolean. `null` while loading. */
  wizardCompleted: boolean | null;
  /**
   * Routing verdict consumed by App.tsx. `null` until BOTH probes have a
   * value — App.tsx renders a loading placeholder during this window.
   */
  shouldRouteToWizard: boolean | null;
}

/** Query keys exported for cache invalidation from the wizard's onComplete handler. */
export const WIZARD_PHASE_QUERY_KEYS = {
  phase: ['get_wizard_phase'] as const,
  completed: ['get_wizard_completed'] as const,
};

/**
 * Probe BOTH `get_wizard_phase` and `get_wizard_completed` in parallel.
 *
 * Both queries treat a backend error as "wizard not yet complete" (the
 * NotConfigured / pre-install path). This matches App.tsx's prior fallback
 * (`.catch(() => setWizardCompleted(false))`) so a brand-new install with no
 * config file still routes to the wizard rather than to an error state.
 *
 * Each query has its own React-Query cache entry so the Wizard's onComplete
 * handler (or any future write path) can target either key with
 * `invalidateQueries`. The hook itself does not call invalidate — App.tsx
 * does on completion to flip the routing branch without an app restart.
 */
export function useWizardPhase(options?: { enabled?: boolean }): WizardPhaseState {
  const enabled = options?.enabled ?? true;

  const phaseQuery = useQuery({
    queryKey: WIZARD_PHASE_QUERY_KEYS.phase,
    queryFn: async (): Promise<WizardPhase> => {
      try {
        return await invoke<WizardPhase>('get_wizard_phase');
      } catch {
        // Pre-install / NotConfigured: treat as no phase persisted yet.
        return 'none';
      }
    },
    enabled,
    retry: false,
    // No refetchInterval — wizard phase only changes via Wizard.onComplete,
    // which invalidates these query keys explicitly. refetchOnWindowFocus
    // stays at the QueryClient default (off — see App.tsx defaults).
    refetchOnWindowFocus: false,
  });

  const completedQuery = useQuery({
    queryKey: WIZARD_PHASE_QUERY_KEYS.completed,
    queryFn: async (): Promise<boolean> => {
      try {
        return await invoke<boolean>('get_wizard_completed');
      } catch {
        return false;
      }
    },
    enabled,
    retry: false,
    refetchOnWindowFocus: false,
  });

  const phase = (phaseQuery.data ?? null) as WizardPhase | null;
  const wizardCompleted = completedQuery.data ?? null;

  // Hold off on a verdict until BOTH probes have a value. App.tsx renders a
  // loading placeholder while shouldRouteToWizard is null so a partial result
  // doesn't briefly flash the wrong branch.
  const shouldRouteToWizard =
    phase === null || wizardCompleted === null
      ? null
      : !(phase === 'complete' || (phase === 'none' && wizardCompleted === true));

  return { phase, wizardCompleted, shouldRouteToWizard };
}
