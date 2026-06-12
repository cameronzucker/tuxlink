// StepLocation.test.tsx — wizard Location step (tuxlink-9xy1).
// The step wraps the shared GpsSourcePicker in wizard chrome and persists grid /
// source via the same config_set_grid / position_set_source commands as Settings.
// Continue advances location → complete (ADVANCE_FROM_LOCATION).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { WizardProvider, useWizard } from './wizardContext';
import { StepLocation } from './StepLocation';

// Backend mock: config_read seeds grid/source; the gps_probe_* commands feed the
// picker's detection (no working sources here — keeps the DOM to the manual card).
function mockBackend(over: { grid?: string | null; position_source?: string } = {}) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'config_read':
        return { grid: over.grid ?? null, position_source: over.position_source ?? 'Manual' } as unknown as never;
      case 'gps_probe_gpsd': return { reachable: false } as unknown as never;
      case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
      case 'gps_probe_dialout': return { member: true, groupExists: true } as unknown as never;
      case 'gps_probe_modemmanager': return { active: false } as unknown as never;
      default: return undefined as unknown as never;
    }
  });
}

function StepProbe() {
  const { state } = useWizard();
  return (
    <>
      <StepLocation />
      <div data-testid="probe-step">{state.step}</div>
    </>
  );
}

beforeEach(() => vi.mocked(invoke).mockReset());

describe('<StepLocation>', () => {
  it('renders the heading and the shared GPS source picker', async () => {
    mockBackend();
    render(<WizardProvider initialStateOverride={{ step: 'location' }}><StepLocation /></WizardProvider>);
    expect(screen.getByRole('heading', { name: /where is this station/i })).toBeInTheDocument();
    expect(await screen.findByTestId('gps-picker')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-location-continue')).toBeInTheDocument();
  });

  it('seeds the manual grid field from config_read', async () => {
    mockBackend({ grid: 'EM75', position_source: 'Manual' });
    render(<WizardProvider initialStateOverride={{ step: 'location' }}><StepLocation /></WizardProvider>);
    const input = (await screen.findByTestId('gps-manual-grid-input')) as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe('EM75'));
  });

  it('persists a valid manual grid via config_set_grid', async () => {
    mockBackend({ grid: '', position_source: 'Manual' });
    render(<WizardProvider initialStateOverride={{ step: 'location' }}><StepLocation /></WizardProvider>);
    const input = await screen.findByTestId('gps-manual-grid-input');
    fireEvent.change(input, { target: { value: 'EM75' } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('config_set_grid', { grid: 'EM75' }));
  });

  it('Continue advances the wizard from location → complete', async () => {
    mockBackend();
    render(<WizardProvider initialStateOverride={{ step: 'location' }}><StepProbe /></WizardProvider>);
    fireEvent.click(await screen.findByTestId('wizard-location-continue'));
    await waitFor(() => expect(screen.getByTestId('probe-step')).toHaveTextContent('complete'));
  });
});
