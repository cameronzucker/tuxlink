/**
 * HelpView — root component mounted at /help in a separate Tauri webview
 * window (label "help"). Replaces the modal HelpPanel from PR #214.
 *
 * Spec: docs/superpowers/specs/2026-06-03-help-window-design.md §4.
 *
 * This is the empty skeleton landed in Task 2 of the implementation plan.
 * Sidebar + reading pane + dropdown land in Tasks 3-4.
 */
export function HelpView() {
  return <div data-testid="tux-help-root">Tuxlink Documentation</div>;
}
