// StepClassifierWeights.tsx — first-run wizard wrapper around the shared
// classifier-weights acquisition surface (tuxlink-13ofm).
//
// The wizard-specific concern is only step advancement: "Continue" when
// ready, "Continue setup while it downloads" (the job persists and the Elmer
// panel takes over as the gate surface), and "Skip for now" (skip is first
// class — the field case installs from a folder later). Everything else
// (download job, progress, retry, switch-source, sideload) lives in the
// shared <ClassifierWeights> so the Elmer panel renders the exact same flow.

import { useCallback } from 'react';
import { useWizard } from './wizardContext';
import { ClassifierWeights } from '../classify/ClassifierWeights';

export function StepClassifierWeights() {
  const { dispatch } = useWizard();

  const advance = useCallback(
    () => dispatch({ type: 'ADVANCE_FROM_CLASSIFIER_WEIGHTS' }),
    [dispatch],
  );

  return (
    <div
      className="wizard-step wizard-step-classifier-weights"
      data-testid="wizard-step-classifier-weights"
    >
      <h1>On-device classifier model</h1>
      <ClassifierWeights variant="wizard" onContinue={advance} onSkip={advance} />
    </div>
  );
}
