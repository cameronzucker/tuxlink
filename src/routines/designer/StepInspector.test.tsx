/**
 * Tests for StepInspector.tsx (routines plan-5 Task 11,
 * `.superpowers/sdd/task-11-brief.md`). StepInspector fetches its own
 * @-reference helper data (`listPresets()`/`listStationSets()`) and its call
 * step's routine dropdown (`listRoutines()`), so `@tauri-apps/api/core` is
 * mocked at module scope, keyed by command name
 * (feedback_vitest_invoke_mock_cleanup_call — the no-arg teardown call must
 * be inert), mirroring RoutineDesigner.test.tsx's proven pattern.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { ActionInfo, ActionStep, ControlStep, RadioPreset, RoutineSummary, StationSet } from '../routinesApi';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

import { StepInspector } from './StepInspector';

const PRESETS: RadioPreset[] = [{ name: 'hf-40m', frequencyHz: 7_100_000, mode: 'USB' }];
const STATION_SETS: StationSet[] = [{ name: 'or-gateways', callsigns: ['W7ABC', 'K7XYZ'] }];
const ROUTINES: RoutineSummary[] = [
  { routine: 'wx-tabular-brief', transmitMode: 'attended', enabled: false, triggers: [{ type: 'manual' }] },
];

type InvokeOverrides = Partial<Record<string, () => unknown>>;

function installInvokeMock(overrides: InvokeOverrides = {}) {
  mockInvoke.mockImplementation((cmd?: string) => {
    // Teardown pitfall: invoke mocks get called with NO args at cleanup.
    if (cmd === undefined) return Promise.resolve();
    if (cmd && cmd in overrides) return Promise.resolve(overrides[cmd]!());
    switch (cmd) {
      case 'routines_presets_list':
        return Promise.resolve(PRESETS);
      case 'routines_station_sets_list':
        return Promise.resolve(STATION_SETS);
      case 'routines_list':
        return Promise.resolve(ROUTINES);
      default:
        return Promise.resolve(undefined);
    }
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  installInvokeMock();
});

const ACTIONS: ActionInfo[] = [
  { name: 'radio.connect', label: '', description: '', needsRadio: true, needsInternet: false, transmits: true },
];

const ACTION_STEP: ActionStep = {
  id: 's1',
  action: 'radio.connect',
  params: { stations: 'or-gateways', bands: ['40m', '80m'] },
  timeout_s: 30,
  on_radio_busy: 'wait',
};

function renderInspector(overrides: Partial<Parameters<typeof StepInspector>[0]> = {}) {
  const onChange = overrides.onChange ?? vi.fn();
  const onRemove = overrides.onRemove ?? vi.fn();
  const utils = render(
    <StepInspector
      step={overrides.step ?? ACTION_STEP}
      actions={overrides.actions ?? ACTIONS}
      onChange={onChange}
      onRemove={onRemove}
    />,
  );
  return { ...utils, onChange, onRemove };
}

describe('StepInspector — action step basics', () => {
  it('shows the step id read-only, the action name, and its capability flags', () => {
    renderInspector();
    expect(screen.getByTestId('inspector-step-id')).toHaveTextContent('s1');
    expect(screen.getByText('radio.connect')).toBeInTheDocument();
    const row = screen.getByText('radio.connect').closest('.insp-row') as HTMLElement;
    expect(row).toHaveTextContent('RIG');
    expect(row).toHaveTextContent('TX');
  });

  it('clicking Delete calls onRemove', () => {
    const { onRemove } = renderInspector();
    fireEvent.click(screen.getByTestId('inspector-remove'));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  // tuxlink-7ewvq item 9: params default to a key/value grid — no operator
  // should have to hand-type JSON to configure read_rig_state. The raw JSON
  // textarea survives behind an "edit as JSON" toggle for nested shapes.
  it('renders one key/value row per param (strings raw, non-scalars as JSON)', () => {
    renderInspector();
    expect(screen.queryByTestId('inspector-params')).not.toBeInTheDocument(); // no default JSON textarea
    expect((screen.getByTestId('param-value-stations') as HTMLInputElement).value).toBe('or-gateways');
    expect((screen.getByTestId('param-value-bands') as HTMLInputElement).value).toBe('["40m","80m"]');
  });
});

describe('StepInspector — params key/value grid (tuxlink-7ewvq item 9)', () => {
  it('editing a value commits on blur, JSON-parsing scalars and keeping @refs/plain text as strings', () => {
    const { onChange } = renderInspector();
    const value = screen.getByTestId('param-value-stations');
    fireEvent.change(value, { target: { value: '@station-set:or-gateways' } });
    fireEvent.blur(value);
    expect(onChange).toHaveBeenCalledWith({
      params: { stations: '@station-set:or-gateways', bands: ['40m', '80m'] },
    });

    const num = screen.getByTestId('param-value-bands');
    fireEvent.change(num, { target: { value: '45' } });
    fireEvent.blur(num);
    expect(onChange).toHaveBeenLastCalledWith({
      params: { stations: 'or-gateways', bands: 45 },
    });
  });

  it('adding a parameter creates an editable row that commits once key+value blur', () => {
    const { onChange } = renderInspector();
    fireEvent.click(screen.getByTestId('param-add'));
    const key = screen.getByTestId('param-key-new-0');
    fireEvent.change(key, { target: { value: 'freq_hz' } });
    fireEvent.blur(key);
    const value = screen.getByTestId('param-value-freq_hz');
    fireEvent.change(value, { target: { value: '7103500' } });
    fireEvent.blur(value);
    expect(onChange).toHaveBeenLastCalledWith({
      params: { stations: 'or-gateways', bands: ['40m', '80m'], freq_hz: 7103500 },
    });
  });

  it('removing a row commits params without that key', () => {
    const { onChange } = renderInspector();
    fireEvent.click(screen.getByTestId('param-remove-bands'));
    expect(onChange).toHaveBeenCalledWith({ params: { stations: 'or-gateways' } });
  });
});

describe('StepInspector — params JSON edit (behind the toggle)', () => {
  it('the toggle reveals the JSON textarea seeded from the current params', () => {
    renderInspector();
    fireEvent.click(screen.getByTestId('params-json-toggle'));
    const textarea = screen.getByTestId('inspector-params') as HTMLTextAreaElement;
    expect(textarea.value).toBe(JSON.stringify(ACTION_STEP.params, null, 2));
  });

  it('round-trips a valid edit through onChange on blur', () => {
    const { onChange } = renderInspector();
    fireEvent.click(screen.getByTestId('params-json-toggle'));
    const textarea = screen.getByTestId('inspector-params');
    fireEvent.change(textarea, { target: { value: '{"stations": "new-set"}' } });
    fireEvent.blur(textarea);
    expect(onChange).toHaveBeenCalledWith({ params: { stations: 'new-set' } });
    expect(screen.queryByTestId('inspector-params-error')).not.toBeInTheDocument();
  });

  it('an invalid JSON edit shows the error inline, leaves the field editable, and does NOT call onChange', () => {
    const { onChange } = renderInspector();
    fireEvent.click(screen.getByTestId('params-json-toggle'));
    const textarea = screen.getByTestId('inspector-params') as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: '{not valid json' } });
    fireEvent.blur(textarea);
    expect(screen.getByTestId('inspector-params-error')).toBeInTheDocument();
    expect(onChange).not.toHaveBeenCalled();
    // Field stays editable with the operator's (unparsed) text, not reverted.
    expect(textarea.value).toBe('{not valid json');
    expect(textarea).not.toBeDisabled();
  });

  it('timeout_s and on_radio_busy edits call onChange directly (no blur needed)', () => {
    const { onChange } = renderInspector();
    fireEvent.change(screen.getByTestId('inspector-timeout'), { target: { value: '90' } });
    expect(onChange).toHaveBeenCalledWith({ timeout_s: 90 });
    fireEvent.change(screen.getByTestId('inspector-on-radio-busy'), { target: { value: 'fail' } });
    expect(onChange).toHaveBeenCalledWith({ on_radio_busy: 'fail' });
  });
});

describe('StepInspector — @-reference helper (assistance only)', () => {
  it('KV mode: shows completions when a row value starts with @, and clicking one fills that row without committing', async () => {
    const step: ActionStep = { id: 's1', action: 'radio.connect', params: { stations: '@or-ga' } };
    const { onChange } = renderInspector({ step });
    const helper = await screen.findByTestId('inspector-ref-helper');
    expect(helper).toHaveTextContent('@preset:hf-40m');
    expect(helper).toHaveTextContent('@station-set:or-gateways');

    fireEvent.click(screen.getByTestId('ref-chip-station-set-or-gateways'));
    // Assistance only — the chip fills the row's value; the operator still
    // blurs to commit, same as any other edit.
    expect(onChange).not.toHaveBeenCalled();
    expect((screen.getByTestId('param-value-stations') as HTMLInputElement).value).toBe(
      '@station-set:or-gateways',
    );
  });

  it('JSON mode: inserting a completion edits the textarea without committing', async () => {
    const step: ActionStep = { id: 's1', action: 'radio.connect', params: { stations: '@or-ga' } };
    const { onChange } = renderInspector({ step });
    fireEvent.click(screen.getByTestId('params-json-toggle'));
    await screen.findByTestId('inspector-ref-helper');
    fireEvent.click(screen.getByTestId('ref-chip-station-set-or-gateways'));
    expect(onChange).not.toHaveBeenCalled();
    const textarea = screen.getByTestId('inspector-params') as HTMLTextAreaElement;
    expect(textarea.value).toContain('@station-set:or-gateways');
  });

  it('does not show the helper row when no params value starts with @', () => {
    renderInspector();
    expect(screen.queryByTestId('inspector-ref-helper')).not.toBeInTheDocument();
  });
});

describe('StepInspector — branch step', () => {
  const BRANCH_STEP: ControlStep = { id: 's2', control: 'branch', on: 's1.connected', then: ['s3'], else: ['s4'] };

  it('edits to the then/else fields patch the arrays (comma-separated step ids)', () => {
    const { onChange } = renderInspector({ step: BRANCH_STEP });
    expect((screen.getByTestId('inspector-branch-then') as HTMLInputElement).value).toBe('s3');
    expect((screen.getByTestId('inspector-branch-else') as HTMLInputElement).value).toBe('s4');

    fireEvent.change(screen.getByTestId('inspector-branch-then'), { target: { value: 's3, s5' } });
    expect(onChange).toHaveBeenCalledWith({ then: ['s3', 's5'] });

    fireEvent.change(screen.getByTestId('inspector-branch-else'), { target: { value: '' } });
    expect(onChange).toHaveBeenCalledWith({ else: [] });
  });

  it('edits the on field', () => {
    const { onChange } = renderInspector({ step: BRANCH_STEP });
    fireEvent.change(screen.getByTestId('inspector-branch-on'), { target: { value: 's9.ok' } });
    expect(onChange).toHaveBeenCalledWith({ on: 's9.ok' });
  });
});

describe('StepInspector — delay step', () => {
  it('edits the delay duration', () => {
    const step: ControlStep = { id: 's5', control: 'delay', delay: '5m' };
    const { onChange } = renderInspector({ step });
    expect((screen.getByTestId('inspector-delay') as HTMLInputElement).value).toBe('5m');
    fireEvent.change(screen.getByTestId('inspector-delay'), { target: { value: '10m' } });
    expect(onChange).toHaveBeenCalledWith({ delay: '10m' });
  });
});

describe('StepInspector — retry step', () => {
  const RETRY_STEP: ControlStep = { id: 's6', control: 'retry', step: 's1', attempts: 3, backoff_s: 2 };

  it('edits step/attempts/backoff_s', () => {
    const { onChange } = renderInspector({ step: RETRY_STEP });
    fireEvent.change(screen.getByTestId('inspector-retry-step'), { target: { value: 's9' } });
    expect(onChange).toHaveBeenCalledWith({ step: 's9' });
    fireEvent.change(screen.getByTestId('inspector-retry-attempts'), { target: { value: '5' } });
    expect(onChange).toHaveBeenCalledWith({ attempts: 5 });
    fireEvent.change(screen.getByTestId('inspector-retry-backoff'), { target: { value: '4' } });
    expect(onChange).toHaveBeenCalledWith({ backoff_s: 4 });
  });
});

describe('StepInspector — call step', () => {
  it('offers a routine dropdown from listRoutines()', async () => {
    const step: ControlStep = { id: 's7', control: 'call', routine: '' };
    const { onChange } = renderInspector({ step });
    const select = (await screen.findByTestId('inspector-call-routine')) as HTMLSelectElement;
    expect(select.querySelector('option[value="wx-tabular-brief"]')).not.toBeNull();
    fireEvent.change(select, { target: { value: 'wx-tabular-brief' } });
    expect(onChange).toHaveBeenCalledWith({ routine: 'wx-tabular-brief' });
  });
});

describe('StepInspector — end step', () => {
  it('toggles the failed checkbox', () => {
    const step: ControlStep = { id: 's8', control: 'end', failed: false };
    const { onChange } = renderInspector({ step });
    const checkbox = screen.getByTestId('inspector-end-failed') as HTMLInputElement;
    expect(checkbox.checked).toBe(false);
    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenCalledWith({ failed: true });
  });
});
