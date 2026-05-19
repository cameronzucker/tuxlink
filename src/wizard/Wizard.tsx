import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';
import { Step2Credentials } from './Step2Credentials';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root" className="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {state.step === 'credentials' && <Step2Credentials />}
      {/* Step 2 offline + Step 3 test-send mount here in Tasks 11.5 / 11. */}
      {state.step === 'offline_identity' && (
        <p data-testid="wizard-offline-placeholder">
          Offline identity screen — Task 11.5 (tuxlink-d76) renders here.
        </p>
      )}
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
