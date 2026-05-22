import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import { WizardProvider } from './wizardContext';
import { WizardInner } from './Wizard';

// tuxlink-eh7: completing the wizard must hand off to the app shell (App.tsx
// swaps <Wizard/> → <AppShell/> via onComplete). Previously the wizard reached
// `complete` and rendered a dev placeholder forever — the shell only mounted on
// an app restart.
describe('Wizard completion hand-off', () => {
  it('fires onComplete when the wizard reaches the complete step', () => {
    const onComplete = vi.fn();
    render(
      <WizardProvider initialStateOverride={{ step: 'complete' }}>
        <WizardInner onComplete={onComplete} />
      </WizardProvider>,
    );
    expect(onComplete).toHaveBeenCalledOnce();
  });

  it('does NOT fire onComplete before the wizard is complete', () => {
    const onComplete = vi.fn();
    render(
      <WizardProvider initialStateOverride={{ step: 'account' }}>
        <WizardInner onComplete={onComplete} />
      </WizardProvider>,
    );
    expect(onComplete).not.toHaveBeenCalled();
  });

  it('shows no developer-facing placeholder text on completion', () => {
    const { container } = render(
      <WizardProvider initialStateOverride={{ step: 'complete' }}>
        <WizardInner onComplete={vi.fn()} />
      </WizardProvider>,
    );
    expect(container.textContent ?? '').not.toMatch(/App\.tsx|routing|mounts via/i);
  });
});
