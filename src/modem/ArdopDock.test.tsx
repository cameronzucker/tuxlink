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
  quality: null,
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

describe('<ArdopDock> Send/Receive', () => {
  it('renders the Send/Receive button when connected-irs', () => {
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs' },
      loading: false,
    });
    render(<ArdopDock />);
    expect(screen.getByRole('button', { name: /send\/receive/i })).toBeInTheDocument();
  });

  it('renders the Send/Receive button when connected-iss', () => {
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-iss' },
      loading: false,
    });
    render(<ArdopDock />);
    expect(screen.getByRole('button', { name: /send\/receive/i })).toBeInTheDocument();
  });

  it('does NOT render the Send/Receive button when stopped (no running view at all)', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    expect(screen.queryByRole('button', { name: /send\/receive/i })).not.toBeInTheDocument();
  });

  it('renders the Send/Receive button as disabled when connecting (not yet ready)', () => {
    // The button is present in the running view but disabled until the
    // backend reports ConnectedIrs/ConnectedIss — the operator can see the
    // affordance, just not click it yet.
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connecting' },
      loading: false,
    });
    render(<ArdopDock />);
    const btn = screen.getByRole('button', { name: /send\/receive/i });
    expect(btn).toBeInTheDocument();
    expect(btn).toBeDisabled();
  });

  it('mints consent THEN invokes modem_ardop_b2f_exchange on click, in that order', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'modem_mint_consent') return Promise.resolve('test-token-456');
      if (cmd === 'modem_ardop_b2f_exchange') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs', peer: 'W7RMS-10' },
      loading: false,
    });
    render(<ArdopDock />);

    fireEvent.click(screen.getByRole('button', { name: /send\/receive/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('modem_mint_consent');
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('modem_ardop_b2f_exchange', {
        target: 'W7RMS-10',
        consentToken: 'test-token-456',
      });
    });

    // Verify the order: mint before exchange. This is the RADIO-1 property
    // — a backend-minted token must precede the transmit-triggering call.
    const calls = mockInvoke.mock.calls.map((c) => c[0]);
    const mintIdx = calls.indexOf('modem_mint_consent');
    const exchangeIdx = calls.indexOf('modem_ardop_b2f_exchange');
    expect(mintIdx).toBeGreaterThanOrEqual(0);
    expect(exchangeIdx).toBeGreaterThanOrEqual(0);
    expect(mintIdx).toBeLessThan(exchangeIdx);
  });

  it('surfaces a backend error in the dock error slot on Send/Receive failure', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'modem_mint_consent') return Promise.resolve('tok');
      if (cmd === 'modem_ardop_b2f_exchange') return Promise.reject('B2F handshake timeout');
      return Promise.resolve(undefined);
    });
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs' },
      loading: false,
    });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /send\/receive/i }));
    await waitFor(() => {
      expect(screen.getByText(/Send\/Receive failed.*B2F handshake timeout/i)).toBeInTheDocument();
    });
  });

  it('disables Send/Receive when status.peer is null even in connected-irs', () => {
    // RADIO-1: the backend-reported peer is the single source of truth
    // for the TX target. If status.peer is null while ConnectedIrs (an
    // unlikely backend state, but defendable from the renderer's side),
    // the Send/Receive button must NOT be clickable. The prior derivation
    // `(status.peer ?? target).trim()` would have fallen back to the
    // hidden stopped-state input's persisted value — a stale-callsign
    // hazard.
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs', peer: null },
      loading: false,
    });
    render(<ArdopDock />);
    const btn = screen.queryByRole('button', { name: /send\/receive/i });
    // The button may render (running view is mounted) but must be disabled.
    // Align with the project's visibility-vs-disabled pattern: the button
    // is in the DOM in the running view; the disabled prop gates the click.
    if (btn) {
      expect(btn).toBeDisabled();
    }
  });

  it('does NOT fall back to stopped-state target when peer is null', async () => {
    // Defense in depth: even if a future regression re-enables the
    // button when status.peer is null, the onClick handler's early-return
    // must prevent the on-air invoke chain from firing.
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs', peer: null },
      loading: false,
    });
    render(<ArdopDock />);
    const btn = screen.queryByRole('button', { name: /send\/receive/i });
    if (btn && !(btn as HTMLButtonElement).disabled) {
      fireEvent.click(btn);
    }
    // Neither the consent-mint nor the B2F-exchange invoke should fire.
    expect(mockInvoke).not.toHaveBeenCalledWith('modem_mint_consent');
    expect(mockInvoke).not.toHaveBeenCalledWith(
      'modem_ardop_b2f_exchange',
      expect.anything(),
    );
  });

  it('does NOT render the Open WebGUI button when stopped (no running view at all)', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    expect(screen.queryByRole('button', { name: /open webgui/i })).not.toBeInTheDocument();
  });

  it('shows the in-flight "Exchanging…" label while the exchange is pending', async () => {
    // Hold the exchange promise open so the in-flight label is observable.
    // Capture the resolver via an outer object so TypeScript doesn't narrow
    // the binding to `never` based on the synchronous initializer.
    const pending: { resolve: (v?: unknown) => void } = { resolve: () => {} };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'modem_mint_consent') return Promise.resolve('tok');
      if (cmd === 'modem_ardop_b2f_exchange') {
        return new Promise<unknown>((res) => {
          pending.resolve = res;
        });
      }
      return Promise.resolve(undefined);
    });
    mockUseModemStatus.mockReturnValue({
      status: { ...RUNNING_FIXTURE, state: 'connected-irs' },
      loading: false,
    });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /send\/receive/i }));
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /exchanging/i })).toBeInTheDocument();
    });
    // Release the exchange so the test exits cleanly.
    pending.resolve(undefined);
  });
});

describe('<ArdopDock> Open WebGUI (tuxlink-60wh)', () => {
  it('renders an Open WebGUI button in the running state', () => {
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    expect(screen.getByRole('button', { name: /open webgui/i })).toBeInTheDocument();
  });

  it('does NOT render the Open WebGUI button in the stopped state', () => {
    mockUseModemStatus.mockReturnValue({ status: STOPPED, loading: false });
    render(<ArdopDock />);
    expect(screen.queryByRole('button', { name: /open webgui/i })).not.toBeInTheDocument();
  });

  it('reads cmd_port from config and opens http://localhost:<cmd_port-1>/ when clicked', async () => {
    // Default cmd_port=8515 → webgui_port=8514 per ardopcf convention.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'config_get_ardop') {
        return Promise.resolve({ cmd_port: 8515 });
      }
      return Promise.resolve(undefined);
    });
    const windowOpenSpy = vi
      .spyOn(window, 'open')
      .mockImplementation(() => null);

    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /open webgui/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('config_get_ardop');
    });
    await waitFor(() => {
      expect(windowOpenSpy).toHaveBeenCalledWith('http://localhost:8514/', '_blank');
    });

    windowOpenSpy.mockRestore();
  });

  it('uses the operator-configured cmd_port (not a hardcoded 8514)', async () => {
    // Operator overrides cmd_port=9001 → webgui_port=9000.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'config_get_ardop') {
        return Promise.resolve({ cmd_port: 9001 });
      }
      return Promise.resolve(undefined);
    });
    const windowOpenSpy = vi
      .spyOn(window, 'open')
      .mockImplementation(() => null);

    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /open webgui/i }));

    await waitFor(() => {
      expect(windowOpenSpy).toHaveBeenCalledWith('http://localhost:9000/', '_blank');
    });

    windowOpenSpy.mockRestore();
  });

  it('surfaces an actionable error when cmd_port is too low to derive a webgui_port', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'config_get_ardop') {
        return Promise.resolve({ cmd_port: 1 });
      }
      return Promise.resolve(undefined);
    });
    const windowOpenSpy = vi
      .spyOn(window, 'open')
      .mockImplementation(() => null);

    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /open webgui/i }));

    await waitFor(() => {
      expect(screen.getByText(/cannot open webgui/i)).toBeInTheDocument();
    });
    // No URL was opened — the guard fired first.
    expect(windowOpenSpy).not.toHaveBeenCalled();

    windowOpenSpy.mockRestore();
  });

  it('surfaces a backend error in the dock error slot on config_get_ardop failure', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'config_get_ardop') return Promise.reject('read config: file not found');
      return Promise.resolve(undefined);
    });
    mockUseModemStatus.mockReturnValue({ status: RUNNING_FIXTURE, loading: false });
    render(<ArdopDock />);
    fireEvent.click(screen.getByRole('button', { name: /open webgui/i }));
    await waitFor(() => {
      expect(
        screen.getByText(/failed to open webgui.*read config: file not found/i),
      ).toBeInTheDocument();
    });
  });
});
