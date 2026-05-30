import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ArdopDock } from './ArdopDock';
import { STOPPED, type ModemStatus } from './types';

const mockUseModemStatus = vi.fn();
vi.mock('./useModemStatus', () => ({
  useModemStatus: () => mockUseModemStatus(),
}));

beforeEach(() => {
  mockUseModemStatus.mockReset();
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
    expect(screen.queryByRole('button', { name: /connect/i })).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/target callsign/i)).not.toBeInTheDocument();
  });
});
