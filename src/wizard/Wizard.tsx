import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';
import { Step2Credentials } from './Step2Credentials';
import { Step2OfflineIdentity } from './Step2OfflineIdentity';
import { Step3TestSend } from './Step3TestSend';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root" className="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {state.step === 'credentials' && <Step2Credentials />}
      {/* Task 11.5 (tuxlink-d76): offline identity path. */}
      {state.step === 'offline_identity' && <Step2OfflineIdentity />}
      {/* Task 11 (tuxlink-e4x): 4-substate test-send verification. */}
      {state.step === 'test_send' && <Step3TestSend />}
      {state.step === 'complete' && (
        <p data-testid="wizard-complete-placeholder">
          Wizard complete — main shell mounts via App.tsx routing.
        </p>
      )}
    </div>
  );
}

export function Wizard() {
  return (
    <WizardProvider>
      <WizardInner />
    </WizardProvider>
  );
}
