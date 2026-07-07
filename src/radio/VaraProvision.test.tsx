// VaraProvision.test.tsx — the shared VARA provisioning flow (tuxlink-w7212).
// Host-agnostic: driven by onComplete/onSkip callbacks. Covers the ready screen,
// skip, download-open, the pick → stream → done happy path, and the error path.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { VaraProvision } from './VaraProvision';

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

beforeEach(() => {
  progressHandler = null;
  (listen as Mock).mockImplementation((_n: string, cb: (e: { payload: EngineEvent }) => void) => {
    progressHandler = cb;
    return Promise.resolve(unlistenMock);
  });
  (openDialog as Mock).mockResolvedValue('/home/ham/Downloads/VARA.exe');
  (shellOpen as Mock).mockResolvedValue(undefined);
  (invoke as Mock).mockResolvedValue({ event: 'summary', ok: true });
});

afterEach(() => vi.clearAllMocks());

describe('<VaraProvision>', () => {
  it('renders the ready screen with download + choose', () => {
    render(<VaraProvision onComplete={vi.fn()} />);
    expect(screen.getByTestId('vara-provision-choose')).toBeTruthy();
    expect(screen.getByTestId('vara-provision-open-rosmodem')).toBeTruthy();
  });

  it('Skip calls onSkip (falling back to onComplete)', () => {
    const onComplete = vi.fn();
    render(<VaraProvision onComplete={onComplete} />);
    fireEvent.click(screen.getByTestId('vara-provision-skip'));
    expect(onComplete).toHaveBeenCalled();
  });

  it('opens the VARA author page in the browser', () => {
    render(<VaraProvision onComplete={vi.fn()} />);
    fireEvent.click(screen.getByTestId('vara-provision-open-rosmodem'));
    expect(shellOpen).toHaveBeenCalledWith('https://rosmodem.wordpress.com/');
  });

  it('picks an installer, streams progress, completes, and calls onComplete', async () => {
    let resolveInstall!: (v: unknown) => void;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'vara_install_start')
        return new Promise((res) => {
          resolveInstall = res;
        });
      return Promise.resolve(undefined);
    });
    const onComplete = vi.fn();
    render(<VaraProvision onComplete={onComplete} />);
    fireEvent.click(screen.getByTestId('vara-provision-choose'));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('vara_install_start', {
        installerPath: '/home/ham/Downloads/VARA.exe',
      }),
    );
    await waitFor(() => expect(progressHandler).not.toBeNull());
    act(() => {
      progressHandler?.({
        payload: { event: 'checkpoint', id: 'ocx', index: 5, total: 7, state: 'running' },
      });
    });
    expect(await screen.findByTestId('vara-provision-cp-ocx')).toBeTruthy();

    await act(async () => {
      resolveInstall({ event: 'summary', ok: true });
    });
    fireEvent.click(await screen.findByTestId('vara-provision-continue'));
    expect(onComplete).toHaveBeenCalled();
    expect(unlistenMock).toHaveBeenCalled();
  });

  it('surfaces an install failure and lets the user skip', async () => {
    (invoke as Mock).mockImplementation((cmd: string) =>
      cmd === 'vara_install_start'
        ? Promise.reject('[ocx] registration failed')
        : Promise.resolve(undefined),
    );
    const onComplete = vi.fn();
    render(<VaraProvision onComplete={onComplete} />);
    fireEvent.click(screen.getByTestId('vara-provision-choose'));
    expect(await screen.findByTestId('vara-provision-error')).toBeTruthy();
    expect(screen.getByText(/registration failed/)).toBeTruthy();
    fireEvent.click(screen.getByTestId('vara-provision-skip-after-error'));
    expect(onComplete).toHaveBeenCalled();
  });
});
