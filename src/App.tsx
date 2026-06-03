import { useEffect, useState, lazy, Suspense } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { AppShell } from './shell/AppShell';
import { parseComposeRoute, parseHelpRoute } from './routing';
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

  // Wizard-completed probe — only meaningful for the main window. Skipped for
  // compose windows (which render <Compose> below regardless) and help windows
  // (which render <HelpView> below regardless).
  useEffect(() => {
    if (isComposeWindow || isHelpWindow) return;
    invoke<boolean>('get_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, [isComposeWindow, isHelpWindow]);

  // Compose webview: render the compose form for its draft id and nothing else
  // (no wizard probe, no shell). Spec §5.4. Lazy-loaded — Suspense fallback is
  // an empty div so the index.html skeleton stays visible until <Compose>
  // hydrates rather than flashing a spinner.
  if (isComposeWindow) {
    return (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <Compose draftId={composeDraftId as string} />
      </Suspense>
    );
  }

  // Help webview: render <HelpView> for /help. Same lazy + Suspense pattern.
  // Spec §4.1.
  if (isHelpWindow) {
    return (
      <Suspense fallback={<div data-testid="app-loading" />}>
        <HelpView />
      </Suspense>
    );
  }

  if (wizardCompleted === null) return <div data-testid="app-loading">Loading…</div>;

  // Post-wizard, the main shell renders (Task 12). Pre-wizard, the onboarding
  // wizard. The shell needs the QueryClient for its mailbox queries.
  return wizardCompleted ? (
    <QueryClientProvider client={queryClient}>
      <AppShell />
    </QueryClientProvider>
  ) : (
    // tuxlink-eh7: hand off to the shell the moment onboarding finishes — no
    // app restart needed (App.tsx otherwise reads wizard_completed only once).
    <Suspense fallback={<div data-testid="app-loading" />}>
      <Wizard onComplete={() => setWizardCompleted(true)} />
    </Suspense>
  );
}
