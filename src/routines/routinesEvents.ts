/**
 * TypeScript bindings for the routines Tauri event channel (plan-5 Task 6).
 *
 * Ground truth: `src-tauri/src/routines/events.rs`. ONE channel,
 * `ROUTINES_EVENT` (`"routines:event"`), carrying a `RoutinesEvent` — an
 * externally-tagged enum (`#[serde(tag = "kind", rename_all = "camelCase")]`)
 * whose variant TAGS are camelCase (`runStarted`, `runFinished`,
 * `stateChanged`, `stepCompleted`, `awaitingConsent`, `libraryChanged`,
 * `scheduledFire`, `scheduleSkipped`, `scheduleRefused`, `missedFires` — 10
 * variants total, verified against events.rs:57-169) and whose per-variant
 * FIELDS are explicitly renamed camelCase on the Rust side (`runId`,
 * `stepId`) per the project's serde pitfall (`rename_all` on an enum renames
 * the tag, not struct-variant field names).
 *
 * Per events.rs's module doc (events.rs:8-27) and the `LibraryChanged` doc
 * comment (events.rs:108-113): every event here carries WHAT changed, never
 * the new value. `useRoutines` (this directory) is the one read path — it
 * re-runs a full `refresh()` off these events rather than trusting any event
 * payload as a value to render directly.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { IfMissed, RunState } from './routinesApi';

/** The single Tauri channel every routine-run lifecycle event is emitted on. */
export const ROUTINES_EVENT = 'routines:event';

/** `LibraryEntity` (events.rs:174-178) — `#[serde(rename_all = "camelCase")]`. */
export type LibraryEntity = 'routine' | 'preset' | 'stationSet';

/**
 * A routine-run lifecycle event. One channel, discriminated by `kind`.
 * Mirrors `RoutinesEvent` (events.rs:57-169) variant-for-variant.
 */
export type RoutinesEvent =
  /** A run has started (snapshot resolved, journal opened). */
  | { kind: 'runStarted'; runId: string; routine: string; dryRun: boolean }
  /** A run reached a terminal state. `reason` is absent (not null) on the
   *  wire when the Rust side has none in hand (`skip_serializing_if`). */
  | { kind: 'runFinished'; runId: string; state: RunState; reason?: string }
  /** A run changed run-state. NOT emitted in v1 (events.rs:84-90); on the
   *  wire now so the listener can be written against the full shape. */
  | { kind: 'stateChanged'; runId: string; state: RunState }
  /** A single step finished. NOT emitted in v1 (events.rs:91-99). */
  | { kind: 'stepCompleted'; runId: string; stepId: string; ok: boolean }
  /** A transmitting step in attended mode is parked awaiting operator
   *  consent. Emitted by the slice-5b consent wrapper, not Task 6. */
  | { kind: 'awaitingConsent'; runId: string; stepId: string }
  /** The routines LIBRARY changed — a definition, preset, or station-set was
   *  created, updated, deleted, enabled, or disabled. Carries only *what*
   *  changed, never the new value (events.rs:108-113). */
  | { kind: 'libraryChanged'; entity: LibraryEntity; name: string }
  /** A schedule fired and a run started. `at` is the SCHEDULED instant (unix
   *  seconds), not the wake instant. */
  | { kind: 'scheduledFire'; routine: string; runId: string; at: number }
  /** A schedule came due while the previous run of the same routine was
   *  still active, so this fire was skipped. `reason` is operator-facing. */
  | { kind: 'scheduleSkipped'; routine: string; at: number; reason: string }
  /** A schedule came due and the run was REFUSED at the start gate.
   *  `reason` is the gate's verbatim operator-facing message. */
  | { kind: 'scheduleRefused'; routine: string; at: number; reason: string }
  /** Fires that elapsed while the app was closed. Emitted once per affected
   *  routine at launch. */
  | { kind: 'missedFires'; routine: string; missed: number; policy: IfMissed; ran: boolean };

/**
 * Subscribe to the routines event channel. Thin wrapper over
 * `listen<RoutinesEvent>` that unwraps the Tauri `Event` envelope down to the
 * bare payload before calling `handler` — every caller in this codebase wants
 * the payload, never the envelope.
 */
export function listenRoutinesEvents(handler: (e: RoutinesEvent) => void): Promise<UnlistenFn> {
  return listen<RoutinesEvent>(ROUTINES_EVENT, (e) => handler(e.payload));
}
