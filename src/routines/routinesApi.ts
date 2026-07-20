// TypeScript bindings for the Routines Tauri command surface
// (src-tauri/src/routines/commands.rs — 18 pre-existing commands plus the 9
// UI-only additions from plan-5 Tasks 1-4: routines_runs_list,
// routines_acknowledge_automatic, routines_validate, routines_validate_draft,
// routines_actions_list, routines_next_fires, routines_fleet_check,
// routines_export_run_bundle, routines_take_radio).
//
// Field-casing note (mirrors wwvApi.ts's convention):
// Tauri v2 maps camelCase JS invoke ARGS to the Rust command's snake_case
// params (defJson -> def_json, runId -> run_id). Return-value casing varies
// PER TYPE depending on whether the Rust struct carries a
// `#[serde(rename_all = "camelCase")]` attribute:
//
//   as-written (snake_case) -- no rename attribute on the Rust struct:
//     RoutineDef / Track / Step / Trigger / TransmitAck / InputDecl / Finding
//     (`transmit_mode`, `on_interrupted`, `if_missed`, `timeout_s`,
//     `on_radio_busy`, `track`, `step` (a StepId newtype -- serializes as a
//     plain string, never `{0: "..."}`), `message`).
//   camelCase -- `#[serde(rename_all = "camelCase")]` on the Rust struct:
//     RoutineSummary, RadioPreset, StationSet, SaveResult, EnableResult,
//     RunStatusDto, DryRunStarted, DryRunScriptDto, ScheduleStatus, Refusal,
//     Skip, NextFire, RunListEntry, ActionInfo, BundleResult.
//   enum TAGS only (rename_all on the enum, fields inside a variant keep
//   their own casing): TransmitMode/OnInterrupted/Severity (lowercase tags),
//   IfMissed (snake_case tags), Trigger/Control (`tag = "type"|"control"`,
//   lowercase tags, snake_case field names), RunEvent (`tag = "type"`,
//   snake_case tags), RunState (snake_case tags), StepError
//   (`tag = "kind", content = "detail"`, snake_case tags).
//
// See the plan-5 UI task brief's wire-casing table
// (.superpowers/sdd/task-5-brief.md) for the authoritative per-type mapping;
// every type below was checked against its real Rust serde derive before
// being written (src-tauri/tuxlink-routines/src/types.rs,
// src-tauri/tuxlink-routines/src/journal.rs,
// src-tauri/tuxlink-routines/src/validate/findings.rs,
// src-tauri/src/routines/{commands,scheduler,presets,station_sets,export}.rs).

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Wire types — routine definition (spec §14), src-tauri/tuxlink-routines/src/types.rs
// ============================================================================

export type TransmitMode = 'attended' | 'automatic';
export type OnInterrupted = 'stay' | 'resume';
export type BusyPolicy = 'wait' | 'fail';
export type IfMissed = 'skip' | 'run_once_on_launch';

/** Recorded only by a UI act (spec §4); MCP cannot supply it. Reused for BOTH
 * the transmit ack (`transmit_ack`) and the config-write ack (`write_ack`)
 * (C3). `closure_digest` is the consent-closure digest captured at ack time —
 * present on acks stamped by the C3+ backend, absent on legacy acks (a
 * digest-less ack reads as stale and fires the AUTO_*_UNACKED validator). */
export interface TransmitAck {
  by: string;
  at: string;
  closure_digest?: string | null;
}

export interface InputDecl {
  name: string;
  required?: boolean;
}

/** `Trigger` (types.rs:64-80) — `#[serde(tag = "type", rename_all = "lowercase")]`:
 * tags are lowercased, every field inside a variant keeps its snake_case name. */
export type Trigger =
  | {
      type: 'schedule';
      /** Interval like "30m", "2h", "45s". */
      every: string;
      /** "hour" | "day". */
      align?: string | null;
      /** Local-time window "HH:MM-HH:MM". */
      window?: string | null;
      if_missed?: IfMissed;
    }
  | { type: 'manual' };

/** An action step (types.rs:82-93) — as-written (snake_case), no rename attribute. */
export interface ActionStep {
  id: string;
  /** Catalog action name, e.g. "radio.connect" (spec §6). */
  action: string;
  params?: unknown;
  timeout_s?: number;
  on_radio_busy?: BusyPolicy;
}

/** Control-flow payload (types.rs:95-132) —
 * `#[serde(tag = "control", rename_all = "lowercase")]`: verified against the
 * real 5-variant enum (Branch, Delay, Retry, Call, End) — the plan brief's
 * "branch/delay/call/end" list omits Retry, which the Rust type does define. */
export type ControlStep =
  | { id: string; control: 'branch'; on: string; then: string[]; else: string[] }
  | { id: string; control: 'delay'; delay: string }
  | { id: string; control: 'retry'; step: string; attempts: number; backoff_s?: number }
  | { id: string; control: 'call'; routine: string; args?: unknown; sync?: boolean }
  | { id: string; control: 'end'; failed?: boolean; reason?: string | null };

/** `Step` (types.rs:149-154) — untagged: an "action" key means an action
 * step, a "control" key means a control step. */
export type Step = ActionStep | ControlStep;

export interface Track {
  name: string;
  steps: Step[];
}

/** `RoutineDef` (types.rs:171-184) — the export format IS the storage format;
 * as-written (snake_case), no rename attribute. */
export interface RoutineDef {
  routine: string;
  schema_version: number;
  transmit_mode: TransmitMode;
  transmit_ack?: TransmitAck | null;
  /** The config-write acknowledgment (C3) — the `writes_config` consent-class
   *  sibling of `transmit_ack`, recorded only by a UI act. Absent until the
   *  operator acknowledges an automatic config-writing routine. */
  write_ack?: TransmitAck | null;
  on_interrupted?: OnInterrupted;
  inputs?: InputDecl[];
  triggers: Trigger[];
  tracks: Track[];
}

// ============================================================================
// Wire types — validation (tuxlink-routines/src/validate/findings.rs)
// ============================================================================

export type Severity = 'error' | 'warning';

/** `Finding` (findings.rs:24-34) — as-written; `step` is a `StepId` newtype,
 * which serializes as a plain string, never `{0: "..."}`. */
export interface Finding {
  code: string;
  severity: Severity;
  routine: string;
  track?: string | null;
  step?: string | null;
  message: string;
}

// ============================================================================
// Wire types — consent closure (C3), src-tauri/src/routines/commands.rs
// `ConsentClosureView` / `ClosureStepView` / `CallEdgeView`, all camelCase.
// The Settings ack panels enumerate exactly the closure enforcement checks.
// ============================================================================

/** One relevant step in a routine's consent closure — the tuple the ack panels
 * enumerate as `<routine> · <step> · <action> · <params>`. `routine` names
 * whichever routine OWNS the step (it differs from the requested routine when
 * the step lives behind one or more `Call` hops). `track` is carried for
 * validator findings; the panels key runtime-value warnings by `step`. */
export interface ClosureStepView {
  routine: string;
  track: string;
  step: string;
  action: string;
  params: unknown;
}

/** One `Call` edge on a path reaching a relevant step: `routine` calls `callee`
 * at `step` with `args`. Both consent classes' edges are merged + deduped. */
export interface CallEdgeView {
  routine: string;
  step: string;
  callee: string;
  args: unknown;
}

/** The enumerated consent closure a Settings ack panel signs (C3). UI-only —
 * never on the MCP surface. `transmitSteps` / `writeSteps` are the transmit and
 * config-write steps the routine's call-graph closure reaches; visibility of
 * the ack rows is CLOSURE-BASED off these (not a direct step scan), so a step
 * reached only via `Call` still surfaces its ack row. */
export interface ConsentClosureView {
  transmitSteps: ClosureStepView[];
  writeSteps: ClosureStepView[];
  callEdges: CallEdgeView[];
}

// ============================================================================
// Wire types — journal (tuxlink-routines/src/journal.rs)
// ============================================================================

/** `RunState` (journal.rs:17-30) — `#[serde(rename_all = "snake_case")]`. */
export type RunState =
  | 'pending'
  | 'running'
  | 'waiting'
  | 'awaiting_consent'
  | 'awaiting_radio'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'interrupted';

/** `StepError` (error.rs) — `#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]`. */
export type StepError =
  | { kind: 'unset_variable'; detail: string }
  | { kind: 'timeout'; detail: { seconds: number } }
  | { kind: 'action'; detail: { action: string; cause: string } }
  | { kind: 'cancelled' };

/** `ParkKind` (journal.rs) — `#[serde(rename_all = "snake_case")]`. The
 * JOURNALED field is snake_case `park_kind` (unrecased into TS); the Tauri
 * app-event payload uses camelCase `parkKind` (see Global Constraints tri-
 * surface naming rule). Absent on pre-O3/O4 (legacy) journals. */
export type ParkKind = 'transmit' | 'write';

/** `RunEvent` (journal.rs:32-71) — `#[serde(tag = "type", rename_all = "snake_case")]`. */
export type RunEvent =
  | { type: 'run_started'; routine: string; snapshot: unknown; dry_run: boolean }
  | { type: 'state_changed'; state: RunState; step?: string; rig?: string; park_kind?: ParkKind }
  | { type: 'step_intent'; step: string; action: string; resolved_params: unknown }
  | { type: 'step_ok'; step: string; output: unknown }
  | { type: 'step_err'; step: string; error: StepError }
  // Observability-decree events (wire-walk 2026-07-18, journal.rs additive
  // variants): journals written before the enrichment simply never carry them.
  | { type: 'branch_taken'; step: string; on: string; value: unknown; took_then: boolean; target?: string }
  | { type: 'step_skipped'; step: string; reason: string }
  // O3/O4 (Task B1): a `call` control step's parent-to-child edge that History
  // navigates, and an `end` control step's termination with the authored
  // reason. `opaque` is A1's tolerant-reader placeholder for an event type a
  // newer build wrote and this build does not know — stepListModel skips it.
  | { type: 'call_child'; step: string; child_run_id: string }
  | { type: 'end_reached'; step: string; failed: boolean; reason?: string }
  | { type: 'opaque'; raw: unknown }
  | { type: 'run_finished'; state: RunState; reason?: string | null };

/** `JournalEntry` (journal.rs:73-79) — as-written. */
export interface JournalEntry {
  ts_unix: number;
  run_id: string;
  seq: number;
  event: RunEvent;
}

// ============================================================================
// Wire types — command DTOs (src-tauri/src/routines/commands.rs), all camelCase
// ============================================================================

export interface SaveResult {
  routine: string;
  findings: Finding[];
  blocked: boolean;
}

export interface EnableResult {
  routine: string;
  enabled: boolean;
  blocked: boolean;
  findings: Finding[];
}

export interface RunStatus {
  runId: string;
  routine: string;
  dryRun: boolean;
  state: RunState;
}

export interface RunListEntry {
  runId: string;
  routine: string;
  dryRun: boolean;
  startedUnix: number;
  state: RunState;
  finishedUnix: number | null;
}

export interface DryRunStarted {
  runId: string;
  findings: Finding[];
}

export interface ActionInfo {
  name: string;
  /** Human palette label (tuxlink-5lfxk). May be empty (test fakes); render
   *  `label || name`. */
  label: string;
  /** One-line human description (tuxlink-5lfxk). May be empty. */
  description: string;
  needsRadio: boolean;
  transmits: boolean;
  needsInternet: boolean;
  /** Declares the `config.*` write consent class (D5+). The palette/inspector
   *  render a WRITES badge from this. Optional for older fixtures; the backend
   *  always sends it. */
  writesConfig?: boolean;
  /** Canonical example `params` object as a compact JSON string (D6); the
   *  authoring UI seeds it on insert. `null` when the action takes no params. */
  exampleParams?: string | null;
  /** Declared per-param contracts (tuxlink-3nvvl): the typed-field surface the
   *  inspector renders. Empty array = the action has not declared its params.
   *  Optional for older fixtures; the backend always sends it. */
  params?: ParamSpec[];
  /** Declared step outputs — the insert-result picker's source. */
  outputs?: OutputSpec[];
}

/** One declared action parameter — `ParamSpecView` (commands.rs,
 * tuxlink-3nvvl). `type` is the registry's snake_case value-type token. */
export interface ParamSpec {
  key: string;
  type:
    | 'string'
    | 'number'
    | 'boolean'
    | 'string_list'
    | 'band_list'
    | 'station_list'
    | 'object_list'
    | 'object';
  required: boolean;
  description: string;
  /** Closed vocabulary for string params / list elements, when declared. */
  allowed?: string[];
  /** Paste-ready example for THIS param alone, as a real JSON value. */
  example: unknown;
}

/** One declared action output — `OutputSpecView` (commands.rs,
 * tuxlink-3nvvl): what a `$sN.<key>` step ref resolves to. */
export interface OutputSpec {
  key: string;
  type: ParamSpec['type'];
  description: string;
}

/** One scripted dry-run outcome — `DryRunOutcomeDto` (commands.rs:184-194),
 * `#[serde(tag = "kind", rename_all = "camelCase")]`. */
export type DryRunOutcome = { kind: 'ok'; output?: unknown } | { kind: 'err'; cause: string };

/** Caller-supplied dry-run script — `DryRunScriptDto` (commands.rs:162-170),
 * `#[serde(rename_all = "camelCase", default)]`; all fields optional. */
export interface DryRunScript {
  defaultOutcome?: 'optimistic' | 'pessimistic';
  outcomes?: Record<string, DryRunOutcome[]>;
}

// ============================================================================
// Wire types — schedule (src-tauri/src/routines/scheduler.rs), all camelCase
// ============================================================================

export interface Refusal {
  at: number;
  reason: string;
}

export interface Skip {
  at: number;
  reason: string;
}

export interface ScheduleStatus {
  routine: string;
  missed: number;
  lastFireUnix: number;
  lastRefusal: Refusal | null;
  lastSkip: Skip | null;
}

export interface NextFire {
  routine: string;
  at: number;
}

// ============================================================================
// Wire types — library entities + fleet (store.rs, presets.rs, station_sets.rs)
// ============================================================================

/** `RoutineSummary` (store.rs:60-67) — camelCase. */
export interface RoutineSummary {
  routine: string;
  transmitMode: TransmitMode;
  enabled: boolean;
  triggers: Trigger[];
}

/** `RadioPreset` (presets.rs:23-33) — camelCase. */
export interface RadioPreset {
  name: string;
  frequencyHz: number;
  mode: string;
  powerW?: number;
  atu?: boolean;
}

/** `StationSet` (station_sets.rs:42-47) — camelCase. */
export interface StationSet {
  name: string;
  callsigns: string[];
}

/** `BundleResult` (export.rs:49-54) — camelCase. */
export interface BundleResult {
  path: string;
  bytes: number;
}

// ============================================================================
// Command manifest — the single source both the bindings below and
// ROUTINES_UI_COMMANDS read from, so a renamed Rust command breaks the
// binding that calls it, not just the exported list.
// ============================================================================

const CMD = {
  list: 'routines_list',
  get: 'routines_get',
  save: 'routines_save',
  delete: 'routines_delete',
  setEnabled: 'routines_set_enabled',
  run: 'routines_run',
  dryRun: 'routines_dry_run',
  cancel: 'routines_cancel',
  runStatus: 'routines_run_status',
  journal: 'routines_journal',
  consentGrant: 'routines_consent_grant',
  missedFires: 'routines_missed_fires',
  presetsList: 'routines_presets_list',
  presetsSave: 'routines_presets_save',
  presetsDelete: 'routines_presets_delete',
  stationSetsList: 'routines_station_sets_list',
  stationSetsSave: 'routines_station_sets_save',
  stationSetsDelete: 'routines_station_sets_delete',
  acknowledgeAutomatic: 'routines_acknowledge_automatic',
  acknowledgeWrite: 'routines_acknowledge_write',
  consentClosure: 'routines_consent_closure',
  validate: 'routines_validate',
  validateDraft: 'routines_validate_draft',
  actionsList: 'routines_actions_list',
  nextFires: 'routines_next_fires',
  runsList: 'routines_runs_list',
  fleetCheck: 'routines_fleet_check',
  exportRunBundle: 'routines_export_run_bundle',
  takeRadio: 'routines_take_radio',
} as const;

/** Every routines Tauri command name, the 18 pre-existing plus the 11
 * UI-only additions (`routines_runs_list`, `routines_acknowledge_automatic`,
 * `routines_validate`, `routines_validate_draft`, `routines_actions_list`,
 * `routines_next_fires`, `routines_fleet_check`, `routines_export_run_bundle`,
 * `routines_take_radio`, and the C3 consent pair `routines_acknowledge_write`
 * + `routines_consent_closure`). Derived from `CMD` so this list and the
 * bindings below can never drift apart. */
export const ROUTINES_UI_COMMANDS: readonly string[] = Object.values(CMD);

// ============================================================================
// Bindings — library (routine authoring/CRUD)
// ============================================================================

export async function listRoutines(): Promise<RoutineSummary[]> {
  return await invoke<RoutineSummary[]>(CMD.list);
}

export async function getRoutine(name: string): Promise<RoutineDef> {
  return await invoke<RoutineDef>(CMD.get, { name });
}

/** Serializes `def` into the `defJson` string arg `routines_save` expects
 * (`save_routine(state, def_json: &str)`, commands.rs:377). Never blocks on
 * validation findings (spec §10) — the routine is saved regardless; the
 * findings + `blocked` bit come back in the response. */
export async function saveRoutine(def: RoutineDef): Promise<SaveResult> {
  return await invoke<SaveResult>(CMD.save, { defJson: JSON.stringify(def) });
}

export async function deleteRoutine(name: string): Promise<void> {
  await invoke(CMD.delete, { name });
}

export async function setEnabled(name: string, enabled: boolean): Promise<EnableResult> {
  return await invoke<EnableResult>(CMD.setEnabled, { name, enabled });
}

// ============================================================================
// Bindings — runs
// ============================================================================

export async function runRoutine(name: string, args: unknown = {}): Promise<string> {
  return await invoke<string>(CMD.run, { name, args });
}

export async function dryRunRoutine(
  name: string,
  args: unknown = {},
  script?: DryRunScript,
): Promise<DryRunStarted> {
  return await invoke<DryRunStarted>(CMD.dryRun, { name, args, script });
}

export async function cancelRun(runId: string): Promise<boolean> {
  return await invoke<boolean>(CMD.cancel, { runId });
}

export async function runStatus(runId: string): Promise<RunStatus | null> {
  return await invoke<RunStatus | null>(CMD.runStatus, { runId });
}

export async function runJournal(runId: string): Promise<JournalEntry[]> {
  return await invoke<JournalEntry[]>(CMD.journal, { runId });
}

export async function listRuns(routine?: string): Promise<RunListEntry[]> {
  return await invoke<RunListEntry[]>(CMD.runsList, { routine });
}

export async function grantConsent(runId: string, stepId: string): Promise<boolean> {
  return await invoke<boolean>(CMD.consentGrant, { runId, stepId });
}

/** Operator "take the radio" (plan-5 Task 4). `rig` defaults to the
 * single-rig placeholder on the Rust side when omitted. */
export async function takeRadio(rig?: string): Promise<boolean> {
  return await invoke<boolean>(CMD.takeRadio, { rig });
}

// ============================================================================
// Bindings — schedule
// ============================================================================

export async function missedFires(): Promise<ScheduleStatus[]> {
  return await invoke<ScheduleStatus[]>(CMD.missedFires);
}

export async function nextFires(): Promise<NextFire[]> {
  return await invoke<NextFire[]>(CMD.nextFires);
}

// ============================================================================
// Bindings — consent + validation
// ============================================================================

/** Record the Part 97 automatic-transmission acknowledgment (spec §4).
 * Operator-only; not on the MCP surface. */
export async function acknowledgeAutomatic(name: string): Promise<void> {
  await invoke(CMD.acknowledgeAutomatic, { name });
}

/** Record the config-write acknowledgment (C3, spec §4) — the `writes_config`
 * sibling of {@link acknowledgeAutomatic}. Operator-only; not on the MCP
 * surface. The backend stamps `by`/`at` + the closure digest at ack time. */
export async function acknowledgeWrite(name: string): Promise<void> {
  await invoke(CMD.acknowledgeWrite, { name });
}

/** Enumerate one routine's consent closure (C3) — the transmit + config-write
 * steps its call-graph closure reaches, plus the call edges leading to them —
 * for the Settings ack panels. UI-only; not on the MCP surface. */
export async function consentClosure(name: string): Promise<ConsentClosureView> {
  return await invoke<ConsentClosureView>(CMD.consentClosure, { name });
}

export async function validateRoutine(name: string): Promise<Finding[]> {
  return await invoke<Finding[]>(CMD.validate, { name });
}

/** Serializes `def` into `defJson` like `saveRoutine`, but validates an
 * UNSAVED draft body without staging a save (`validate_draft`, commands.rs:437). */
export async function validateDraft(def: RoutineDef): Promise<Finding[]> {
  return await invoke<Finding[]>(CMD.validateDraft, { defJson: JSON.stringify(def) });
}

export async function listActions(): Promise<ActionInfo[]> {
  return await invoke<ActionInfo[]>(CMD.actionsList);
}

export async function fleetCheck(): Promise<Finding[]> {
  return await invoke<Finding[]>(CMD.fleetCheck);
}

export async function exportRunBundle(runId: string, outputPath: string): Promise<BundleResult> {
  return await invoke<BundleResult>(CMD.exportRunBundle, { runId, outputPath });
}

// ============================================================================
// Bindings — presets + station sets (the authorable @-entities)
// ============================================================================

export async function listPresets(): Promise<RadioPreset[]> {
  return await invoke<RadioPreset[]>(CMD.presetsList);
}

export async function savePreset(preset: RadioPreset): Promise<void> {
  await invoke(CMD.presetsSave, { preset });
}

export async function deletePreset(name: string): Promise<void> {
  await invoke(CMD.presetsDelete, { name });
}

export async function listStationSets(): Promise<StationSet[]> {
  return await invoke<StationSet[]>(CMD.stationSetsList);
}

export async function saveStationSet(set: StationSet): Promise<void> {
  await invoke(CMD.stationSetsSave, { set });
}

export async function deleteStationSet(name: string): Promise<void> {
  await invoke(CMD.stationSetsDelete, { name });
}
