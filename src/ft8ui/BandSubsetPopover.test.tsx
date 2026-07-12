// src/ft8ui/BandSubsetPopover.test.tsx — Task C10.
//
// `invoke` is mocked at module level, GATED ON `cmd` (feedback_vitest_invoke_
// mock_cleanup_call — vitest's stray no-arg teardown call must be inert; the
// repo idiom useFt8Listener.test.ts already established).

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { BandSubsetPopover, type BandSubsetPopoverProps } from './BandSubsetPopover';
import type { SweepConfigDto } from './ft8Types';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

function sweepConfig(overrides: Partial<SweepConfigDto> = {}): SweepConfigDto {
  return { enabled: false, bands: ['20m'], dwellSlots: 4, ...overrides };
}

function renderPopover(overrides: Partial<BandSubsetPopoverProps> = {}) {
  const props: BandSubsetPopoverProps = {
    sweepConfig: sweepConfig(),
    heldBand: '20m',
    isListening: false,
    fallbackHold: false,
    ...overrides,
  };
  return render(<BandSubsetPopover {...props} />);
}

beforeEach(() => {
  invokeMock.mockReset();
  // Default: ft8_cat_probe never resolves unless a test overrides it — keeps
  // the "absent" (no probe result yet) case exercisable deterministically.
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'ft8_cat_probe') return new Promise(() => {});
    return Promise.resolve();
  });
});

describe('BandSubsetPopover — reads sweepConfig (config truth)', () => {
  it('reflects the configured bands as selected chips', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['40m', '20m'] }) });
    expect(screen.getByTestId('band-subset-chip-40m')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('band-subset-chip-20m')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('band-subset-chip-80m')).toHaveAttribute('aria-pressed', 'false');
  });

  it('checks the Hold-one radio and unchecks Sweep when config.enabled is false', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    expect(screen.getByTestId('band-subset-mode-hold')).toBeChecked();
    expect(screen.getByTestId('band-subset-mode-sweep')).not.toBeChecked();
  });

  it('checks the Sweep radio when config.enabled is true, even mid-fallback-hold', async () => {
    invokeMock.mockResolvedValue(undefined);
    renderPopover({
      sweepConfig: sweepConfig({ enabled: true }),
      fallbackHold: true,
    });
    // Config truth wins immediately — no probe-await needed to see it checked.
    expect(screen.getByTestId('band-subset-mode-sweep')).toBeChecked();
    expect(screen.getByTestId('band-subset-mode-sweep')).not.toBeDisabled();
  });
});

describe('BandSubsetPopover — hold-one mode: chips single-select the HELD band', () => {
  // The approved states mock's popover card ("no CAT, operator hasn't clicked a
  // band") makes the chip click the operator's band assertion. The original C10
  // brief's inert-chips reading contradicted that mock and left ft8_set_band
  // with ZERO UI callers — a held band the operator could never change.
  it('marks the held band selected (not the sweep subset) while hold-one', () => {
    renderPopover({
      sweepConfig: sweepConfig({ enabled: false, bands: ['40m', '15m'] }),
      heldBand: '20m',
    });
    expect(screen.getByTestId('band-subset-chip-20m')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('band-subset-chip-40m')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByTestId('band-subset-chip-15m')).toHaveAttribute('aria-pressed', 'false');
  });

  it('clicking a different band invokes ft8_set_band — never the sweep-bands command', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: false, bands: ['20m'] }), heldBand: '20m' });
    fireEvent.click(screen.getByTestId('band-subset-chip-40m'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_band', { band: '40m' });
    expect(invokeMock).not.toHaveBeenCalledWith('ft8_set_sweep_bands', expect.anything());
  });

  it('clicking the already-held band invokes nothing (no redundant retune)', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: false, bands: ['20m'] }), heldBand: '20m' });
    fireEvent.click(screen.getByTestId('band-subset-chip-20m'));
    expect(invokeMock).not.toHaveBeenCalledWith('ft8_set_band', expect.anything());
  });

  it('surfaces a rejected retune as the bands error', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') return new Promise(() => {});
      if (cmd === 'ft8_set_band') return Promise.reject({ kind: 'invalid-band', detail: 'not an FT8 band' });
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false, bands: ['20m'] }), heldBand: '20m' });
    fireEvent.click(screen.getByTestId('band-subset-chip-40m'));
    expect(await screen.findByTestId('band-subset-bands-error')).toHaveTextContent('not an FT8 band');
  });

  it('in sweep mode chips stay the multi-select subset (ft8_set_sweep_bands, never set_band)', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['20m'] }), heldBand: '20m' });
    fireEvent.click(screen.getByTestId('band-subset-chip-40m'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_sweep_bands', { bands: ['20m', '40m'] });
    expect(invokeMock).not.toHaveBeenCalledWith('ft8_set_band', expect.anything());
  });
});

describe('BandSubsetPopover — sweep-enable gated on a fresh ft8_cat_probe', () => {
  it('disables the Sweep radio with the default reason while no probe result exists (absent)', () => {
    // beforeEach's default invoke impl never resolves ft8_cat_probe.
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    expect(screen.getByTestId('band-subset-mode-sweep')).toBeDisabled();
    expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent(
      'sweep needs CAT to QSY between bands',
    );
  });

  it('enables the Sweep radio and shows the dwell caption once the probe succeeds', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') return Promise.resolve({ dialHz: 14074000, band: '20m' });
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false, dwellSlots: 4 }) });
    await waitFor(() => expect(screen.getByTestId('band-subset-mode-sweep')).not.toBeDisabled());
    expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent('4 slots/band');
  });

  it('disables the Sweep radio with the modem-busy reason interpolating the blocking mode', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') {
        return Promise.reject({ kind: 'modem-busy', detail: 'a modem session is active' });
      }
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }), blockingSessionMode: 'VARA' });
    await waitFor(() =>
      expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent(
        'radio busy with VARA session — disconnect first',
      ),
    );
    expect(screen.getByTestId('band-subset-mode-sweep')).toBeDisabled();
  });

  it('degrades the modem-busy reason to "another session" when the blocking mode is absent', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') {
        return Promise.reject({ kind: 'modem-busy', detail: 'a modem session is active' });
      }
      return Promise.resolve();
    });
    // No blockingSessionMode prop — the graceful fallback path.
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    await waitFor(() =>
      expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent(
        'radio busy with another session — disconnect first',
      ),
    );
    expect(screen.getByTestId('band-subset-mode-sweep')).toBeDisabled();
  });

  it('disables the Sweep radio with the rig-not-configured reason on that probe failure', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') {
        return Promise.reject({ kind: 'rig-not-configured', detail: 'no rig is configured' });
      }
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    await waitFor(() =>
      expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent(
        'no rig configured — set up CAT first',
      ),
    );
  });

  it('falls back to a generic reason for probe-timeout / unrecognized error kinds', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') {
        return Promise.reject({ kind: 'probe-timeout', detail: 'CAT probe did not complete within 3s' });
      }
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    await waitFor(() =>
      expect(screen.getByTestId('band-subset-sweep-caption')).toHaveTextContent(
        'radio not responding — check CAT',
      ),
    );
  });

  it('does not re-probe when sweepConfig.enabled is already true', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true }) });
    expect(invokeMock).not.toHaveBeenCalledWith('ft8_cat_probe', undefined);
  });

  it('invokes ft8_set_sweep(true) when the operator selects Sweep after a successful probe', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_cat_probe') return Promise.resolve({ dialHz: 14074000, band: '20m' });
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: false }) });
    await waitFor(() => expect(screen.getByTestId('band-subset-mode-sweep')).not.toBeDisabled());
    fireEvent.click(screen.getByTestId('band-subset-mode-sweep'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_sweep', { enabled: true });
  });

  it('invokes ft8_set_sweep(false) when the operator selects Hold-one while sweep is enabled', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true }) });
    fireEvent.click(screen.getByTestId('band-subset-mode-hold'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_sweep', { enabled: false });
  });
});

describe('BandSubsetPopover — persist-only caption', () => {
  it('shows the persist-only caption while not listening', () => {
    renderPopover({ isListening: false });
    expect(screen.getByTestId('band-subset-persist-caption')).toHaveTextContent(
      'saved — applies at next start (will tune your radio)',
    );
  });

  it('hides the persist-only caption while listening', () => {
    renderPopover({ isListening: true });
    expect(screen.queryByTestId('band-subset-persist-caption')).not.toBeInTheDocument();
  });
});

describe('BandSubsetPopover — fallback-hold inline warning', () => {
  it('shows the warning when sweep.mode is fallback-hold', () => {
    renderPopover({ fallbackHold: true, sweepConfig: sweepConfig({ enabled: true }) });
    expect(screen.getByTestId('band-subset-fallback-warning')).toHaveTextContent(
      'radio not responding',
    );
  });

  it('omits the warning otherwise', () => {
    renderPopover({ fallbackHold: false });
    expect(screen.queryByTestId('band-subset-fallback-warning')).not.toBeInTheDocument();
  });
});

describe('BandSubsetPopover — chip change invokes ft8_set_sweep_bands', () => {
  it('adds a band: invokes with the existing bands plus the clicked one', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['20m', '40m'] }) });
    fireEvent.click(screen.getByTestId('band-subset-chip-30m'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_sweep_bands', { bands: ['20m', '40m', '30m'] });
  });

  it('removes a band: invokes with it filtered out', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['20m', '40m'] }) });
    fireEvent.click(screen.getByTestId('band-subset-chip-40m'));
    expect(invokeMock).toHaveBeenCalledWith('ft8_set_sweep_bands', { bands: ['20m'] });
  });

  it('refuses to submit an empty subset (last band cannot be deselected)', () => {
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['20m'] }) });
    fireEvent.click(screen.getByTestId('band-subset-chip-20m'));
    expect(invokeMock).not.toHaveBeenCalledWith('ft8_set_sweep_bands', expect.anything());
  });

  it('surfaces a persist error inline when the command rejects', async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'ft8_set_sweep_bands') {
        return Promise.reject({ kind: 'internal-error', detail: 'disk full' });
      }
      return Promise.resolve();
    });
    renderPopover({ sweepConfig: sweepConfig({ enabled: true, bands: ['20m'] }) });
    fireEvent.click(screen.getByTestId('band-subset-chip-40m'));
    await waitFor(() =>
      expect(screen.getByTestId('band-subset-bands-error')).toHaveTextContent('disk full'),
    );
  });
});
