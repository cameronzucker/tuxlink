import { useEffect, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { invoke } from '@tauri-apps/api/core';
import { Wizard } from './wizard/Wizard';
import { AppShell } from './shell/AppShell';
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

  useEffect(() => {
    invoke<boolean>('get_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, []);

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
