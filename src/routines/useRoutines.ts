/**
 * useRoutines — the routines library hook (plan-5 Task 6).
 *
 * One read path (events.rs:108-113, `RoutinesEvent::LibraryChanged` doc): a
 * routines event carries only WHAT changed, never the new value, so this
 * hook re-runs its own full `refresh()` on every event that means "the
 * library or schedule state may have moved" — `libraryChanged`,
 * `runFinished`, `scheduledFire`, `scheduleRefused`, `scheduleSkipped`, and
 * `missedFires`. `runStarted` / `stateChanged` / `stepCompleted` /
 * `awaitingConsent` are run-progress events a run-status surface consumes
 * directly; they don't change the library/schedule snapshot this hook owns,
 * so they don't trigger a refresh here.
 *
 * `refresh()` loads, in parallel: `listRoutines`, `missedFires`, `nextFires`,
 * `fleetCheck`, `listActions` (the action catalog, fetched once — not per
 * routine), plus a per-routine `validateRoutine` + `getRoutine` for every
 * summary `listRoutines` returned (also run in parallel — the dashboard
 * Task 8 brief's «N tracks · M steps» meta line and TX-mode column both need
 * the full `RoutineDef`, not just the summary, so this hook fetches it
 * lazily alongside validation rather than opening a second read path). A
 * save or a schedule tick can emit several of the trigger events in a short
 * burst (e.g. a save emits `libraryChanged` and the caller's own re-read
 * races it) — a 150ms trailing debounce coalesces a burst into one refresh,
 * mirroring `useFt8Listener.ts`'s `REHYDRATE_DEBOUNCE_MS` precedent.
 *
 * Guards against state updates after unmount (`mountedRef`) and against a
 * stale in-flight refresh clobbering a newer one (`generationRef`), mirroring
 * `useFt8Listener.ts`'s hydrate pattern.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  listRoutines,
  missedFires,
  nextFires as fetchNextFires,
  fleetCheck,
  validateRoutine,
  getRoutine,
  listActions,
  type RoutineSummary,
  type ScheduleStatus,
  type Finding,
  type RoutineDef,
  type ActionInfo,
} from './routinesApi';
import { listenRoutinesEvents } from './routinesEvents';

/** Coalesce window for event-triggered refreshes. */
const REFRESH_DEBOUNCE_MS = 150;

export interface UseRoutinesResult {
  summaries: RoutineSummary[];
  scheduleStatus: ScheduleStatus[];
  /** Next scheduled fire per routine name, unix seconds. */
  nextFires: Record<string, number>;
  /** Validation findings per routine name (`routines_validate`). */
  findingsByRoutine: Record<string, Finding[]>;
  /** Fleet-wide findings (`routines_fleet_check`) — cross-routine issues that
   *  don't attach to any single routine. */
  fleetFindings: Finding[];
  /** Full routine definitions per routine name (`routines_get`), fetched
   *  lazily alongside validation. */
  defsByRoutine: Record<string, RoutineDef>;
  /** The action catalog (`routines_actions_list`), keyed by action name —
   *  lets a caller check `ActionInfo.transmits` for a routine's steps
   *  without any hardcoded action-name list. Fetched once per refresh, not
   *  per routine. */
  actionsByName: Record<string, ActionInfo>;
  /** True once the FIRST refresh has settled (success or failure). Until
   *  then `summaries` is only its initial `[]` — an empty-library UI keyed
   *  on `summaries.length` alone would falsely tell an operator with saved
   *  routines that the library is empty while the initial list + per-routine
   *  validate/get fan-out is still in flight (Codex P2, tuxlink-3awm9). */
  loaded: boolean;
  /** Force an immediate, generation-gated re-read. */
  refresh(): Promise<void>;
}

export function useRoutines(): UseRoutinesResult {
  const [summaries, setSummaries] = useState<RoutineSummary[]>([]);
  const [scheduleStatus, setScheduleStatus] = useState<ScheduleStatus[]>([]);
  const [nextFiresByRoutine, setNextFiresByRoutine] = useState<Record<string, number>>({});
  const [findingsByRoutine, setFindingsByRoutine] = useState<Record<string, Finding[]>>({});
  const [fleetFindings, setFleetFindings] = useState<Finding[]>([]);
  const [defsByRoutine, setDefsByRoutine] = useState<Record<string, RoutineDef>>({});
  const [actionsByName, setActionsByName] = useState<Record<string, ActionInfo>>({});
  const [loaded, setLoaded] = useState(false);

  const mountedRef = useRef(true);
  const generationRef = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    const gen = (generationRef.current += 1);
    try {
      const [summariesResult, scheduleStatusResult, nextFiresResult, fleetFindingsResult, actionsResult] =
        await Promise.all([listRoutines(), missedFires(), fetchNextFires(), fleetCheck(), listActions()]);

      const perRoutineEntries = await Promise.all(
        summariesResult.map(async (s) => {
          const [findings, def] = await Promise.all([validateRoutine(s.routine), getRoutine(s.routine)]);
          return [s.routine, findings, def] as const;
        }),
      );

      if (!mountedRef.current || gen !== generationRef.current) return; // unmounted or superseded

      setSummaries(summariesResult);
      setScheduleStatus(scheduleStatusResult);
      setNextFiresByRoutine(Object.fromEntries(nextFiresResult.map((nf) => [nf.routine, nf.at])));
      setFindingsByRoutine(Object.fromEntries(perRoutineEntries.map(([name, findings]) => [name, findings])));
      setDefsByRoutine(Object.fromEntries(perRoutineEntries.map(([name, , def]) => [name, def])));
      setActionsByName(Object.fromEntries(actionsResult.map((a) => [a.name, a])));
      setFleetFindings(fleetFindingsResult);
    } catch {
      // No Tauri runtime (test/dev harness) or a command failed — leave the
      // last-known state in place rather than clearing it out from under the
      // operator.
    } finally {
      // First refresh has settled either way; a superseded generation means
      // a NEWER refresh is in flight, and that one will flip the flag.
      if (mountedRef.current && gen === generationRef.current) setLoaded(true);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    let disposed = false;
    let unlisten: (() => void) | null = null;

    const scheduleRefresh = () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        debounceRef.current = null;
        void refresh();
      }, REFRESH_DEBOUNCE_MS);
    };

    // Register the listener BEFORE the initial load so an event that fires
    // while the first refresh is still in flight is captured, not dropped.
    listenRoutinesEvents((event) => {
      if (!mountedRef.current) return;
      switch (event.kind) {
        case 'libraryChanged':
        case 'runFinished':
        case 'scheduledFire':
        case 'scheduleRefused':
        case 'scheduleSkipped':
        case 'missedFires':
          scheduleRefresh();
          break;
        default:
          break; // run-progress events; not this hook's concern
      }
    })
      .then((u) => {
        if (disposed) u();
        else unlisten = u;
      })
      .catch(() => {
        // No Tauri runtime in some tests/dev harnesses — refresh() is still
        // callable directly by the caller.
      });

    void refresh();

    return () => {
      mountedRef.current = false;
      disposed = true;
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
      if (unlisten) unlisten();
    };
    // Mount-once subscription; `refresh` is stable (useCallback([])).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  return {
    summaries,
    scheduleStatus,
    nextFires: nextFiresByRoutine,
    findingsByRoutine,
    fleetFindings,
    defsByRoutine,
    actionsByName,
    loaded,
    refresh,
  };
}
