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
 *  - `{ view: 'designer'; routine; tab }` renders a placeholder — Task 9
 *    supplies the real `RoutineDesigner` (src/routines/designer/RoutineDesigner.tsx).
 *    An empty `routine` name means a fresh, unsaved draft.
 */
import { RoutinesDashboard } from './RoutinesDashboard';
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
  // Task 9 stub: RoutineDesigner doesn't exist yet. One line, no fake
  // controls — Task 9 replaces this outright with the real designer shell
  // (header, tabs, canvas). Empty `routine` is a fresh draft (Routines → New
  // Routine…); a non-empty name is an existing routine opened for edit
  // (Task 8's row double-click, once wired).
  return (
    <div className="surface" data-testid="routine-designer-placeholder">
      Routine Designer — {view.routine || 'new draft'} ({view.tab})
    </div>
  );
}
