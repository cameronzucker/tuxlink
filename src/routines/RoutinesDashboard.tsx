/**
 * Minimal compile stub (routines plan-5 Task 7). Task 8 replaces this file
 * with the real fleet-ops dashboard (ops table, fleet-check strip, row
 * actions — see `.superpowers/sdd/task-8-brief.md` / dashboard.html). Landing
 * this stub in the SAME PR as Task 8's real implementation is the only
 * acceptable way to ship an inert stub — never alone (ADR 0022).
 *
 * Renders only the surface heading Task 8 keeps, so RoutinesSurface has a
 * real component to mount and AppShell.routines.test.tsx has a real
 * data-testid to assert against.
 */
export function RoutinesDashboard() {
  return (
    <div className="surface" data-testid="routines-dashboard">
      <div className="surface-head">
        <span className="surface-title">Routines</span>
      </div>
    </div>
  );
}
