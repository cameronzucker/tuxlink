import { describe, it, expect } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { Step1Welcome } from './Step1Welcome';

function StepWithProbe() {
  const { state } = useWizard();
  return (
    <>
      <Step1Welcome />
      <div data-testid="probe-step">{state.step}</div>
      <div data-testid="probe-cms">{String(state.connectToCms)}</div>
    </>
  );
}

describe('<Step1Welcome>', () => {
  it('renders the canonical question + both choice cards', () => {
    render(<WizardProvider><Step1Welcome /></WizardProvider>);
    expect(screen.getByText(/Will this installation connect to the Winlink CMS/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Yes, connect to the Winlink CMS/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /No, this is an offline/i })).toBeInTheDocument();
  });

  it('CMS card click → SET_CONNECT_TO_CMS(true) + ADVANCE → step=credentials', () => {
    render(<WizardProvider><StepWithProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /Yes, connect/i }));
    expect(screen.getByTestId('probe-cms')).toHaveTextContent('true');
    expect(screen.getByTestId('probe-step')).toHaveTextContent('credentials');
  });

  it('Offline card click → SET_CONNECT_TO_CMS(false) + ADVANCE → step=offline_identity', () => {
    render(<WizardProvider><StepWithProbe /></WizardProvider>);
    fireEvent.click(screen.getByRole('button', { name: /No, this is an offline/i }));
    expect(screen.getByTestId('probe-cms')).toHaveTextContent('false');
    expect(screen.getByTestId('probe-step')).toHaveTextContent('offline_identity');
  });
});
