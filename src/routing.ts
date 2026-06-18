// Minimal path routing for the app's two webview kinds.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §4.3, §5.4
// bd issue: tuxlink-8zg (orchestrator integration)
//
// The app has NO router library (react-router is not a dependency — adding one
// would be scope creep for a two-route app). Routing is by URL path:
//   - The MAIN webview loads "/" → wizard or AppShell (App.tsx).
//   - Each COMPOSE webview loads "/compose/<draftId>" inside a window labeled
//     `compose-<draftId>` (compose_window.rs WebviewWindowBuilder).
//
// `parseComposeRoute` is the pure path matcher (the unit-test target). App.tsx
// calls it with `window.location.pathname` to decide which tree to mount;
// `<Compose>` for a compose route, the wizard/shell otherwise.

/**
 * If `pathname` is a compose route (`/compose/<draftId>`), return the decoded
 * `draftId`; otherwise return `null`.
 *
 * The draftId segment is URL-decoded and must be non-empty. A trailing slash
 * is tolerated. Anything else (`/`, `/compose`, `/compose/`, other paths)
 * returns `null` so the caller falls through to the main wizard/shell tree.
 */
export function parseComposeRoute(pathname: string): string | null {
  const m = pathname.match(/^\/compose\/([^/]+)\/?$/);
  if (!m) return null;
  let draftId: string;
  try {
    draftId = decodeURIComponent(m[1]);
  } catch {
    // Malformed percent-encoding — treat as no match rather than throwing.
    return null;
  }
  return draftId.length > 0 ? draftId : null;
}

/** Fresh draft id for a new compose window. Stable per click. */
export function newDraftId(): string {
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rand = Math.random().toString(36).slice(2, 8);
  return `draft-${ts}-${rand}`;
}

/**
 * If `pathname` is the help route (`/help` or `/help/`), return true.
 * The help window is single-instance with no parameters (tuxlink-0gsy /
 * spec §4.1) — boolean is sufficient; no equivalent to parseComposeRoute's
 * id return.
 */
export function parseHelpRoute(pathname: string): boolean {
  return pathname === '/help' || pathname === '/help/';
}

/**
 * If `pathname` is the logging route (`/logging` or `/logging/`), return true.
 * The logging window is single-instance with no parameters (tuxlink-qjgx /
 * spec §8.1); boolean suffices.
 */
export function parseLoggingRoute(pathname: string): boolean {
  return /^\/logging\/?$/.test(pathname);
}

/**
 * If `pathname` is the Station Data route (`/stations` or `/stations/`), return
 * true. The popped-out environmental panel is single-instance with no
 * parameters (tuxlink-2phz); boolean suffices.
 */
export function parseStationsRoute(pathname: string): boolean {
  return /^\/stations\/?$/.test(pathname);
}
