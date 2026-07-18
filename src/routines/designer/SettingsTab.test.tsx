/**
 * Tests for SettingsTab.tsx (routines plan-5 Task 12,
 * `.superpowers/sdd/task-12-brief.md`). `@tauri-apps/api/core` is mocked at
 * module scope, keyed by command name (feedback_vitest_invoke_mock_cleanup_call
 * — the no-arg teardown call must be inert), mirroring
 * RoutineDesigner.test.tsx / StepInspector.test.tsx's proven pattern.
 *
 * `../../shell/useStatus`'s `useStatusData` is mocked directly (StationRail.
 * test.tsx's established pattern) rather than standing up a real
 * QueryClientProvider — this component only reads `.callsign` off it.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import type { Finding, RadioPreset, RoutineDef, RoutineSummary, SaveResult, StationSet } from '../routinesApi';
import { saveRoutine } from '../routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

const statusMock = vi.hoisted(() => ({ callsign: '' as string }));
vi.mock('../../shell/useStatus', () => ({
  useStatusData: () => ({ callsign: statusMock.callsign }),
}));

import { SettingsTab } from './SettingsTab';

const BASE_DEF: RoutineDef = {
  routine: 'morning-sweep',
  schema_version: 1,
  transmit_mode: 'automatic',
  triggers: [{ type: 'manual' }],
  tracks: [{ name: 'track-1', steps: [{ id: 's1', action: 'radio.connect', params: {} }] }],
};

const TRANSMITTING_ACTIONS = [
  { name: 'radio.connect', needsRadio: true, needsInternet: false, transmits: true },
];

const PRESETS: RadioPreset[] = [{ name: 'hf-40m', frequencyHz: 7_100_000, mode: 'USB', powerW: 50 }];
const STATION_SETS: StationSet[] = [{ name: 'or-gateways', callsigns: ['W7ABC', 'K7XYZ'] }];
const ROUTINE_SUMMARIES: RoutineSummary[] = [
  { routine: 'morning-sweep', transmitMode: 'automatic', enabled: false, triggers: [{ type: 'manual' }] },
];

type InvokeOverrides = Partial<Record<string, (args: unknown) => unknown>>;

function installInvokeMock(overrides: InvokeOverrides = {}) {
  mockInvoke.mockImplementation((cmd?: string, args?: unknown) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd in overrides) return Promise.resolve(overrides[cmd]!(args));
    switch (cmd) {
      case 'routines_actions_list':
        return Promise.resolve(TRANSMITTING_ACTIONS);
      case 'routines_list':
        return Promise.resolve(ROUTINE_SUMMARIES);
      case 'routines_presets_list':
        return Promise.resolve(PRESETS);
      case 'routines_station_sets_list':
        return Promise.resolve(STATION_SETS);
      case 'routines_get':
        return Promise.resolve(BASE_DEF);
      case 'routines_save':
        return Promise.resolve({ routine: 'morning-sweep', findings: [], blocked: false } satisfies SaveResult);
      default:
        return Promise.resolve(undefined);
    }
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  statusMock.callsign = '';
  installInvokeMock();
});

function callsFor(cmd: string) {
  return mockInvoke.mock.calls.filter((c) => c[0] === cmd);
}

function renderTab(overrides: Partial<Parameters<typeof SettingsTab>[0]> = {}) {
  const onChange = overrides.onChange ?? vi.fn();
  const onSaved =
    overrides.onSaved ??
    vi.fn(async () => saveRoutine(overrides.draft ?? BASE_DEF));
  const utils = render(
    <SettingsTab
      draft={overrides.draft ?? BASE_DEF}
      findings={overrides.findings ?? []}
      onChange={onChange}
      onSaved={onSaved}
    />,
  );
  return { ...utils, onChange, onSaved };
}

describe('SettingsTab — transmit-mode section visibility', () => {
  it('shows the section when a step action transmits per the registry', async () => {
    renderTab();
    await screen.findByTestId('settings-transmit-section');
  });

  it('shows the section when findings carry a consent-closure code, even with no transmitting step', async () => {
    installInvokeMock({ routines_actions_list: () => [] });
    renderTab({
      draft: { ...BASE_DEF, tracks: [{ name: 't', steps: [] }] },
      findings: [
        { code: 'AUTO_TX_UNACKED', severity: 'error', routine: 'morning-sweep', message: 'no ack' } satisfies Finding,
      ],
    });
    await screen.findByTestId('settings-enable-section'); // wait for mount to settle
    expect(screen.getByTestId('settings-transmit-section')).toBeInTheDocument();
  });

  it('hides the section for a non-transmitting routine with no relevant findings', async () => {
    installInvokeMock({ routines_actions_list: () => [] });
    renderTab({ draft: { ...BASE_DEF, tracks: [{ name: 't', steps: [] }] }, findings: [] });
    await screen.findByTestId('settings-enable-section');
    expect(screen.queryByTestId('settings-transmit-section')).not.toBeInTheDocument();
  });
});

describe('SettingsTab — ack panel (a)', () => {
  it('automatic + no ack shows the Acknowledge button', async () => {
    renderTab({ draft: { ...BASE_DEF, transmit_ack: null } });
    await screen.findByTestId('settings-ack-pending');
    expect(screen.getByTestId('settings-ack-button')).toBeInTheDocument();
  });

  it('clicking Acknowledge invokes routines_save then routines_acknowledge_automatic, then reloads via onChange', async () => {
    const onChange = vi.fn();
    installInvokeMock({
      routines_get: () => ({
        ...BASE_DEF,
        transmit_ack: { by: 'N0CALL', at: '2026-07-08T19:41:22Z' },
      }),
    });
    renderTab({ draft: { ...BASE_DEF, transmit_ack: null }, onChange });
    await screen.findByTestId('settings-ack-button');

    fireEvent.click(screen.getByTestId('settings-ack-button'));

    await vi.waitFor(() => {
      expect(callsFor('routines_acknowledge_automatic')).toHaveLength(1);
    });

    expect(callsFor('routines_save')).toHaveLength(1);
    const saveIdx = mockInvoke.mock.calls.findIndex((c) => c[0] === 'routines_save');
    const ackIdx = mockInvoke.mock.calls.findIndex((c) => c[0] === 'routines_acknowledge_automatic');
    expect(saveIdx).toBeLessThan(ackIdx);
    expect(callsFor('routines_acknowledge_automatic')[0]?.[1]).toEqual({ name: 'morning-sweep' });

    await vi.waitFor(() => {
      expect(onChange).toHaveBeenCalledWith({
        transmit_ack: { by: 'N0CALL', at: '2026-07-08T19:41:22Z' },
      });
    });
  });

  it('an acked def shows the stamped line and NO Acknowledge button', async () => {
    renderTab({
      draft: { ...BASE_DEF, transmit_ack: { by: 'N0CALL', at: '2026-07-08T19:41:22Z' } },
    });
    await screen.findByTestId('settings-ack-acknowledged');
    expect(screen.getByText(/ACKNOWLEDGED/)).toBeInTheDocument();
    expect(screen.getByText(/N0CALL/)).toBeInTheDocument();
    expect(screen.getByText(/2026-07-08T19:41:22Z/)).toBeInTheDocument();
    expect(screen.queryByTestId('settings-ack-button')).not.toBeInTheDocument();
  });

  it('renders the Acknowledge button with the active callsign suffix when useStatusData supplies one', async () => {
    statusMock.callsign = 'N0CALL';
    renderTab({ draft: { ...BASE_DEF, transmit_ack: null } });
    await screen.findByTestId('settings-ack-button');
    expect(screen.getByRole('button', { name: 'Acknowledge as N0CALL' })).toBeInTheDocument();
  });
});

describe('SettingsTab — mode switch clears the ack panel (b)', () => {
  it('switching to Attended patches transmit_mode AND transmit_ack:null in one call, and the ack panel disappears once the draft reflects it', async () => {
    const onChange = vi.fn();
    const { rerender } = renderTab({
      draft: { ...BASE_DEF, transmit_ack: { by: 'N0CALL', at: '2026-07-08T19:41:22Z' } },
      onChange,
    });
    await screen.findByTestId('settings-ack-acknowledged');

    fireEvent.click(screen.getByTestId('settings-mode-attended'));
    // The ack clear rides in the SAME patch as the mode change — a
    // mode-only patch would leave the stale ack on the draft (see the
    // switch-away-and-back regression test below).
    expect(onChange).toHaveBeenCalledWith({ transmit_mode: 'attended', transmit_ack: null });

    rerender(
      <SettingsTab
        draft={{ ...BASE_DEF, transmit_mode: 'attended', transmit_ack: null }}
        findings={[]}
        onChange={onChange}
        onSaved={vi.fn()}
      />,
    );

    expect(screen.queryByTestId('settings-ack-acknowledged')).not.toBeInTheDocument();
    expect(screen.queryByTestId('settings-ack-pending')).not.toBeInTheDocument();
    expect(screen.queryByText(/ACKNOWLEDGED/)).not.toBeInTheDocument();
  });

  it('acknowledge → switch to Attended → switch back to Automatic: the ACKNOWLEDGED box is ABSENT and the Acknowledge button renders (no stale consent display)', async () => {
    // Drive the same controlled-prop loop RoutineDesigner runs: apply each
    // onChange patch to a live `draft` and rerender, so the draft evolves
    // exactly as updateSettings would evolve it. This is the reviewer-flagged
    // sequence — before the paired transmit_ack:null clear, the switch-back
    // resurrected a stale green ACKNOWLEDGED box while the STORED def was
    // unacked (the backend clears the stored ack on the attended-mode save).
    let draft: RoutineDef = { ...BASE_DEF, transmit_ack: { by: 'N0CALL', at: '2026-07-08T19:41:22Z' } };
    const onSaved = vi.fn();
    const onChange = vi.fn((patch: Partial<RoutineDef>) => {
      draft = { ...draft, ...patch };
    });
    const { rerender } = render(
      <SettingsTab draft={draft} findings={[]} onChange={onChange} onSaved={onSaved} />,
    );
    await screen.findByTestId('settings-ack-acknowledged');

    // Switch away from automatic…
    fireEvent.click(screen.getByTestId('settings-mode-attended'));
    rerender(<SettingsTab draft={draft} findings={[]} onChange={onChange} onSaved={onSaved} />);
    expect(screen.queryByText(/ACKNOWLEDGED/)).not.toBeInTheDocument();

    // …and back. The draft's ack was cleared with the switch-away, so the
    // stored-def-matching UN-acked panel renders — never a stale green box
    // resurrected from a leftover draft value.
    fireEvent.click(screen.getByTestId('settings-mode-automatic'));
    rerender(<SettingsTab draft={draft} findings={[]} onChange={onChange} onSaved={onSaved} />);

    expect(screen.queryByTestId('settings-ack-acknowledged')).not.toBeInTheDocument();
    expect(screen.queryByText(/ACKNOWLEDGED/)).not.toBeInTheDocument();
    expect(screen.getByTestId('settings-ack-pending')).toBeInTheDocument();
    expect(screen.getByTestId('settings-ack-button')).toBeInTheDocument();
  });
});

describe('SettingsTab — enable gate (c)', () => {
  it('blocked:true keeps the toggle off and renders finding messages verbatim', async () => {
    installInvokeMock({
      routines_set_enabled: () => ({
        routine: 'morning-sweep',
        enabled: false,
        blocked: true,
        findings: [
          { code: 'AUTO_TX_UNACKED', severity: 'error', routine: 'morning-sweep', message: 'transmits with no recorded acknowledgment' } satisfies Finding,
        ],
      }),
    });
    renderTab();
    await screen.findByTestId('settings-enable-toggle');
    expect(screen.getByTestId('settings-enable-toggle')).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(screen.getByTestId('settings-enable-toggle'));

    await screen.findByTestId('settings-enable-blocked');
    expect(screen.getByTestId('settings-enable-toggle')).toHaveAttribute('aria-checked', 'false');
    expect(
      within(screen.getByTestId('settings-enable-blocked')).getByText(
        'transmits with no recorded acknowledgment',
        { exact: false },
      ),
    ).toBeInTheDocument();

    const call = callsFor('routines_set_enabled')[0];
    expect(call?.[1]).toEqual({ name: 'morning-sweep', enabled: true });
  });

  it('a warning-only result enables the toggle and shows the FLEET CHECK … ENABLE PERMITTED panel', async () => {
    installInvokeMock({
      routines_set_enabled: () => ({
        routine: 'morning-sweep',
        enabled: true,
        blocked: false,
        findings: [
          {
            code: 'SCHEDULE_COLLISION',
            severity: 'warning',
            routine: 'morning-sweep',
            message: 'collides with "APRS position + catalog refresh" at 16:00Z on rig G90',
          } satisfies Finding,
        ],
      }),
    });
    renderTab();
    await screen.findByTestId('settings-enable-toggle');

    fireEvent.click(screen.getByTestId('settings-enable-toggle'));

    await screen.findByTestId('settings-enable-fleet');
    expect(screen.getByTestId('settings-enable-toggle')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText(/ENABLE PERMITTED/)).toBeInTheDocument();
    expect(
      within(screen.getByTestId('settings-enable-fleet')).getByText('SCHEDULE_COLLISION', { exact: false }),
    ).toBeInTheDocument();
  });
});

describe('SettingsTab — schedule editor (d)', () => {
  it('patches triggers to keep {type:"manual"} alongside the edited schedule trigger', async () => {
    const onChange = vi.fn();
    renderTab({ draft: { ...BASE_DEF, triggers: [{ type: 'manual' }] }, onChange });
    await screen.findByTestId('settings-schedule-section');

    fireEvent.change(screen.getByTestId('schedule-every-input'), { target: { value: '2h' } });
    fireEvent.click(screen.getByTestId('schedule-align-hour'));
    fireEvent.change(screen.getByTestId('schedule-window-input'), { target: { value: '06:00-22:00' } });

    const last = onChange.mock.calls.at(-1)?.[0];
    expect(last).toEqual({
      triggers: [
        { type: 'manual' },
        { type: 'schedule', every: '2h', align: 'hour', window: '06:00-22:00', if_missed: 'skip' },
      ],
    });
  });

  it('removing the schedule strips the schedule trigger, keeping manual', async () => {
    const onChange = vi.fn();
    renderTab({
      draft: {
        ...BASE_DEF,
        triggers: [
          { type: 'manual' },
          { type: 'schedule', every: '2h', align: 'hour', window: '06:00-22:00', if_missed: 'skip' },
        ],
      },
      onChange,
    });
    await screen.findByTestId('schedule-remove');
    fireEvent.click(screen.getByTestId('schedule-remove'));
    expect(onChange).toHaveBeenCalledWith({ triggers: [{ type: 'manual' }] });
  });
});

describe('SettingsTab — preset save (e)', () => {
  it('invokes routines_presets_save with the camelCase body', async () => {
    renderTab();
    await screen.findByTestId('preset-form-name');

    fireEvent.change(screen.getByTestId('preset-form-name'), { target: { value: 'hf-80m' } });
    fireEvent.change(screen.getByTestId('preset-form-frequency'), { target: { value: '3630000' } });
    fireEvent.change(screen.getByTestId('preset-form-mode'), { target: { value: 'LSB' } });
    fireEvent.change(screen.getByTestId('preset-form-power'), { target: { value: '100' } });
    fireEvent.click(screen.getByTestId('preset-form-atu'));
    fireEvent.click(screen.getByTestId('preset-form-save'));

    await vi.waitFor(() => {
      expect(callsFor('routines_presets_save')).toHaveLength(1);
    });
    expect(callsFor('routines_presets_save')[0]?.[1]).toEqual({
      preset: { name: 'hf-80m', frequencyHz: 3630000, mode: 'LSB', powerW: 100, atu: true },
    });
  });

  it('renders a backend Rejected name-format error verbatim', async () => {
    installInvokeMock({
      routines_presets_save: () => {
        throw {
          kind: 'Rejected',
          detail:
            'preset name "Bad Name!" is invalid — use lowercase letters, digits, and hyphens (1-64 chars, e.g. "or-gateways"); a routine references it as "@preset:Bad Name!", which must be an unambiguous token',
        };
      },
    });
    renderTab();
    await screen.findByTestId('preset-form-name');
    fireEvent.change(screen.getByTestId('preset-form-name'), { target: { value: 'Bad Name!' } });
    fireEvent.change(screen.getByTestId('preset-form-frequency'), { target: { value: '3630000' } });
    fireEvent.change(screen.getByTestId('preset-form-mode'), { target: { value: 'LSB' } });
    fireEvent.click(screen.getByTestId('preset-form-save'));

    await screen.findByTestId('presets-error');
    expect(
      screen.getByText(
        'preset name "Bad Name!" is invalid — use lowercase letters, digits, and hyphens (1-64 chars, e.g. "or-gateways"); a routine references it as "@preset:Bad Name!", which must be an unambiguous token',
      ),
    ).toBeInTheDocument();
  });
});

describe('SettingsTab — referenced entities render existing rows', () => {
  it('renders the fetched presets and station sets tables', async () => {
    renderTab();
    await screen.findByTestId('preset-row-hf-40m');
    expect(within(screen.getByTestId('preset-row-hf-40m')).getByText('7100000')).toBeInTheDocument();
    await screen.findByTestId('station-set-row-or-gateways');
    expect(
      within(screen.getByTestId('station-set-row-or-gateways')).getByText('W7ABC, K7XYZ'),
    ).toBeInTheDocument();
  });

  it('deleting a preset invokes routines_presets_delete and refreshes the list', async () => {
    renderTab();
    await screen.findByTestId('preset-row-hf-40m');
    installInvokeMock({ routines_presets_list: () => [] });
    fireEvent.click(screen.getByTestId('preset-delete-hf-40m'));
    await vi.waitFor(() => {
      expect(callsFor('routines_presets_delete')).toHaveLength(1);
    });
    expect(callsFor('routines_presets_delete')[0]?.[1]).toEqual({ name: 'hf-40m' });
  });
});

describe('SettingsTab — if-interrupted section', () => {
  it('defaults to "stay" selected and patches on_interrupted on click', async () => {
    const onChange = vi.fn();
    renderTab({ draft: { ...BASE_DEF, on_interrupted: undefined }, onChange });
    await screen.findByTestId('settings-interrupted-stay');
    expect(screen.getByTestId('settings-interrupted-stay').className).toContain('sel');

    fireEvent.click(screen.getByTestId('settings-interrupted-resume'));
    expect(onChange).toHaveBeenCalledWith({ on_interrupted: 'resume' });
  });
});

// tuxlink-7ewvq item 1: SettingsTab.css declared a BARE `.radio` utility (the
// option-card's fake radio dot). RoutineDesigner loads this stylesheet on
// every tab, so the bare selector leaked onto PaletteRail's `.pal-group.radio`
// ("RADIO" category header) and squashed it into a stray 28x14 bordered oval
// below the palette hint — the operator's "random oval of no discernible
// provenance". The dot's selector must stay scoped under `.opt`.
describe('SettingsTab.css — no bare .radio selector leak', () => {
  const CSS = import.meta.glob('./SettingsTab.css', {
    eager: true,
    query: '?raw',
    import: 'default',
  }) as Record<string, string>;
  const css = CSS['./SettingsTab.css']!;

  it('scopes the radio-dot rule under .opt', () => {
    expect(css).toMatch(/\.opt \.radio\s*{/);
    // No top-of-line bare `.radio {` anywhere (the leaking form).
    expect(css).not.toMatch(/^\.radio\s*{/m);
  });
});
