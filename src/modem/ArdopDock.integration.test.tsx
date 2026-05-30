import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ArdopDock } from './ArdopDock';
import { STOPPED, type ModemStatus } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const CONNECTED_FIXTURE: ModemStatus = {
  state: 'connected-irs',
  peer: 'W7RMS-10',
  mode: '4FSK 500',
  widthHz: 500,
  pttBackend: 'rts',
  snDb: 8.4,
  vuDbfs: -18.0,
  throughputBps: 540,
  bytesRx: 0,
  bytesTx: 0,
  uptimeSec: 1,
  arqFlags: { busy: false, rx: false, tx: false },
  lastError: null,
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('ArdopDock end-to-end consent + connect flow', () => {
  it('stopped → ack → mint consent → connect → running', async () => {
    let emitListener: ((e: { payload: ModemStatus }) => void) | null = null;
    (listen as unknown as ReturnType<typeof vi.fn>).mockImplementation(
      (_event: string, cb: (e: { payload: ModemStatus }) => void) => {
        emitListener = cb;
        return Promise.resolve(() => {});  // unsubscribe fn
      }
    );
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'modem_get_status') return Promise.resolve(STOPPED);
      if (cmd === 'modem_mint_consent') return Promise.resolve('test-token-123');
      if (cmd === 'modem_ardop_connect') {
        // Simulate the backend transitioning to ConnectedIrs and emitting an event.
        // Schedule the emission on the next tick so the awaiting code sees it post-resolve.
        setTimeout(() => emitListener?.({ payload: CONNECTED_FIXTURE }), 0);
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    render(<ArdopDock />);

    // 1. Stopped: dock shows the Connect form.
    await waitFor(() => {
      expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument();
    });

    // 2. Operator types a target.
    fireEvent.change(screen.getByTestId('ardop-target'), {
      target: { value: 'W7RMS-10' },
    });

    // 3. Click Connect (dock-level).
    fireEvent.click(screen.getByRole('button', { name: /^connect$/i }));

    // 4. Consent modal appears with W7RMS-10.
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText(/W7RMS-10/)).toBeInTheDocument();

    // 5. Tick the ack checkbox.
    fireEvent.click(within(dialog).getByRole('checkbox'));

    // 6. Click the modal's Connect button (scoped to the dialog — the dock
    //    also has a Connect button, so the global query would be ambiguous).
    fireEvent.click(within(dialog).getByRole('button', { name: /^connect$/i }));

    // 7. Frontend invoked modem_mint_consent, then modem_ardop_connect with the minted token.
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('modem_mint_consent');
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('modem_ardop_connect', {
        target: 'W7RMS-10',
        consentToken: 'test-token-123',
      });
    });

    // 8. Backend emits status; the dock re-renders into running state.
    // (The setTimeout-scheduled emission needs a tick to fire; waitFor will retry.)
    await waitFor(() => {
      expect(screen.getByText(/W7RMS-10/)).toBeInTheDocument();
    });
  });

  it('Cancel on consent modal does NOT invoke modem_ardop_connect', async () => {
    (listen as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(() => {});
    (invoke as unknown as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
      if (cmd === 'modem_get_status') return Promise.resolve(STOPPED);
      return Promise.resolve(undefined);
    });

    render(<ArdopDock />);
    await waitFor(() => {
      expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument();
    });
    fireEvent.change(screen.getByTestId('ardop-target'), {
      target: { value: 'W7RMS-10' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^connect$/i }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    // Critical: no modem_ardop_connect invocation.
    expect(invoke).not.toHaveBeenCalledWith('modem_ardop_connect', expect.anything());
    // Also no modem_mint_consent (modal was canceled before confirming).
    expect(invoke).not.toHaveBeenCalledWith('modem_mint_consent');
  });
});
