/**
 * RoutinesSurface — the inline full-pane view-switch skeleton for the
 * Routines feature (routines plan-5 Task 7, spec §12).
 *
 * Mounted by AppShell in place of the mailbox master-detail panes
 * (FolderSidebar + message list + reading pane) when `routinesView` is
 * non-null. The app chrome (titlebar, menubar, ribbon, statusbar) stays
 * mounted above/below this surface — there is no new OS window and no
 * "pop out" (Global Constraint 6 / task-7 binding constraint 2).
 *
 * View switch:
 *  - `{ view: 'dashboard' }` renders `RoutinesDashboard` (Task 8's real
 *    fleet-ops table, wired to `onNavigate` below: a row double-click or the
 *    ⋯ menu's Edit opens the designer on that routine; the header's "＋ New
 *    Routine" opens a fresh, unsaved draft).
 *  - `{ view: 'designer'; routine; tab }` renders `RoutineDesigner` (Task 9,
 *    src/routines/designer/RoutineDesigner.tsx) — the real designer shell,
 *    replacing Task 7's one-line placeholder outright. An empty `routine`
 *    name means a fresh, unsaved draft (`RoutineDesigner` never fetches a
 *    def for it). `onBack` returns to the dashboard; `onTabChange` updates
 *    just the `tab` field of this same designer view (Design/Runs/Settings),
 *    keeping `routine` fixed.
 */
import { RoutinesDashboard } from './RoutinesDashboard';
import { RoutineDesigner } from './designer/RoutineDesigner';
import './RoutinesSurface.css';

export type DesignerTab = 'design' | 'runs' | 'settings';

export type RoutinesView =
  | { view: 'dashboard' }
  | { view: 'designer'; routine: string; tab: DesignerTab };

export interface RoutinesSurfaceProps {
  view: RoutinesView;
  /** Navigate to another RoutinesView — the dashboard's row double-click / Edit
   *  and "＋ New Routine" all resolve through this (Task 8). */
  onNavigate: (next: RoutinesView) => void;
}

export function RoutinesSurface({ view, onNavigate }: RoutinesSurfaceProps) {
  if (view.view === 'dashboard') {
    return (
      <RoutinesDashboard
        onOpenDesigner={(routine, tab) => onNavigate({ view: 'designer', routine, tab: tab ?? 'design' })}
        onNewRoutine={() => onNavigate({ view: 'designer', routine: '', tab: 'design' })}
      />
    );
  }
  // Empty `routine` is a fresh draft (Routines → New Routine…); a non-empty
  // name is an existing routine opened for edit (the dashboard's row
  // double-click / ⋯ menu Edit).
  return (
    <RoutineDesigner
      routine={view.routine}
      tab={view.tab}
      onBack={() => onNavigate({ view: 'dashboard' })}
      onTabChange={(tab) => onNavigate({ view: 'designer', routine: view.routine, tab })}
    />
  );
}
