import { useEffect } from 'react';
import { WizardProvider, useWizard } from './wizardContext';
import './wizard.css';
import { Step1Welcome } from './Step1Welcome';
import { Step2Credentials } from './Step2Credentials';
import { Step2OfflineIdentity } from './Step2OfflineIdentity';
import { Step3TestSend } from './Step3TestSend';

interface WizardInnerProps {
  /** Called once the wizard reaches `complete`, so App.tsx can swap
   *  <Wizard/> → <AppShell/> without an app restart (tuxlink-eh7). */
  onComplete?: () => void;
}

export function WizardInner({ onComplete }: WizardInnerProps) {
  const { state } = useWizard();

  // Hand off to the main shell the instant onboarding finishes. Without this the
  // user is stranded on the `complete` step until an app restart re-reads
  // wizard_completed (tuxlink-eh7).
  useEffect(() => {
    if (state.step === 'complete') onComplete?.();
  }, [state.step, onComplete]);

  return (
    <div data-testid="wizard-root" className="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {state.step === 'credentials' && <Step2Credentials />}
      {/* Task 11.5 (tuxlink-d76): offline identity path. */}
      {state.step === 'offline_identity' && <Step2OfflineIdentity />}
      {/* Task 5.4 (tuxlink-9phd): connect-only CMS verification (no transmission). */}
      {state.step === 'cms_verify' && <Step3TestSend />}
      {/* Transient — App.tsx swaps to the shell via onComplete almost immediately. */}
      {state.step === 'complete' && (
        <p data-testid="wizard-complete-placeholder" className="wizard-complete-msg">
          Opening Tuxlink…
        </p>
      )}
    </div>
  );
}

export interface WizardProps {
  /** Invoked when onboarding completes (App.tsx routes to the shell). */
  onComplete?: () => void;
}

export function Wizard({ onComplete }: WizardProps) {
  return (
    <WizardProvider>
      <WizardInner onComplete={onComplete} />
    </WizardProvider>
  );
}
