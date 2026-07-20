/**
 * Tests for routinesApi.ts — the manifest + arg-shape contract.
 *
 * This is intentionally NOT an exhaustive per-binding test suite: every
 * binding is a one-line `invoke` wrapper, and the two things worth guarding
 * against a silent drift are (1) the manifest naming all 27 commands with
 * their real Rust names, and (2) the camelCase-arg / snake_case-body wire
 * contract on the two trickiest calls (`saveRoutine`'s `defJson` stringify,
 * and `setEnabled`/`grantConsent`'s multi-arg camelCase shapes).
 *
 * routines plan-5 Task 5 (`.superpowers/sdd/task-5-brief.md`).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
import * as api from './routinesApi';

beforeEach(() => {
  mockInvoke.mockReset();
  // Teardown pitfall: invoke mocks get called with NO args — always resolve.
  mockInvoke.mockImplementation((cmd?: string) =>
    cmd === undefined ? Promise.resolve() : Promise.resolve([]));
});

describe('ROUTINES_UI_COMMANDS manifest', () => {
  it('manifest names all 18 pre-existing commands plus the 11 UI-only additions', () => {
    const required = [
      'routines_list','routines_get','routines_save','routines_delete','routines_set_enabled',
      'routines_run','routines_dry_run','routines_cancel','routines_run_status','routines_journal',
      'routines_consent_grant','routines_missed_fires','routines_presets_list','routines_presets_save',
      'routines_presets_delete','routines_station_sets_list','routines_station_sets_save',
      'routines_station_sets_delete',
      'routines_acknowledge_automatic','routines_validate','routines_validate_draft',
      'routines_actions_list','routines_next_fires','routines_runs_list','routines_fleet_check',
      'routines_export_run_bundle','routines_take_radio',
      'routines_acknowledge_write','routines_consent_closure',
    ];
    expect([...api.ROUTINES_UI_COMMANDS].sort()).toEqual([...required].sort());
  });

  it('acknowledgeWrite passes the routine name as { name }', async () => {
    await api.acknowledgeWrite('morning-sweep');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_acknowledge_write')?.[1])
      .toEqual({ name: 'morning-sweep' });
  });

  it('consentClosure passes { name } and returns the view through', async () => {
    mockInvoke.mockImplementation((cmd?: string) =>
      cmd === undefined
        ? Promise.resolve()
        : Promise.resolve({ transmitSteps: [], writeSteps: [], callEdges: [] }));
    const view = await api.consentClosure('morning-sweep');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_consent_closure')?.[1])
      .toEqual({ name: 'morning-sweep' });
    expect(view).toEqual({ transmitSteps: [], writeSteps: [], callEdges: [] });
  });
});

describe('routinesApi — wire-casing contract', () => {
  it('saveRoutine serializes the def and passes camelCase invoke args', async () => {
    mockInvoke.mockResolvedValueOnce({ routine: 'x', revision: 'rev-1', findings: [], blocked: false });
    await api.saveRoutine({ routine: 'x', schema_version: 1, transmit_mode: 'attended',
      triggers: [{ type: 'manual' }], tracks: [] } as api.RoutineDef);
    const call = mockInvoke.mock.calls.find((c) => c[0] === 'routines_save');
    expect(call?.[1]).toHaveProperty('defJson');           // camelCase ARG name
    expect(JSON.parse(call![1].defJson).transmit_mode).toBe('attended'); // snake body
  });

  it('setEnabled and consent pass camelCase args', async () => {
    await api.setEnabled('r', true);
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_set_enabled')?.[1])
      .toEqual({ name: 'r', enabled: true });
    await api.grantConsent('run-1', 's4');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_consent_grant')?.[1])
      .toEqual({ runId: 'run-1', stepId: 's4' });
  });

  it('validateDraft serializes the def into defJson like saveRoutine', async () => {
    await api.validateDraft({ routine: 'x', schema_version: 1, transmit_mode: 'attended',
      triggers: [{ type: 'manual' }], tracks: [] } as api.RoutineDef);
    const call = mockInvoke.mock.calls.find((c) => c[0] === 'routines_validate_draft');
    expect(call?.[1]).toHaveProperty('defJson');
  });

  it('exportRunBundle passes camelCase runId/outputPath', async () => {
    await api.exportRunBundle('run-9', '/tmp/bundle.json');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_export_run_bundle')?.[1])
      .toEqual({ runId: 'run-9', outputPath: '/tmp/bundle.json' });
  });

  it('runRoutine defaults args to an empty object and passes it through', async () => {
    mockInvoke.mockResolvedValueOnce('run-1');
    await api.runRoutine('r');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_run')?.[1])
      .toEqual({ name: 'r', args: {} });
  });

  it('listRuns passes an optional routine filter through as-is', async () => {
    await api.listRuns();
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'routines_runs_list')?.[1])
      .toEqual({ routine: undefined });
  });
});
