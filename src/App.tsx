import { useEffect, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { Wizard } from './wizard/Wizard';
import { AppShell } from './shell/AppShell';
import { Compose } from './compose/Compose';
import { parseComposeRoute } from './routing';
import './App.css';

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

  // Wizard-completed probe — only meaningful for the main window. Skipped for
  // compose windows (which render <Compose> below regardless).
  useEffect(() => {
    if (isComposeWindow) return;
    invoke<boolean>('get_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, [isComposeWindow]);

  // Compose webview: render the compose form for its draft id and nothing else
  // (no wizard probe, no shell). Spec §5.4.
  if (isComposeWindow) {
    return <Compose draftId={composeDraftId as string} />;
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
    <Wizard onComplete={() => setWizardCompleted(true)} />
  );
}
