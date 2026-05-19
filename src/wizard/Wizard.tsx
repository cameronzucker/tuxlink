import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';

function WizardInner() {
  const { state } = useWizard();
  return (
    <div data-testid="wizard-root" className="wizard-root">
      {state.step === 'account' && <Step1Welcome />}
      {/* Step 2 (credentials / offline) + Step 3 (test send) mount here in
          Tasks 10 / 11 / 11.5. Until then, choosing a card on Step 1 routes
          to one of these placeholders. */}
      {state.step === 'credentials' && (
        <p data-testid="wizard-credentials-placeholder">
          Credentials screen — Task 10 (tuxlink-1r5) renders here.
        </p>
      )}
      {state.step === 'offline_identity' && (
        <p data-testid="wizard-offline-placeholder">
          Offline identity screen — Task 11.5 (tuxlink-d76) renders here.
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
