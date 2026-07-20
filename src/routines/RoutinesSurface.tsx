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
import { useEffect } from 'react';
import { RoutinesDashboard } from './RoutinesDashboard';
import { RoutineDesigner } from './designer/RoutineDesigner';
import type { RoutineDef } from './routinesApi';
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
  /** When provided, both the dashboard header and the designer header show a
   *  text-labeled "↗ Pop out" affordance (tuxlink-dmwte task 8, spec §5) that
   *  pops the Routines surface to its own window. Absent inside the popped
   *  window (there is nothing to pop out to). */
  onPopOut?: () => void;
  /** Continuity-token draft (spec §7) — seeds the designer and suppresses its
   *  fetch when the current view is the designer. Ignored for the dashboard
   *  view (nothing to seed). */
  initialDraft?: RoutineDef;
  /** Reports the designer's live draft upward for token collection at pop-out
   *  / dock-back time (tuxlink-dmwte task 8). Only fires from the designer
   *  view. */
  onDraftChange?: (draft: RoutineDef) => void;
  /** Continuity-token revision riding with `initialDraft` (spec D7). */
  initialRevision?: string;
  /** Reports the designer's loaded/saved revision upward for token
   *  collection, the counterpart of `onDraftChange`. */
  onRevisionChange?: (revision: string | null) => void;
  /** Close the surface back to the mailbox (tuxlink-9se1x). Provided by the
   *  inline AppShell host only — absent in the popped window, where there is
   *  no mailbox pane to return to. Drives the dashboard's "← Mailbox" button
   *  and the dashboard-level Escape shortcut. */
  onClose?: () => void;
}

export function RoutinesSurface({ view, onNavigate, onPopOut, initialDraft, onDraftChange, initialRevision, onRevisionChange, onClose }: RoutinesSurfaceProps) {
  // tuxlink-9se1x: Escape returns to the mailbox — from the DASHBOARD only.
  // In the designer a stray Escape would silently discard an unsaved draft,
  // so it stays inert there (the "← Routines" button is the deliberate path).
  // Guards: an open dialog or popup menu owns Escape (ImportJsonDialog closes
  // itself on the same keydown without preventDefault; the consent modal must
  // never be dismissed underneath; the dashboard's row menu must dismiss, not
  // the whole surface — Codex P3), and typing surfaces keep their key events.
  useEffect(() => {
    if (view.view !== 'dashboard' || !onClose) return;
    const close = onClose;
    function onKey(e: KeyboardEvent) {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || /^(?:INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
      if (document.querySelector('[role="dialog"], [role="menu"]')) return;
      close();
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [view.view, onClose]);

  if (view.view === 'dashboard') {
    return (
      <RoutinesDashboard
        onOpenDesigner={(routine, tab) => onNavigate({ view: 'designer', routine, tab: tab ?? 'design' })}
        onNewRoutine={() => onNavigate({ view: 'designer', routine: '', tab: 'design' })}
        onPopOut={onPopOut}
        onClose={onClose}
      />
    );
  }
  // Empty `routine` is a fresh draft (Routines → New Routine…); a non-empty
  // name is an existing routine opened for edit (the dashboard's row
  // double-click / ⋯ menu Edit).
  return (
    <RoutineDesigner
      // `key={view.routine}` forces a clean remount if a future navigation
      // path ever goes designer→designer directly (e.g. "next routine"
      // paging) without an intermediate dashboard render — otherwise
      // `RoutineDesigner`'s `isNewDraft` mount-time `useState` initializer
      // and its `draft`/selection state would stick from the PREVIOUS
      // routine (task-9 report's flagged concern; every current navigation
      // path already goes through the dashboard, so this is defensive
      // hardening, not a fix for an observed bug).
      key={view.routine}
      routine={view.routine}
      tab={view.tab}
      onBack={() => onNavigate({ view: 'dashboard' })}
      onTabChange={(tab) => onNavigate({ view: 'designer', routine: view.routine, tab })}
      initialDraft={initialDraft}
      onDraftChange={onDraftChange}
      initialRevision={initialRevision}
      onRevisionChange={onRevisionChange}
      onPopOut={onPopOut}
    />
  );
}
