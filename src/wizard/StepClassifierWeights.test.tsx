// StepClassifierWeights.test.tsx — the first-run wizard wrapper
// (tuxlink-13ofm). Tests only the wrapper's concern: delegation to the shared
// <ClassifierWeights> and step advancement on continue/skip. The acquisition
// flow itself is tested in classify/ClassifierWeights.test.tsx.

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { StepClassifierWeights } from './StepClassifierWeights';
import type { WizardState } from './types';

// Stub the shared flow so the wrapper test stays focused on advancement.
vi.mock('../classify/ClassifierWeights', () => ({
  ClassifierWeights: ({
    variant,
    onContinue,
    onSkip,
  }: {
    variant: string;
    onContinue: () => void;
    onSkip: () => void;
  }) => (
    <div data-testid="classifier-weights-stub" data-variant={variant}>
      <button data-testid="stub-continue" onClick={onContinue}>
        continue
      </button>
      <button data-testid="stub-skip" onClick={onSkip}>
        skip
      </button>
    </div>
  ),
}));

function Probe() {
  const { state } = useWizard();
  return <div data-testid="probe-step">{state.step}</div>;
}

function renderStep() {
  const base: Partial<WizardState> = { step: 'classifier_weights' };
  render(
    <WizardProvider initialStateOverride={base}>
      <StepClassifierWeights />
      <Probe />
    </WizardProvider>,
  );
}

afterEach(() => vi.clearAllMocks());

describe('<StepClassifierWeights> (wizard wrapper)', () => {
  it('renders the shared flow in wizard variant', () => {
    renderStep();
    expect(screen.getByTestId('wizard-step-classifier-weights')).toBeInTheDocument();
    expect(screen.getByTestId('classifier-weights-stub').dataset.variant).toBe('wizard');
  });

  it('advances to complete on continue', () => {
    renderStep();
    fireEvent.click(screen.getByTestId('stub-continue'));
    expect(screen.getByTestId('probe-step').textContent).toBe('complete');
  });

  it('advances to complete on skip — skip is first class', () => {
    renderStep();
    fireEvent.click(screen.getByTestId('stub-skip'));
    expect(screen.getByTestId('probe-step').textContent).toBe('complete');
  });
});
