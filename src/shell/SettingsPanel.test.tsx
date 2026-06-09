import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { SettingsPanel } from './SettingsPanel';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') {
      return {
        gps_state: 'BroadcastAtPrecision',
        position_precision: 'FourCharGrid',
        review_inbound_before_download: false,
      };
    }
    return undefined;
  });
});

describe('SettingsPanel', () => {
  it('renders nothing when closed', () => {
    const { container } = render(<SettingsPanel open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('loads current config and checks the matching radios', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    expect(broadcast).toBeChecked();
    expect(screen.getByRole('radio', { name: /4-char grid/i })).toBeChecked();
  });

  it('persists a gps_state change via config_set_privacy (keeps current precision)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    // tuxlink-61yg CI fix: the gpsState/precision change handlers short-
    // circuit when config isn't loaded yet (`gpsState && persist(...)`,
    // `precision && persist(...)`). Wait for the mock config_read to land
    // in state — signaled by the "Broadcast At Precision" radio becoming
    // checked — before firing the click. Without this wait the test
    // races on slow CI (passes locally where microtasks drain faster).
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    await waitFor(() => expect(broadcast).toBeChecked());
    const off = screen.getByRole('radio', { name: /^off/i });
    fireEvent.click(off);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_privacy', {
        gpsState: 'Off',
        positionPrecision: 'FourCharGrid',
      });
    });
  });

  it('persists a precision change via config_set_privacy (keeps current gps_state)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    // Same race as above — wait for config to load before firing the click.
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    await waitFor(() => expect(broadcast).toBeChecked());
    const six = screen.getByRole('radio', { name: /6-char grid/i });
    fireEvent.click(six);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_privacy', {
        gpsState: 'BroadcastAtPrecision',
        positionPrecision: 'SixCharGrid',
      });
    });
  });

  it('calls onClose on the close button and on Escape', async () => {
    const onClose = vi.fn();
    render(<SettingsPanel open onClose={onClose} />);
    await screen.findByTestId('settings-panel');
    fireEvent.click(screen.getByTestId('settings-close'));
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it('does NOT render the ARDOP HF fieldset (tuxlink-jmfm)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    // Wait for the panel to be open before asserting absence.
    await screen.findByTestId('settings-panel');
    expect(screen.queryByText(/ARDOP HF/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/ardopcf binary/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/Capture device/i)).not.toBeInTheDocument();
  });

});
