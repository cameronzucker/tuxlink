// StepLocation.test.tsx — wizard Location step (tuxlink-9xy1).
// The step wraps the shared GpsSourcePicker in wizard chrome and persists grid /
// source via the same config_set_grid / position_set_source commands as Settings.
// Continue advances location → complete (ADVANCE_FROM_LOCATION).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Stub the offline map so the wizard test doesn't mount leaflet in jsdom. Expose
// a testid so the reachability assertion can confirm the map renders.
vi.mock('../location/LocationMap', () => ({
  LocationMap: () => <div data-testid="location-map-stub" />,
}));
import { invoke } from '@tauri-apps/api/core';
import { WizardProvider, useWizard } from './wizardContext';
import { StepLocation } from './StepLocation';

// Backend mock: config_read seeds grid/source; the gps_probe_* commands feed the
// picker's detection; position_status feeds the live readout poll. `dialout`
// defaults to member:true so the default fixture shows no triage (the manual /
// reachability tests override it).
function mockBackend(
  over: {
    grid?: string | null;
    position_source?: string;
    dialout?: { member: boolean; groupExists: boolean };
    gpsd?: { reachable: boolean };
  } = {},
) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'config_read':
        return { grid: over.grid ?? null, position_source: over.position_source ?? 'Manual' } as unknown as never;
      case 'position_status':
        return { gps_ready: false, broadcast_grid: '', ui_grid: '', fix_lat: null, fix_lon: null } as unknown as never;
      case 'gps_probe_gpsd': return (over.gpsd ?? { reachable: false }) as unknown as never;
      case 'gps_probe_serial_devices': return { devices: [] } as unknown as never;
      case 'gps_probe_dialout': return (over.dialout ?? { member: true, groupExists: true }) as unknown as never;
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
    expect(screen.getByRole('heading', { name: /set up your location/i })).toBeInTheDocument();
    expect(await screen.findByTestId('gps-picker')).toBeInTheDocument();
    expect(screen.getByTestId('wizard-location-continue')).toBeInTheDocument();
  });

  // tuxlink-yy1m reachability regression: from a clean install with NO GPS device
  // and a broken dialout, the step must show the confirm map AND the Linux
  // diagnostics — NOT a blank grid box (the defect that motivated this feature).
  it('shows the map + Linux diagnostics even with no GPS device', async () => {
    mockBackend({ gpsd: { reachable: false }, dialout: { member: false, groupExists: true } });
    render(<WizardProvider initialStateOverride={{ step: 'location' }}><StepLocation /></WizardProvider>);
    expect(await screen.findByTestId('location-map-stub')).toBeInTheDocument();
    expect(await screen.findByTestId('gps-triage-dialout')).toBeInTheDocument();
    expect(screen.getByTestId('gps-no-device')).toBeInTheDocument();
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
