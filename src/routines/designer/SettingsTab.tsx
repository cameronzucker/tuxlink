/**
 * SettingsTab — the Settings tab body (routines plan-5 Task 12,
 * `.superpowers/sdd/task-12-brief.md`, spec §12 flow 2's settings half + the
 * flow 4 consent-envelope feed).
 *
 * Layout is the approved mock verbatim (dev/scratch/routines-ui-mocks/
 * designer-settings.html): four always-present sections (Transmit mode —
 * conditionally shown; If interrupted; Schedule; Enable) plus a fifth
 * (Referenced entities) that is this task's own addition — the flows doc
 * leaves the presets/station-sets CRUD surface's placement to the plan, and
 * Settings is where the operator already thinks about the routine's
 * environment.
 *
 * Ownership split (mirrors StepInspector/PaletteRail): `RoutineDesigner`
 * owns `draft` and the always-on validation `findings`, and hands down
 * `onChange(patch)` (applied via `defDraft.updateSettings`) plus `onSaved`
 * (RoutineDesigner's existing `handleSave` — persists the CURRENT draft,
 * never blocks, returns `SaveResult | null`). This component self-fetches
 * everything else it needs the same way StepInspector/PaletteRail do
 * (`listActions`, `listRoutines`, `listPresets`/`listStationSets`) rather
 * than threading more props down — none of those change per-keystroke, so a
 * one-shot fetch on mount is the right cost.
 *
 * Callsign-for-display: `useStatusData().callsign` (src/shell/useStatus.ts)
 * is the app's one existing "active callsign" source (the ribbon/status bar
 * already reads it) — `''` until config loads, in which case the Acknowledge
 * button's label drops the "as {callsign}" suffix rather than rendering
 * "Acknowledge as " with a dangling space.
 *
 * Acknowledge flow (spec §4, task brief §1): the stamp targets the STORED
 * def, so clicking Acknowledge (1) calls `onSaved()` (persists the current
 * draft first — a routine can toggle to automatic and acknowledge in the
 * same click without a separate Save), (2) calls `acknowledgeAutomatic(name)`
 * once that succeeds, then (3) reloads the persisted def via `getRoutine`
 * (the binding returns `void`, not the stamped `TransmitAck` — the backend
 * supplies `by`/`at`, never the UI) and feeds the fresh `transmit_ack` back
 * through `onChange` so `RoutineDesigner`'s `draft` picks up the real stamp.
 * A `libraryChanged` event also fires (routinesEvents.ts) for any OTHER
 * mounted surface watching the library; this component doesn't need to wait
 * on it since it already has the fresh value in hand.
 *
 * Mode-switch-away-from-automatic (task brief §1): the ack panel/button only
 * render inside the `transmit_mode === 'automatic'` branch, so switching to
 * Attended hides the whole thing immediately — no separate "clear
 * transmit_ack" patch is needed for the UI to stop showing it. The backend
 * clears the stored `transmit_ack` on the next real Save (Task 1); this
 * component doesn't try to predict/mirror that value locally.
 *
 * Schedule editor (task brief §3, spec §5 one-cadence): edits `triggers`,
 * keeping every non-schedule trigger (in practice just `{type:'manual'}`)
 * untouched and replacing/inserting the single schedule trigger this editor
 * targets — the FIRST `type: 'schedule'` entry, if any. It does not enforce
 * singleness beyond offering one editor (a hand-imported def with two
 * schedules is the validator's `MULTIPLE_SCHEDULES` finding to catch, not
 * this component's).
 */
import { useEffect, useState } from 'react';
import {
  acknowledgeAutomatic,
  deletePreset,
  deleteStationSet,
  getRoutine,
  listActions,
  listPresets,
  listRoutines,
  listStationSets,
  savePreset,
  saveStationSet,
  setEnabled,
  type ActionInfo,
  type Finding,
  type IfMissed,
  type OnInterrupted,
  type RadioPreset,
  type RoutineDef,
  type SaveResult,
  type StationSet,
  type Trigger,
  type TransmitMode,
} from '../routinesApi';
import { formatUiError } from '../format';
import type { SettingsPatch } from './defDraft';
import { useStatusData } from '../../shell/useStatus';
import './SettingsTab.css';

export interface SettingsTabProps {
  draft: RoutineDef;
  findings: Finding[];
  onChange: (patch: SettingsPatch) => void;
  /** Persist the CURRENT draft (RoutineDesigner's `handleSave`) — never
   *  blocks; returns the `SaveResult` on success, `null` on a genuine
   *  backend/parse error (already surfaced via the valbar by the caller). */
  onSaved: () => Promise<SaveResult | null>;
}

/** consent.rs's whole module of consent-closure finding codes (task brief
 *  §1: "AUTO_TX_UNACKED/consent-closure findings") — any of these means the
 *  call-graph transmits under a mode the validator has an opinion about. */
const CONSENT_CLOSURE_CODES = new Set(['AUTO_TX_UNACKED', 'MIXED_MODE_STALL', 'ATTENDED_UNDER_SCHEDULE']);

type ScheduleTrigger = Extract<Trigger, { type: 'schedule' }>;

type AlignChoice = 'hour' | 'day' | 'none';

interface PresetFormState {
  name: string;
  frequencyHz: string;
  mode: string;
  powerW: string;
  atu: boolean;
}

const EMPTY_PRESET_FORM: PresetFormState = { name: '', frequencyHz: '', mode: '', powerW: '', atu: false };

interface StationSetFormState {
  name: string;
  callsigns: string;
}

const EMPTY_STATION_SET_FORM: StationSetFormState = { name: '', callsigns: '' };

export function SettingsTab({ draft, findings, onChange, onSaved }: SettingsTabProps) {
  // ------------------------------------------------------------------------
  // Transmit-mode section visibility: the action registry, self-fetched
  // (mirrors StepInspector/PaletteRail) — never a hardcoded action-name list.
  // ------------------------------------------------------------------------
  const [actions, setActions] = useState<ActionInfo[]>([]);
  useEffect(() => {
    let cancelled = false;
    listActions()
      .then((l) => {
        if (!cancelled) setActions(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setActions([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const stepTransmits = draft.tracks.some((t) =>
    t.steps.some((s) => 'action' in s && actions.find((a) => a.name === s.action)?.transmits === true),
  );
  const findingIndicatesTransmit = findings.some((f) => CONSENT_CLOSURE_CODES.has(f.code));
  const showTransmitSection = stepTransmits || findingIndicatesTransmit;

  const { callsign } = useStatusData();

  const [ackBusy, setAckBusy] = useState(false);
  const [ackError, setAckError] = useState<string | null>(null);

  async function handleAcknowledge() {
    setAckBusy(true);
    setAckError(null);
    try {
      const saved = await onSaved();
      if (!saved) return; // save itself failed — already surfaced via the valbar
      await acknowledgeAutomatic(draft.routine);
      const fresh = await getRoutine(draft.routine);
      onChange({ transmit_ack: fresh.transmit_ack ?? null });
    } catch (e) {
      setAckError(formatUiError(e));
    } finally {
      setAckBusy(false);
    }
  }

  // ------------------------------------------------------------------------
  // Schedule editor — local field state, seeded once from the first
  // `type: 'schedule'` trigger present (or defaults). RoutineDesigner keys
  // this component by `draft.routine` so a routine switch remounts fresh
  // (StepInspector's established convention) rather than needing a resync
  // effect.
  // ------------------------------------------------------------------------
  const existingSchedule = draft.triggers.find((t): t is ScheduleTrigger => t.type === 'schedule') ?? null;
  const [every, setEvery] = useState(existingSchedule?.every ?? '');
  const [align, setAlign] = useState<AlignChoice>(
    existingSchedule?.align === 'hour' || existingSchedule?.align === 'day' ? existingSchedule.align : 'none',
  );
  const [scheduleWindow, setScheduleWindow] = useState(existingSchedule?.window ?? '');
  const [ifMissed, setIfMissed] = useState<IfMissed>(existingSchedule?.if_missed ?? 'skip');

  function commitSchedule(fields: {
    every: string;
    align: AlignChoice;
    window: string;
    ifMissed: IfMissed;
  }) {
    const scheduleTrigger: Trigger = {
      type: 'schedule',
      every: fields.every,
      ...(fields.align !== 'none' ? { align: fields.align } : {}),
      ...(fields.window.trim() !== '' ? { window: fields.window } : {}),
      if_missed: fields.ifMissed,
    };
    const rest = draft.triggers.filter((t) => t.type !== 'schedule');
    onChange({ triggers: [...rest, scheduleTrigger] });
  }

  function handleEveryChange(value: string) {
    setEvery(value);
    commitSchedule({ every: value, align, window: scheduleWindow, ifMissed });
  }
  function handleAlignChange(value: AlignChoice) {
    setAlign(value);
    commitSchedule({ every, align: value, window: scheduleWindow, ifMissed });
  }
  function handleWindowChange(value: string) {
    setScheduleWindow(value);
    commitSchedule({ every, align, window: value, ifMissed });
  }
  function handleIfMissedChange(value: IfMissed) {
    setIfMissed(value);
    commitSchedule({ every, align, window: scheduleWindow, ifMissed: value });
  }
  function handleRemoveSchedule() {
    setEvery('');
    setAlign('none');
    setScheduleWindow('');
    setIfMissed('skip');
    onChange({ triggers: draft.triggers.filter((t) => t.type !== 'schedule') });
  }

  // ------------------------------------------------------------------------
  // Enable — current state self-fetched from `listRoutines()` (RoutineDef
  // itself carries no `enabled` field; only `RoutineSummary`, the library
  // listing's shape, does).
  // ------------------------------------------------------------------------
  const [enabled, setEnabledState] = useState(false);
  useEffect(() => {
    let cancelled = false;
    listRoutines()
      .then((list) => {
        if (cancelled) return;
        const mine = Array.isArray(list) ? list.find((r) => r.routine === draft.routine) : undefined;
        setEnabledState(mine?.enabled ?? false);
      })
      .catch(() => {
        if (!cancelled) setEnabledState(false);
      });
    return () => {
      cancelled = true;
    };
  }, [draft.routine]);

  const [enableBusy, setEnableBusy] = useState(false);
  const [enableFindings, setEnableFindings] = useState<Finding[]>([]);
  const [enableBlocked, setEnableBlockedFlag] = useState(false);

  async function handleToggleEnable() {
    setEnableBusy(true);
    try {
      const result = await setEnabled(draft.routine, !enabled);
      setEnableFindings(result.findings);
      setEnableBlockedFlag(result.blocked);
      if (!result.blocked) setEnabledState(result.enabled);
    } catch (e) {
      setEnableFindings([
        { code: 'ERROR', severity: 'error', routine: draft.routine, message: formatUiError(e) },
      ]);
      setEnableBlockedFlag(true);
    } finally {
      setEnableBusy(false);
    }
  }

  // ------------------------------------------------------------------------
  // Referenced entities — presets + station sets, self-fetched (mirrors
  // StepInspector's @-reference helper data fetch) and refreshed after every
  // successful mutation.
  // ------------------------------------------------------------------------
  const [presets, setPresets] = useState<RadioPreset[]>([]);
  const [presetsError, setPresetsError] = useState<string | null>(null);
  const [presetForm, setPresetForm] = useState<PresetFormState>(EMPTY_PRESET_FORM);

  const [stationSets, setStationSets] = useState<StationSet[]>([]);
  const [stationSetsError, setStationSetsError] = useState<string | null>(null);
  const [setForm, setSetForm] = useState<StationSetFormState>(EMPTY_STATION_SET_FORM);

  useEffect(() => {
    let cancelled = false;
    listPresets()
      .then((l) => {
        if (!cancelled) setPresets(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setPresets([]);
      });
    listStationSets()
      .then((l) => {
        if (!cancelled) setStationSets(Array.isArray(l) ? l : []);
      })
      .catch(() => {
        if (!cancelled) setStationSets([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshPresets() {
    try {
      const l = await listPresets();
      setPresets(Array.isArray(l) ? l : []);
    } catch {
      // A refresh failure leaves the last-known list rendered rather than
      // clearing it out from under the operator.
    }
  }

  async function refreshStationSets() {
    try {
      const l = await listStationSets();
      setStationSets(Array.isArray(l) ? l : []);
    } catch {
      // Same as refreshPresets — keep the last-known list.
    }
  }

  async function handleSavePreset() {
    setPresetsError(null);
    const preset: RadioPreset = {
      name: presetForm.name.trim(),
      frequencyHz: Number(presetForm.frequencyHz),
      mode: presetForm.mode.trim(),
      ...(presetForm.powerW.trim() !== '' ? { powerW: Number(presetForm.powerW) } : {}),
      ...(presetForm.atu ? { atu: true } : {}),
    };
    try {
      await savePreset(preset);
      await refreshPresets();
      setPresetForm(EMPTY_PRESET_FORM);
    } catch (e) {
      setPresetsError(formatUiError(e));
    }
  }

  async function handleDeletePreset(name: string) {
    setPresetsError(null);
    try {
      await deletePreset(name);
      await refreshPresets();
    } catch (e) {
      setPresetsError(formatUiError(e));
    }
  }

  async function handleSaveStationSet() {
    setStationSetsError(null);
    const set: StationSet = {
      name: setForm.name.trim(),
      callsigns: setForm.callsigns
        .split(',')
        .map((c) => c.trim())
        .filter((c) => c.length > 0),
    };
    try {
      await saveStationSet(set);
      await refreshStationSets();
      setSetForm(EMPTY_STATION_SET_FORM);
    } catch (e) {
      setStationSetsError(formatUiError(e));
    }
  }

  async function handleDeleteStationSet(name: string) {
    setStationSetsError(null);
    try {
      await deleteStationSet(name);
      await refreshStationSets();
    } catch (e) {
      setStationSetsError(formatUiError(e));
    }
  }

  return (
    <div className="settings-scroll" data-testid="settings-tab">
      <div className="settings-col">
        {showTransmitSection && (
          <section className="sect" data-testid="settings-transmit-section">
            <div className="sect-head">
              <span className="sect-title">Transmit mode</span>
              <span className="sect-sub">routine transmits — mode required</span>
            </div>
            <div className="sect-body">
              <div className="optrow">
                <button
                  type="button"
                  className={`opt${draft.transmit_mode === 'attended' ? ' sel' : ''}`}
                  data-testid="settings-mode-attended"
                  onClick={() => onChange({ transmit_mode: 'attended' as TransmitMode })}
                >
                  <div className="r">
                    <span className="radio" />
                    Attended
                  </div>
                  <div className="desc">
                    Every transmit step pauses the run (awaiting consent) until you confirm in the GUI.
                    The routine becomes a guided sequence.
                  </div>
                </button>
                <button
                  type="button"
                  className={`opt${draft.transmit_mode === 'automatic' ? ' sel' : ''}`}
                  data-testid="settings-mode-automatic"
                  onClick={() => onChange({ transmit_mode: 'automatic' as TransmitMode })}
                >
                  <div className="r">
                    <span className="radio" />
                    Automatic
                  </div>
                  <div className="desc">
                    Transmit steps fire unattended — on schedule, from an agent, or from a calling
                    routine. All invokers are equivalent after acknowledgment.
                  </div>
                </button>
              </div>

              {draft.transmit_mode === 'automatic' &&
                (draft.transmit_ack ? (
                  <div className="ack" data-testid="settings-ack-acknowledged">
                    <div className="h">
                      ✓ ACKNOWLEDGED — {draft.transmit_ack.by} · {draft.transmit_ack.at}
                    </div>
                    <div className="words">
                      Automatic transmission under Part 97 is the licensee&apos;s responsibility
                      (§97.109(d) automatic control, §97.221 sub-band limits). This routine may key the
                      radio with nobody at the station. Recorded in the routine definition; only
                      grantable here — never by an agent.
                    </div>
                  </div>
                ) : (
                  <div className="ack ack-pending" data-testid="settings-ack-pending">
                    <div className="words">
                      Automatic transmission under Part 97 is the licensee&apos;s responsibility
                      (§97.109(d) automatic control, §97.221 sub-band limits). This routine may key the
                      radio with nobody at the station. Recorded in the routine definition; only
                      grantable here — never by an agent.
                    </div>
                    <button
                      type="button"
                      className="btn btn-accent"
                      data-testid="settings-ack-button"
                      disabled={ackBusy}
                      onClick={() => void handleAcknowledge()}
                    >
                      Acknowledge{callsign ? ` as ${callsign}` : ''}
                    </button>
                    {ackError && (
                      <div className="insp-error" data-testid="settings-ack-error">
                        {ackError}
                      </div>
                    )}
                  </div>
                ))}
            </div>
          </section>
        )}

        <section className="sect" data-testid="settings-interrupted-section">
          <div className="sect-head">
            <span className="sect-title">If interrupted</span>
            <span className="sect-sub">on_interrupted — crash / power-loss policy</span>
          </div>
          <div className="sect-body">
            <div className="optrow">
              <button
                type="button"
                className={`opt${(draft.on_interrupted ?? 'stay') === 'stay' ? ' sel' : ''}`}
                data-testid="settings-interrupted-stay"
                onClick={() => onChange({ on_interrupted: 'stay' as OnInterrupted })}
              >
                <div className="r">
                  <span className="radio" />
                  Stay interrupted
                </div>
                <div className="desc">
                  Default. Interrupted runs stay interrupted at their last journaled step — re-run
                  deliberately.
                </div>
              </button>
              <button
                type="button"
                className={`opt${draft.on_interrupted === 'resume' ? ' sel' : ''}`}
                data-testid="settings-interrupted-resume"
                onClick={() => onChange({ on_interrupted: 'resume' as OnInterrupted })}
              >
                <div className="r">
                  <span className="radio" />
                  Resume on next launch
                </div>
                <div className="desc">
                  Resumes from the interrupted step using the run&apos;s snapshot. On automatic
                  transmit: may key the radio shortly after boot.
                </div>
              </button>
            </div>
          </div>
        </section>

        <section className="sect" data-testid="settings-schedule-section">
          <div className="sect-head">
            <span className="sect-title">Schedule</span>
            <span className="sect-sub">at most one per routine</span>
          </div>
          <div className="sect-body">
            <div className="frow">
              <span className="flabel">Cadence</span>
              <span className="pill-input">
                every{' '}
                <input
                  data-testid="schedule-every-input"
                  placeholder="2h"
                  value={every}
                  onChange={(e) => handleEveryChange(e.target.value)}
                />
              </span>
              <span className="seg">
                <button
                  type="button"
                  className={align === 'hour' ? 'on' : ''}
                  data-testid="schedule-align-hour"
                  onClick={() => handleAlignChange('hour')}
                >
                  align: top of hour
                </button>
                <button
                  type="button"
                  className={align === 'day' ? 'on' : ''}
                  data-testid="schedule-align-day"
                  onClick={() => handleAlignChange('day')}
                >
                  align: midnight
                </button>
                <button
                  type="button"
                  className={align === 'none' ? 'on' : ''}
                  data-testid="schedule-align-none"
                  onClick={() => handleAlignChange('none')}
                >
                  none
                </button>
              </span>
            </div>
            <div className="frow">
              <span className="flabel">Window</span>
              <span className="pill-input">
                <input
                  data-testid="schedule-window-input"
                  placeholder="06:00-22:00"
                  value={scheduleWindow}
                  onChange={(e) => handleWindowChange(e.target.value)}
                />
              </span>
            </div>
            <div className="frow">
              <span className="flabel">Missed fire</span>
              <span className="seg">
                <button
                  type="button"
                  className={ifMissed === 'skip' ? 'on' : ''}
                  data-testid="schedule-missed-skip"
                  onClick={() => handleIfMissedChange('skip')}
                >
                  skip
                </button>
                <button
                  type="button"
                  className={ifMissed === 'run_once_on_launch' ? 'on' : ''}
                  data-testid="schedule-missed-run-once"
                  onClick={() => handleIfMissedChange('run_once_on_launch')}
                >
                  run once on launch
                </button>
              </span>
              <span className="fval" style={{ color: 'var(--text-faint)', fontSize: '11px' }}>
                misses are recorded visibly either way
              </span>
            </div>
            {existingSchedule && (
              <button
                type="button"
                className="btn remove-schedule"
                data-testid="schedule-remove"
                onClick={handleRemoveSchedule}
              >
                Remove schedule
              </button>
            )}
            <div className="note">
              One schedule per routine — a second cadence is a second routine. Need "also every 6 h"?
              Split that track into its own routine and <span className="code">call</span> it.
              (Validator: <span className="code">MULTIPLE_SCHEDULES</span>)
            </div>
          </div>
        </section>

        <section className="sect" data-testid="settings-enable-section">
          <div className="sect-head">
            <span className="sect-title">Enable</span>
            <span className="sect-sub">fleet check runs on every enable / edit-while-enabled</span>
          </div>
          <div className="sect-body">
            <div className="enrow">
              <button
                type="button"
                role="switch"
                aria-checked={enabled}
                className={`toggle-btn${enabled ? ' on' : ''}`}
                data-testid="settings-enable-toggle"
                disabled={enableBusy}
                onClick={() => void handleToggleEnable()}
              />
              <span className="st">{enabled ? 'Enabled' : 'Disabled'}</span>
            </div>
            {enableBlocked && enableFindings.length > 0 && (
              <div className="enable-blocked" data-testid="settings-enable-blocked">
                <div className="h">ENABLE BLOCKED</div>
                {enableFindings.map((f, i) => (
                  <div className="m" key={`${f.code}-${i}`}>
                    <span className="code">{f.code}</span> — {f.message}
                  </div>
                ))}
              </div>
            )}
            {!enableBlocked && enableFindings.length > 0 && (
              <div className="fleetres" data-testid="settings-enable-fleet">
                <div className="h">
                  FLEET CHECK — {enableFindings.length} WARNING{enableFindings.length === 1 ? '' : 'S'} ·
                  ENABLE PERMITTED
                </div>
                {enableFindings.map((f, i) => (
                  <div className="m" key={`${f.code}-${i}`}>
                    <span className="code">{f.code}</span> — {f.message}
                  </div>
                ))}
              </div>
            )}
          </div>
        </section>

        <section className="sect" data-testid="settings-entities-section">
          <div className="sect-head">
            <span className="sect-title">Referenced entities</span>
            <span className="sect-sub">@preset / @station-set — used by step params</span>
          </div>
          <div className="sect-body">
            <div className="entity-group">
              <div className="entity-head">Radio presets</div>
              {presetsError && (
                <div className="insp-error" data-testid="presets-error">
                  {presetsError}
                </div>
              )}
              <table className="entity-table" data-testid="presets-table">
                <thead>
                  <tr>
                    <th>name</th>
                    <th>frequencyHz</th>
                    <th>mode</th>
                    <th>powerW</th>
                    <th>atu</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {presets.map((p) => (
                    <tr key={p.name} data-testid={`preset-row-${p.name}`}>
                      <td className="mono">{p.name}</td>
                      <td className="mono">{p.frequencyHz}</td>
                      <td className="mono">{p.mode}</td>
                      <td className="mono">{p.powerW ?? '—'}</td>
                      <td className="mono">{p.atu ? 'yes' : '—'}</td>
                      <td>
                        <button
                          type="button"
                          data-testid={`preset-edit-${p.name}`}
                          onClick={() =>
                            setPresetForm({
                              name: p.name,
                              frequencyHz: String(p.frequencyHz),
                              mode: p.mode,
                              powerW: p.powerW != null ? String(p.powerW) : '',
                              atu: p.atu === true,
                            })
                          }
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          data-testid={`preset-delete-${p.name}`}
                          onClick={() => void handleDeletePreset(p.name)}
                        >
                          Delete
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="entity-form">
                <input
                  data-testid="preset-form-name"
                  placeholder="name"
                  value={presetForm.name}
                  onChange={(e) => setPresetForm((f) => ({ ...f, name: e.target.value }))}
                />
                <input
                  data-testid="preset-form-frequency"
                  placeholder="frequencyHz"
                  value={presetForm.frequencyHz}
                  onChange={(e) => setPresetForm((f) => ({ ...f, frequencyHz: e.target.value }))}
                />
                <input
                  data-testid="preset-form-mode"
                  placeholder="mode"
                  value={presetForm.mode}
                  onChange={(e) => setPresetForm((f) => ({ ...f, mode: e.target.value }))}
                />
                <input
                  data-testid="preset-form-power"
                  placeholder="powerW"
                  value={presetForm.powerW}
                  onChange={(e) => setPresetForm((f) => ({ ...f, powerW: e.target.value }))}
                />
                <label>
                  <input
                    type="checkbox"
                    data-testid="preset-form-atu"
                    checked={presetForm.atu}
                    onChange={(e) => setPresetForm((f) => ({ ...f, atu: e.target.checked }))}
                  />{' '}
                  atu
                </label>
                <button type="button" className="btn" data-testid="preset-form-save" onClick={() => void handleSavePreset()}>
                  Save preset
                </button>
              </div>
            </div>

            <div className="entity-group">
              <div className="entity-head">Station sets</div>
              {stationSetsError && (
                <div className="insp-error" data-testid="station-sets-error">
                  {stationSetsError}
                </div>
              )}
              <table className="entity-table" data-testid="station-sets-table">
                <thead>
                  <tr>
                    <th>name</th>
                    <th>callsigns</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {stationSets.map((s) => (
                    <tr key={s.name} data-testid={`station-set-row-${s.name}`}>
                      <td className="mono">{s.name}</td>
                      <td className="mono">{s.callsigns.join(', ')}</td>
                      <td>
                        <button
                          type="button"
                          data-testid={`station-set-edit-${s.name}`}
                          onClick={() => setSetForm({ name: s.name, callsigns: s.callsigns.join(', ') })}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          data-testid={`station-set-delete-${s.name}`}
                          onClick={() => void handleDeleteStationSet(s.name)}
                        >
                          Delete
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div className="entity-form">
                <input
                  data-testid="station-set-form-name"
                  placeholder="name"
                  value={setForm.name}
                  onChange={(e) => setSetForm((f) => ({ ...f, name: e.target.value }))}
                />
                <input
                  data-testid="station-set-form-callsigns"
                  placeholder="callsigns (comma-separated)"
                  value={setForm.callsigns}
                  onChange={(e) => setSetForm((f) => ({ ...f, callsigns: e.target.value }))}
                />
                <button
                  type="button"
                  className="btn"
                  data-testid="station-set-form-save"
                  onClick={() => void handleSaveStationSet()}
                >
                  Save station set
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
