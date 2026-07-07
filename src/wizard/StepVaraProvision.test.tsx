// StepVaraProvision.test.tsx — tuxlink-w7212
//
// Covers: self-skip on unsupported hardware / unbundled engine, the ready screen,
// skip → complete, opening a download page, and the full pick → stream → done →
// continue happy path (including a live progress checkpoint rendering).

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { WizardProvider, useWizard } from './wizardContext';
import { StepVaraProvision } from './StepVaraProvision';
import type { WizardState } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { open as shellOpen } from '@tauri-apps/plugin-shell';

interface EngineEvent {
  event: string;
  id?: string;
  index?: number;
  total?: number;
  state?: string;
}

let progressHandler: ((e: { payload: EngineEvent }) => void) | null = null;
const unlistenMock = vi.fn();

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

/** Default: x86_64 + engine bundled + install succeeds. */
function primeInvoke(overrides: Partial<Record<string, unknown>> = {}) {
  (invoke as unknown as Mock).mockImplementation((cmd: string) => {
    if (cmd === 'platform_info') return Promise.resolve({ varaSupported: true });
    if (cmd === 'vara_engine_available') return Promise.resolve(true);
    if (cmd === 'vara_install_start') return Promise.resolve({ event: 'summary', ok: true });
    if (cmd in overrides) return Promise.resolve(overrides[cmd]);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  progressHandler = null;
  (listen as Mock).mockImplementation((_name: string, cb: (e: { payload: EngineEvent }) => void) => {
    progressHandler = cb;
    return Promise.resolve(unlistenMock);
  });
  (openDialog as Mock).mockResolvedValue('/home/ham/Downloads/VARA.exe');
  (shellOpen as Mock).mockResolvedValue(undefined);
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('<StepVaraProvision>', () => {
  it('self-skips to complete on non-x86_64 hardware', async () => {
    (invoke as unknown as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'platform_info') return Promise.resolve({ varaSupported: false });
      if (cmd === 'vara_engine_available') return Promise.resolve(true);
      return Promise.resolve(undefined);
    });
    renderStep();
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });

  it('self-skips to complete when the engine is not bundled', async () => {
    (invoke as unknown as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'platform_info') return Promise.resolve({ varaSupported: true });
      if (cmd === 'vara_engine_available') return Promise.resolve(false);
      return Promise.resolve(undefined);
    });
    renderStep();
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });

  it('shows the ready screen when VARA is supported and bundled', async () => {
    primeInvoke();
    renderStep();
    expect(await screen.findByTestId('wizard-vara-choose')).toBeTruthy();
    expect(screen.getByTestId('wizard-vara-open-rosmodem')).toBeTruthy();
    // still on the step (not advanced)
    expect(screen.getByTestId('probe-step').textContent).toBe('vara_provision');
  });

  it('Skip advances to complete without installing', async () => {
    primeInvoke();
    renderStep();
    const skip = await screen.findByTestId('wizard-vara-skip');
    fireEvent.click(skip);
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
    expect(invoke).not.toHaveBeenCalledWith('vara_install_start', expect.anything());
  });

  it('opens the VARA author page in the browser', async () => {
    primeInvoke();
    renderStep();
    const btn = await screen.findByTestId('wizard-vara-open-rosmodem');
    fireEvent.click(btn);
    expect(shellOpen).toHaveBeenCalledWith('https://rosmodem.wordpress.com/');
  });

  it('picks an installer, streams progress, and completes', async () => {
    // Hold the install pending so the 'installing' phase (which renders the
    // checklist) is observable while we emit a progress checkpoint.
    let resolveInstall!: (v: unknown) => void;
    (invoke as unknown as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'platform_info') return Promise.resolve({ varaSupported: true });
      if (cmd === 'vara_engine_available') return Promise.resolve(true);
      if (cmd === 'vara_install_start')
        return new Promise((res) => {
          resolveInstall = res;
        });
      return Promise.resolve(undefined);
    });
    renderStep();
    const choose = await screen.findByTestId('wizard-vara-choose');
    fireEvent.click(choose);

    // installer path is forwarded to the install command
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('vara_install_start', {
        installerPath: '/home/ham/Downloads/VARA.exe',
      }),
    );

    // while the install is in flight, a progress checkpoint renders in the checklist
    await waitFor(() => expect(progressHandler).not.toBeNull());
    act(() => {
      progressHandler?.({
        payload: { event: 'checkpoint', id: 'ocx', index: 5, total: 7, state: 'running' },
      });
    });
    expect(await screen.findByTestId('wizard-vara-cp-ocx')).toBeTruthy();

    // resolve the install → done → Continue advances to complete
    await act(async () => {
      resolveInstall({ event: 'summary', ok: true });
    });
    const cont = await screen.findByTestId('wizard-vara-continue');
    fireEvent.click(cont);
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
    expect(unlistenMock).toHaveBeenCalled();
  });

  it('surfaces an install failure and lets the user skip', async () => {
    primeInvoke({});
    (invoke as unknown as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'platform_info') return Promise.resolve({ varaSupported: true });
      if (cmd === 'vara_engine_available') return Promise.resolve(true);
      if (cmd === 'vara_install_start') return Promise.reject('[ocx] registration failed');
      return Promise.resolve(undefined);
    });
    renderStep();
    const choose = await screen.findByTestId('wizard-vara-choose');
    fireEvent.click(choose);
    expect(await screen.findByTestId('wizard-vara-error')).toBeTruthy();
    expect(screen.getByText(/registration failed/)).toBeTruthy();
    fireEvent.click(screen.getByTestId('wizard-vara-skip-after-error'));
    await waitFor(() => expect(screen.getByTestId('probe-step').textContent).toBe('complete'));
  });
});
