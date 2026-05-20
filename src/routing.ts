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
