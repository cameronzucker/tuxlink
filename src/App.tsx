import { lazy, Suspense, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider, useQueryClient } from '@tanstack/react-query';
import { AppShell } from './shell/AppShell';
import { parseComposeRoute, parseHelpRoute } from './routing';
import { useWizardPhase, WIZARD_PHASE_QUERY_KEYS } from './wizard/useWizardPhase';
import './App.css';

// tuxlink-perf-coldstart: lazy-load the first-run wizard and the compose-webview
// component. Neither is on the main window's cold-start critical path — the
// wizard only appears on first launch; <Compose> only renders inside compose
// webviews (separate windows with their own paint cycle). Removing them from
// the eager import graph trims the bundle that gates first paint of AppShell.
const Wizard = lazy(() =>
  import('./wizard/Wizard').then((m) => ({ default: m.Wizard })),
);
const Compose = lazy(() =>
  import('./compose/Compose').then((m) => ({ default: m.Compose })),
);
// tuxlink-0gsy: separate Tauri webview for Help → Documentation. Same lazy
// pattern as compose to keep it off the main window's cold-start critical path.
const HelpView = lazy(() =>
  import('./help/HelpView').then((m) => ({ default: m.HelpView })),
);

// One QueryClient for the app lifetime. Mailbox/status queries live under it
// (Task 12 useMailbox, Task 16 useStatus). Retry is off so a NotConfigured
// backend surfaces immediately as an empty state rather than retry-spinning.
//
// Exported so unit tests can call `queryClient.clear()` between renders —
// without this, the wizard-phase cache (and any other module-level query)
// leaks between sequential `render(<App />)` calls and the second test sees
// stale data from the first. The production app renders App once, so the
// module-singleton has no observable effect at runtime.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
  },
});

export default function App() {
  // Routing (spec §4.3 / §5.4): compose webviews load "/compose/<draftId>" and
  // must render <Compose>, NOT the main wizard/shell. Computed once — a webview
  // does not change its path at runtime, so the value is stable across the
  // webview's lifetime.
  const composeDraftId = parseComposeRoute(window.location.pathname);
  const isComposeWindow = composeDraftId !== null;
  // tuxlink-0gsy: help webview branch — single-instance, no params (spec §4.1).
  const isHelpWindow = parseHelpRoute(window.location.pathname);

  // QueryClientProvider wraps everything (tuxlink-n4hz: HelpView's
  // useHelpSearch hook calls useQuery — without the provider the help window
  // crashes with "No QueryClient set"; tests didn't catch it because each unit
  // test wraps its own QueryClient). tuxlink-9xy1 Task 4: the main-window
  // routing decision now ALSO depends on a TanStack-Query hook
  // (useWizardPhase), so the wizard-vs-shell decision moved into a child
  // component that runs inside the provider.
  return (
    <QueryClientProvider client={queryClient}>
      <AppRouter
        isComposeWindow={isComposeWindow}
        composeDraftId={composeDraftId}
        isHelpWindow={isHelpWindow}
      />
    </QueryClientProvider>
  );
}

interface AppRouterProps {
  isComposeWindow: boolean;
  composeDraftId: string | null;
  isHelpWindow: boolean;
}

/**
 * Routing branch selector. Lives inside the QueryClientProvider so the
 * wizard-phase probe (`useWizardPhase`) can call useQuery directly.
 */
function AppRouter({ isComposeWindow, composeDraftId, isHelpWindow }: AppRouterProps): ReactNode {
  const queryClient = useQueryClient();

  // tuxlink-9xy1 Task 4: phase-aware wizard routing. Replaces the prior
  // single-boolean `get_wizard_completed` probe. The hook reads both
  // `get_wizard_phase` (new) AND `get_wizard_completed` (legacy) and derives
  // `shouldRouteToWizard` — see the hook's comment for the routing truth
  // table. The probes are only meaningful for the main window; compose +
  // help windows render below regardless. `enabled` is wired off for those
  // branches to avoid wasted invokes.
  const isMainWindow = !isComposeWindow && !isHelpWindow;
  const { shouldRouteToWizard } = useWizardPhase({ enabled: isMainWindow });

  if (isComposeWindow) {
    // Compose webview: render the compose form for its draft id and nothing
    // else (no wizard probe, no shell). Spec §5.4. Lazy-loaded — Suspense
    // fallback is an empty div so the index.html skeleton stays visible until
    // <Compose> hydrates rather than flashing a spinner.
    return (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <Compose draftId={composeDraftId as string} />
      </Suspense>
    );
  }

  if (isHelpWindow) {
    // Help webview: render <HelpView> for /help. Same lazy + Suspense pattern.
    // Spec §4.1.
    return (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <HelpView />
      </Suspense>
    );
  }

  if (shouldRouteToWizard === null) {
    // Both probes still resolving — show the same loading placeholder the
    // prior implementation used while wizard_completed was null.
    return <div data-testid="app-loading">Loading…</div>;
  }

  if (!shouldRouteToWizard) {
    // Post-wizard, the main shell renders (Task 12).
    return <AppShell />;
  }

  // Pre-wizard, the onboarding wizard.
  // tuxlink-eh7: hand off to the shell the moment onboarding finishes — no
  // app restart needed. Previously App.tsx flipped a local boolean; now we
  // invalidate both wizard-phase queries so the hook re-fetches and the
  // routing branch flips on the next render.
  return (
    <Suspense fallback={<div data-testid="app-loading" />}>
      <Wizard
        onComplete={() => {
          queryClient.invalidateQueries({ queryKey: WIZARD_PHASE_QUERY_KEYS.phase });
          queryClient.invalidateQueries({ queryKey: WIZARD_PHASE_QUERY_KEYS.completed });
        }}
      />
    </Suspense>
  );
}
