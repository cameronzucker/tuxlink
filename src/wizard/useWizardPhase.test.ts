// src/wizard/useWizardPhase.test.ts
//
// Covers the routing-decision logic in useWizardPhase (tuxlink-9xy1 Task 4).
// The four scenarios exhaust the routing truth table:
//
//   phase=None,     completed=false → wizard   (fresh install)
//   phase=Identity, completed=false → wizard   (mid-wizard — the CODEX-1 fix)
//   phase=Complete, completed=true  → shell    (wizard done)
//   phase=None,     completed=true  → shell    (legacy compat — pre-9xy1 config)
//
// Each scenario mocks @tauri-apps/api/core's `invoke` to return shape-correct
// values for BOTH commands the hook queries in parallel.

import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
import { invoke } from '@tauri-apps/api/core';

import { useWizardPhase, type WizardPhase } from './useWizardPhase';

function makeWrapper() {
  // Fresh QueryClient per test — staleTime: Infinity in the hook plus a
  // module-level QueryClient would leak fixture data between scenarios.
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: qc }, children);
}

function routeInvoke(phase: WizardPhase, completed: boolean) {
  (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
    if (cmd === 'get_wizard_phase') return Promise.resolve(phase);
    if (cmd === 'get_wizard_completed') return Promise.resolve(completed);
    return Promise.resolve(undefined);
  });
}

describe('useWizardPhase', () => {
  beforeEach(() => {
    (invoke as ReturnType<typeof vi.fn>).mockReset();
  });

  it('fresh install (phase=none, completed=false) routes to the wizard', async () => {
    routeInvoke('none', false);
    const { result } = renderHook(() => useWizardPhase(), { wrapper: makeWrapper() });
    await waitFor(() => expect(result.current.shouldRouteToWizard).not.toBeNull());
    expect(result.current.phase).toBe('none');
    expect(result.current.wizardCompleted).toBe(false);
    expect(result.current.shouldRouteToWizard).toBe(true);
  });

  it('mid-Identity (phase=identity, completed=false) routes to the wizard — CODEX-1 fix', async () => {
    routeInvoke('identity', false);
    const { result } = renderHook(() => useWizardPhase(), { wrapper: makeWrapper() });
    await waitFor(() => expect(result.current.shouldRouteToWizard).not.toBeNull());
    expect(result.current.phase).toBe('identity');
    expect(result.current.wizardCompleted).toBe(false);
    // Pre-9xy1 behavior would have routed to the shell here (the CODEX-1 bug
    // the Task 3+4 chain fixes); now the phase awareness keeps the user on
    // the wizard so they reach the Location step on restart.
    expect(result.current.shouldRouteToWizard).toBe(true);
  });

  it('wizard done (phase=complete, completed=true) routes to the shell', async () => {
    routeInvoke('complete', true);
    const { result } = renderHook(() => useWizardPhase(), { wrapper: makeWrapper() });
    await waitFor(() => expect(result.current.shouldRouteToWizard).not.toBeNull());
    expect(result.current.phase).toBe('complete');
    expect(result.current.wizardCompleted).toBe(true);
    expect(result.current.shouldRouteToWizard).toBe(false);
  });

  it('legacy user (phase=none, completed=true) routes to the shell — pre-9xy1 compat', async () => {
    // Pre-9xy1 configs on disk had `wizard_completed: true` and NO
    // `wizard_phase` key. With #[serde(default)] those deserialize as
    // phase=None + completed=true. The hook treats this as "wizard done"
    // so existing users do NOT get re-routed back to the wizard on upgrade.
    routeInvoke('none', true);
    const { result } = renderHook(() => useWizardPhase(), { wrapper: makeWrapper() });
    await waitFor(() => expect(result.current.shouldRouteToWizard).not.toBeNull());
    expect(result.current.phase).toBe('none');
    expect(result.current.wizardCompleted).toBe(true);
    expect(result.current.shouldRouteToWizard).toBe(false);
  });

  it('falls back to wizard when both probes reject (NotConfigured)', async () => {
    (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'get_wizard_phase') return Promise.reject(new Error('no config'));
      if (cmd === 'get_wizard_completed') return Promise.reject(new Error('no config'));
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useWizardPhase(), { wrapper: makeWrapper() });
    await waitFor(() => expect(result.current.shouldRouteToWizard).not.toBeNull());
    expect(result.current.phase).toBe('none');
    expect(result.current.wizardCompleted).toBe(false);
    expect(result.current.shouldRouteToWizard).toBe(true);
  });
});
