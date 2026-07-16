import { useEffect, useState, lazy, Suspense, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { AppShell } from './shell/AppShell';
import { HintProvider } from './onboarding/HintProvider';
import { ErrorBoundary } from './ErrorBoundary';
import {
  parseComposeRoute,
  parseHelpRoute,
  parseLoggingRoute,
  parseStationsRoute,
  parsePopRoute,
  isSecondaryWindow,
} from './routing';
import './App.css';
import './styles/controls.css';

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
// tuxlink-qjgx: separate Tauri webview for Help → Logging. Same lazy pattern.
const LoggingView = lazy(() =>
  import('./help/LoggingView').then((m) => ({ default: m.LoggingView })),
);
// tuxlink-2phz: separate Tauri webview for the popped-out Station Data panel.
// Same lazy pattern — off the main window's cold-start critical path.
const StationsView = lazy(() =>
  import('./aprs/StationsView').then((m) => ({ default: m.StationsView })),
);
// bd tuxlink-dmwte: separate Tauri webview for a popped-out dockable surface
// (Routines / Tac Map / APRS Chat, spec §3/§4). Same lazy pattern — off the
// main window's cold-start critical path. Task 7 replaces the placeholder.
const PoppedSurfaceHost = lazy(() =>
  import('./dock/PoppedSurfaceHost').then((m) => ({ default: m.PoppedSurfaceHost })),
);

// One QueryClient for the app lifetime. Mailbox/status queries live under it
// (Task 12 useMailbox, Task 16 useStatus). Retry is off so a NotConfigured
// backend surfaces immediately as an empty state rather than retry-spinning.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false, refetchOnWindowFocus: false },
  },
});

export default function App() {
  const [wizardCompleted, setWizardCompleted] = useState<boolean | null>(null);

  // Routing (spec §4.3 / §5.4): compose webviews load "/compose/<draftId>" and
  // must render <Compose>, NOT the main wizard/shell. Computed once — a webview
  // does not change its path at runtime, so the value is stable across the
  // webview's lifetime (hooks below run unconditionally regardless).
  const composeDraftId = parseComposeRoute(window.location.pathname);
  const isComposeWindow = composeDraftId !== null;
  // tuxlink-0gsy: help webview branch — single-instance, no params (spec §4.1).
  const isHelpWindow = parseHelpRoute(window.location.pathname);
  // tuxlink-qjgx: logging webview branch — single-instance, no params (spec §8.1).
  const isLoggingWindow = parseLoggingRoute(window.location.pathname);
  // tuxlink-2phz: Station Data pop-out webview branch — single-instance, no params.
  const isStationsWindow = parseStationsRoute(window.location.pathname);
  // bd tuxlink-dmwte: pop-out webview branch — /pop/<surface> (spec §3). Not
  // single-instance (three distinct routes, one per surface), so this
  // yields the resolved SurfaceId rather than a boolean.
  const popSurface = parsePopRoute(window.location.pathname);

  // Amendment E.7.7: signal the backend that the main window's first paint is
  // complete so env-probe-runner can start its "after first paint" probes.
  // Only the main window emits this — secondary windows (compose, help,
  // logging, stations, pop-*) are not the target. Deferred via queueMicrotask
  // so React's commit phase finishes before the IPC call.
  useEffect(() => {
    if (isSecondaryWindow(window.location.pathname)) return;
    queueMicrotask(() => {
      invoke('emit_first_paint_complete').catch(() => {
        /* silently no-op if backend unavailable (e.g., logging in Degraded mode) */
      });
    });
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Wizard-completed probe — only meaningful for the main window. Skipped for
  // every secondary window kind (compose, help, logging, stations, pop-*) —
  // isSecondaryWindow is a superset of the four booleans this used to check
  // individually; behavior for those four is unchanged (adrev Codex-9).
  useEffect(() => {
    if (isSecondaryWindow(window.location.pathname)) return;
    invoke<boolean>('get_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Select the branch's content first; QueryClientProvider wraps the whole
  // tree below so every branch has access to react-query context (tuxlink-n4hz:
  // HelpView's useHelpSearch hook calls useQuery — without the provider the
  // help window crashes with "No QueryClient set"; tests didn't catch it
  // because each unit test wraps its own QueryClient. Lift the provider above
  // the branch switch so production matches the test-time assumption.)
  let content: ReactNode;
  if (isComposeWindow) {
    // Compose webview: render the compose form for its draft id and nothing
    // else (no wizard probe, no shell). Spec §5.4. Lazy-loaded — Suspense
    // fallback is an empty div so the index.html skeleton stays visible until
    // <Compose> hydrates rather than flashing a spinner.
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <Compose draftId={composeDraftId as string} />
      </Suspense>
    );
  } else if (isHelpWindow) {
    // Help webview: render <HelpView> for /help. Same lazy + Suspense pattern.
    // Spec §4.1.
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <HelpView />
      </Suspense>
    );
  } else if (isLoggingWindow) {
    // Logging webview: render <LoggingView> for /logging. Spec §8.1.
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <LoggingView />
      </Suspense>
    );
  } else if (isStationsWindow) {
    // Station Data pop-out webview: render <StationsView> for /stations
    // (tuxlink-2phz). Same lazy + Suspense pattern.
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <StationsView />
      </Suspense>
    );
  } else if (popSurface !== null) {
    // Popped-surface webview: render <PoppedSurfaceHost> for /pop/<surface>
    // (bd tuxlink-dmwte, spec §3/§4). Same lazy + Suspense pattern. Task 7
    // replaces the placeholder body.
    //
    // HintProvider wraps the surface for the same reason QueryClientProvider
    // wraps the whole tree (see the block comment above the branch switch): a
    // popped surface renders shared components that call `useFirstOpenTip()` —
    // AprsChatPanel's first-open tip is the concrete case — and `useHints()`
    // THROWS with no provider, blanking the pop window (caught here by the
    // WebKitGTK render smoke, tuxlink-dmwte task 11). The main window gets its
    // HintProvider from AppShell; a popped window has no AppShell, so it must
    // provide the ambient onboarding context the surface expects. By the time a
    // surface is poppable the tour is long complete (config `tour_completed`),
    // so no offer card appears here.
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <HintProvider>
          <PoppedSurfaceHost surface={popSurface} />
        </HintProvider>
      </Suspense>
    );
  } else if (wizardCompleted === null) {
    content = <div data-testid="app-loading">Loading…</div>;
  } else if (wizardCompleted) {
    // Post-wizard, the main shell renders (Task 12).
    content = <AppShell />;
  } else {
    // Pre-wizard, the onboarding wizard.
    // tuxlink-eh7: hand off to the shell the moment onboarding finishes — no
    // app restart needed (App.tsx otherwise reads wizard_completed only once).
    content = (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <Wizard onComplete={() => setWizardCompleted(true)} />
      </Suspense>
    );
  }

  return (
    <QueryClientProvider client={queryClient}>
      {/* tuxlink-52h6: app-wide catch-all. Without it, a single unguarded throw
          (the 0.60.0 maplibre map-init failure) unmounts the whole React root to
          a blank window. The boundary turns that into a reload-to-recover screen. */}
      <ErrorBoundary>{content}</ErrorBoundary>
    </QueryClientProvider>
  );
}
