/**
 * Cross-layer integration test (spec §6.3 + R5 #7 — the test class pjih violated).
 *
 * The test class pjih violated: per-layer arbiter tests and per-layer GridEdit
 * tests passed independently, but no test exercised the COMPOSED flow that
 * justifies the position-subsystem restoration. The pjih PR shipped a regression
 * (sticky-Manual broken, source segment flipping to Gps on any grid set) and the
 * CI was green because no test exercised the full backend → frontend round
 * trip.
 *
 * This integration test mounts the full DashboardRibbon + useStatusData hook
 * with mocked Tauri commands, drives the mocked backend state via mockImpl,
 * and walks the spec §6.3 9-step flow:
 *
 *   1. Mount full GridEdit + useStatusData hook with mocked Tauri commands.
 *   2. Initial mocked state: config.position_source = Manual + manual_grid = EM75
 *      + position_status.gps_ready = false.
 *   3. Assert State 1 (MANUAL segment selected + grid value EM75).
 *   4. Click the GPS segment in the segmented control (tuxlink-z5pz amendment).
 *   5. Assert invoke('position_set_source', { source: 'Gps' }) was called.
 *   6. Update mocked config_read.position_source to 'Gps'.
 *   7. Assert State 4 (GPS segment selected + dimmed + grid value `· EM75` +
 *      status text + Set manually button present).
 *   8. Click the Set manually button.
 *   9. Assert grid input mounts AND receives focus.
 *
 * If this test had existed at pjih merge time, the pjih regression would have
 * been caught at CI. The per-layer tests on arbiter and GridEdit have always
 * passed; only the composed flow exercises the contract pjih violated.
 *
 * Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §6.3
 * (Updated 2026-06-02 for the tuxlink-z5pz segmented-control amendment.)
 */

import { test, expect, vi } from 'vitest';
import { act } from 'react';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { DashboardRibbon } from './DashboardRibbon';
import { useStatusData, type ConfigViewDto, type PositionStatusDto } from './useStatus';

// Mock the Tauri bridge at module scope (vitest hoists vi.mock above imports).
// The test below installs a mockImplementation that drives the mocked backend
// state machine for the spec §6.3 9-step flow.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// Mock @tauri-apps/api/event so useStatusData's listen() call resolves to a
// no-op unlisten (no event-driven path needed for this test — the 2s/5s poll
// snapshots drive state updates).
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

/**
 * Wrapper that composes the real useStatusData hook with DashboardRibbon —
 * exactly how AppShell wires them in production. This is what makes the test
 * cross-layer: invoke('config_read') → setConfig → derived position_source
 * → DashboardRibbon prop → GridEdit render.
 */
function ComposedRibbon() {
  const data = useStatusData();
  return <DashboardRibbon data={data} />;
}

test('integration §6.3: State 1 → click source chip → State 4 → Set manually → grid input focused (the test class pjih violated)', async () => {
  // -------------------------------------------------------------------------
  // Step 2: Initial mocked backend state — Manual + manual_grid = EM75 + no fix.
  // -------------------------------------------------------------------------
  let configSource: 'Manual' | 'Gps' = 'Manual';
  const configDto = (): ConfigViewDto => ({
    connect_to_cms: false,
    transport: 'CmsSsl',
    host: 'cms-z.winlink.org',
    callsign: 'N7CPZ',
    identifier: null,
    grid: 'EM75',
    gps_state: 'BroadcastAtPrecision',
    position_precision: 'FourCharGrid',
    position_source: configSource,
    review_inbound_before_download: false,
    trash_auto_purge: true,
    trash_retention_days: 30,
    close_to_tray: true,
  });
  const positionDto = (): PositionStatusDto => ({
    gps_ready: false,
    broadcast_grid: 'EM75',
    ui_grid: 'EM75',
  });

  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
    if (cmd === 'config_read') return configDto();
    if (cmd === 'backend_status') return null;
    if (cmd === 'position_status') return positionDto();
    if (cmd === 'position_set_source') {
      // Step 6: simulate the backend committing source = 'Gps' so the next
      // config_read poll (5s interval) sees the new value.
      configSource = 'Gps';
      return null;
    }
    if (cmd === 'config_set_grid') {
      return null;
    }
    return null;
  });

  // -------------------------------------------------------------------------
  // Step 1: Mount the composed ribbon under a QueryClientProvider.
  // -------------------------------------------------------------------------
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <ComposedRibbon />
    </QueryClientProvider>,
  );

  // -------------------------------------------------------------------------
  // Step 3: Assert State 1 — MANUAL segment selected + grid value EM75.
  // Spec §3.2 row 1: source = Manual && !gpsReady → MANUAL segment selected,
  // grid `EM75`.
  // -------------------------------------------------------------------------
  await waitFor(() => {
    expect(screen.getByTestId('source-segment-manual')).toHaveAttribute('aria-checked', 'true');
  });
  expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-checked', 'false');
  expect(screen.getByTestId('grid-value-display').textContent).toBe('EM75');
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
  expect(screen.queryByTestId('gps-no-fix-status')).not.toBeInTheDocument();

  // -------------------------------------------------------------------------
  // Step 4: Click the GPS segment in the source segmented control (tuxlink-z5pz).
  // The segment is a real <button role="radio">; fireEvent.click fires onUseGps,
  // which wraps invoke('position_set_source', { source: 'Gps' }) +
  // queryClient.invalidateQueries({ queryKey: ['config_read'] }).
  // -------------------------------------------------------------------------
  fireEvent.click(screen.getByTestId('source-segment-gps'));

  // -------------------------------------------------------------------------
  // Step 5: Assert invoke('position_set_source', { source: 'Gps' }) was called.
  // This is the FRONTEND → BACKEND boundary — the contract pjih violated by
  // not preserving the source-segment-as-real-button semantic.
  // -------------------------------------------------------------------------
  await waitFor(() => {
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('position_set_source', { source: 'Gps' });
  });

  // -------------------------------------------------------------------------
  // Step 6: configSource has flipped to 'Gps' inside the mockImplementation.
  // useStatusData polls config_read on a 5s interval, so the new value reaches
  // the hook only on the next tick. Force-call config_read here via the
  // module-level invoke mock to assert the wiring: setting source = 'Gps' at
  // the config boundary causes the next config_read to return position_source
  // = 'Gps', which useStatusData would observe on its 5s poll.
  //
  // To deterministically advance useStatusData past its 5s poll without
  // fake-timer/act conflicts, we wait for the next config_read invocation —
  // the setInterval will fire it within ~5s; vitest's default 5s test timeout
  // is per-test, so this test overrides it below.
  // -------------------------------------------------------------------------
  await waitFor(
    () => {
      // After the next config_read poll resolves, useStatusData's state has
      // position_source = 'Gps' and the GPS segment flips to selected.
      expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-checked', 'true');
    },
    { timeout: 8000, interval: 200 },
  );

  // -------------------------------------------------------------------------
  // Step 7: Assert State 4 — GPS segment selected + dimmed + grid value
  // `· EM75` interpunct + status text 'GPS no fix · broadcasting fallback' +
  // Set manually present. Spec §3.2 row 4 + §2.3 + §2.4 + §2.5.
  // -------------------------------------------------------------------------
  const gpsSegment = screen.getByTestId('source-segment-gps');
  expect(gpsSegment.tagName).toBe('BUTTON');
  expect(gpsSegment.classList.contains('selected')).toBe(true);
  expect(gpsSegment.classList.contains('gps-ready')).toBe(false);
  expect(gpsSegment.classList.contains('dimmed')).toBe(true);
  expect(screen.getByTestId('grid-value-display').textContent).toMatch(/·\s+EM75/);
  expect(screen.getByTestId('gps-no-fix-status').textContent).toMatch(/GPS no fix\s*·\s*broadcasting fallback/);
  expect(screen.getByTestId('set-manually-button')).toBeInTheDocument();

  // -------------------------------------------------------------------------
  // Step 8: Click the Set manually button.
  // Per spec §2.3 + Codex P2 #6: clicking Set manually MUST mount the grid
  // input AND focus it (operator's path back to inline-edit without needing a
  // GPS fix).
  // -------------------------------------------------------------------------
  fireEvent.click(screen.getByTestId('set-manually-button'));
  // Drain the React commit phase for the inline-edit transition.
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });

  // -------------------------------------------------------------------------
  // Step 9: Assert grid input mounts AND receives focus.
  // -------------------------------------------------------------------------
  const gridInput = screen.getByTestId('grid-input');
  expect(gridInput).toBeInTheDocument();
  expect(document.activeElement).toBe(gridInput);
}, 15_000);
