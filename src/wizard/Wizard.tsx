import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';
import { Step2Credentials } from './Step2Credentials';
import { Step2OfflineIdentity } from './Step2OfflineIdentity';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root" className="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {state.step === 'credentials' && <Step2Credentials />}
      {/* Task 11.5 (tuxlink-d76): offline identity path — wired here. */}
      {state.step === 'offline_identity' && <Step2OfflineIdentity />}
      {/* Step 3 test-send mounts here in Task 11 (tuxlink-e4x). */}
      {state.step === 'test_send' && (
        <p data-testid="wizard-test-send-placeholder">
          Test send screen — Task 11 (tuxlink-e4x) renders here.
        </p>
      )}
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
