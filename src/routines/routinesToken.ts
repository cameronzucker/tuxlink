// src/routines/routinesToken.ts — the Routines continuity token's `state`
// shape, defined ONCE here (tuxlink-dmwte task 8, spec §7). The full wire
// context per surface is an ENVELOPE `{ foreground, state }` (spec §5 — the
// foreground bit is a main-window presentation concern, §5's ⇤-vs-✕ split);
// this module owns the `state` half for Routines.
//
// Deliberately dependency-light (type-only imports, one guard function) so
// AppShell can import `isRoutinesView` as a value without eagerly pulling the
// heavy RoutinesSurface / designer chunk into the cold-start bundle.
import type { RoutinesView } from './RoutinesSurface';
import type { RoutineDef } from './routinesApi';

/** The Routines continuity token's `state` half (spec §7). Travels inside the
 *  `{ foreground, state }` envelope on every pop-out / dock-back. `view` is the
 *  operator's place in the surface; `draft` is the designer's in-progress def
 *  (present only when popping/docking from the designer). */
export interface RoutinesTokenState {
  view: RoutinesView;
  draft?: RoutineDef;
}

/** True when `value` is a well-formed {@link RoutinesView}. Used to validate a
 *  token's `state.view` before seeding a surface from it — a malformed or
 *  absent view falls back to the dashboard rather than crashing. */
export function isRoutinesView(value: unknown): value is RoutinesView {
  if (!value || typeof value !== 'object') return false;
  const v = value as { view?: unknown };
  if (v.view === 'dashboard') return true;
  if (v.view === 'designer') {
    const d = value as { routine?: unknown; tab?: unknown };
    return (
      typeof d.routine === 'string' &&
      (d.tab === 'design' || d.tab === 'runs' || d.tab === 'settings')
    );
  }
  return false;
}
