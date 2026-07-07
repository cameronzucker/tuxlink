// StepVaraProvision.test.tsx — the first-run wizard wrapper (tuxlink-w7212).
// Tests only the wrapper's concern: self-skip on unsupported hardware / unbundled
// engine, and delegation to the shared <VaraProvision>. The flow itself is tested
// in radio/VaraProvision.test.tsx.

import { describe, it, expect, vi, afterEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { StepVaraProvision } from './StepVaraProvision';
import type { WizardState } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

// Stub the shared flow so the wrapper test stays focused on self-skip/delegation.
vi.mock('../radio/VaraProvision', () => ({
  VaraProvision: ({ onComplete }: { onComplete: () => void }) => (
    <button data-testid="vara-provision-stub" onClick={onComplete}>
      provision
    </button>
  ),
}));

import { invoke } from '@tauri-apps/api/core';

function Probe() {
  const { state } = useWizard();
  return <div data-testid="probe-step">{state.step}</div>;
}

function renderStep() {
  const base: Partial<WizardState> = { step: 'vara_provision' };
  render(
    <WizardProvider initialStateOverride={base}>
      <StepVaraProvision />
      <Probe />
    </WizardProvider>,
  );
}

afterEach(() => vi.clearAllMocks());

describe('<StepVaraProvision> (wizard wrapper)', () => {
  it('self-skips to complete on non-x86_64 hardware', async () => {
    (invoke as Mock).mockImplementation((cmd: string) =>
      cmd === 'platform_info'
        ? Promise.resolve({ varaSupported: false })
        : Promise.resolve(true),
    );
    renderStep();
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });

  it('self-skips to complete when the engine is not bundled', async () => {
    (invoke as Mock).mockImplementation((cmd: string) =>
      cmd === 'platform_info'
        ? Promise.resolve({ varaSupported: true })
        : Promise.resolve(false),
    );
    renderStep();
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });

  it('renders the shared flow when VARA is supported + bundled', async () => {
    (invoke as Mock).mockImplementation((cmd: string) =>
      cmd === 'platform_info'
        ? Promise.resolve({ varaSupported: true })
        : Promise.resolve(true),
    );
    renderStep();
    expect(await screen.findByTestId('vara-provision-stub')).toBeTruthy();
    expect(screen.getByTestId('probe-step').textContent).toBe('vara_provision');
  });

  it('advances to complete when the shared flow signals done', async () => {
    (invoke as Mock).mockImplementation((cmd: string) =>
      cmd === 'platform_info'
        ? Promise.resolve({ varaSupported: true })
        : Promise.resolve(true),
    );
    renderStep();
    fireEvent.click(await screen.findByTestId('vara-provision-stub'));
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });
});
