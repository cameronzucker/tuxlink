import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ArdopDock } from './ArdopDock';
import { STOPPED, type ModemStatus } from './types';

const mockUseModemStatus = vi.fn();
vi.mock('./useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
}));

// The Disconnect-button tests exercise the `modem_ardop_disconnect` invoke
// path; mock @tauri-apps/api/core's `invoke` so the test does not need a
// live Tauri runtime. The original stopped/running render tests do not
// click any Tauri-invoking buttons (Connect opens the modal; the modal's
// Connect is not exercised here), so the mock is a no-op for those.
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

beforeEach(() => {
  mockUseModemStatus.mockReset();
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined);
});

const RUNNING_FIXTURE: ModemStatus = {
  state: 'connected-irs',
  peer: 'W7RMS-10',
  mode: '4FSK 500',
  widthHz: 500,
  pttBackend: 'rts',
  snDb: 8.4,
  vuDbfs: -18.0,
  throughputBps: 540,
  bytesRx: 4128,
  bytesTx: 982,
  uptimeSec: 222,
  arqFlags: { busy: true, rx: true, tx: false },
  lastError: null,
};

describe('<ArdopDock> stopped', () => {
  it('renders the Connect form when status.state === stopped', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
    expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /connect/i })).toBeInTheDocument();
  });

  it('opens the consent modal when Connect is clicked', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    const input = screen.getByTestId('ardop-target');
    fireEvent.change(input, { target: { value: 'W7RMS-10' } });
    fireEvent.click(screen.getByRole('button', { name: /^connect$/i }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(screen.getByText(/About to transmit on amateur radio/i)).toBeInTheDocument();
  });
});

describe('<ArdopDock> running', () => {
  it('renders meters + peer + state grid when status.state === connected-irs', () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    expect(screen.getByText(/W7RMS-10/)).toBeInTheDocument();
    expect(screen.getByText(/\+8\.4 dB/)).toBeInTheDocument();
    expect(screen.getByText(/540 bps/)).toBeInTheDocument();
    expect(screen.getByTestId('arq-cell-CON')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-IRS')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-BUSY')).toHaveAttribute('data-on', 'true');
    expect(screen.getByTestId('arq-cell-TX')).toHaveAttribute('data-on', 'false');
  });

  it('does NOT render the Connect form when running', () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    // Use ^connect$ so this assertion doesn't accidentally match the new
    // Disconnect button in the running-state JSX (tuxlink-qvl).
    expect(screen.queryByRole('button', { name: /^connect$/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/target callsign/i)).not.toBeInTheDocument();
  });
});

describe('<ArdopDock> Disconnect', () => {
  it('renders a Disconnect button in the running state', () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    expect(screen.getByRole('button', { name: /disconnect/i })).toBeInTheDocument();
  });

  it('invokes modem_ardop_disconnect when clicked', async () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /disconnect/i }));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('modem_ardop_disconnect');
    });
  });

  it('does NOT render a Disconnect button in the stopped state', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    expect(screen.queryByRole('button', { name: /disconnect/i })).not.toBeInTheDocument();
  });
});
