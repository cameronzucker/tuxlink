import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Wizard } from './wizard/Wizard';
import './App.css';

function MainShell() {
  return (
    <div data-testid="main-shell-root" className="container">
      Main shell — Tasks 12+ will render the inbox here.
    </div>
  );
}

export default function App() {
  const [wizardCompleted, setWizardCompleted] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>('get_wizard_completed')
      .then(setWizardCompleted)
      .catch(() => setWizardCompleted(false));
  }, []);

  if (wizardCompleted === null) return <div data-testid="app-loading">Loading…</div>;
  return wizardCompleted ? <MainShell /> : <Wizard />;
}
