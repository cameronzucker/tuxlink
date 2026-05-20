import { useEffect, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
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

/**
 * Generate a fresh draft id for a brand-new compose window. Stable per click;
 * the compose window keys its localStorage draft on this id.
 */
function newDraftId(): string {
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rand = Math.random().toString(36).slice(2, 8);
  return `draft-${ts}-${rand}`;
}

export default function App() {
  const [wizardCompleted, setWizardCompleted] = useState<boolean | null>(null);

  // Routing (spec §4.3 / §5.4): compose webviews load "/compose/<draftId>" and
  // must render <Compose>, NOT the main wizard/shell. Computed once — a webview
  // does not change its path at runtime, so the value is stable across the
  // webview's lifetime (hooks below run unconditionally regardless).
  const composeDraftId = parseComposeRoute(window.location.pathname);
  const isComposeWindow = composeDraftId !== null;

  // Main-window-ONLY: listen for the File → New Message menu event and open a
  // compose window. Codex F7 (menu.rs:123 broadcasts `menu` to EVERY webview
  // via app.emit): a compose window must NOT listen, or it recursively spawns
  // nested compose windows. We guard two ways: (1) skip on a compose route,
  // and (2) double-check the live window label === "main". This effect is
  // declared unconditionally (rules of hooks); the guards live inside it.
  useEffect(() => {
    if (isComposeWindow) return; // compose window — never listen (F7)

    let unlisten: (() => void) | undefined;
    let mounted = true;

    // Confirm this is the main window before subscribing (belt-and-suspenders
    // alongside the route guard — mirrors lib.rs's `window.label() == "main"`
    // close-to-tray guard).
    if (getCurrentWindow().label === 'main') {
      listen<string>('menu', (event) => {
        if (event.payload === 'menu:file:new') {
          invoke('compose_window_open', { draftId: newDraftId() }).catch(() => {
            /* window-open failure is non-fatal; surfaced via Rust logs */
          });
        }
      }).then((fn) => {
        if (mounted) unlisten = fn;
        else fn();
      });
    }

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [isComposeWindow]);

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
    <Wizard />
  );
}
