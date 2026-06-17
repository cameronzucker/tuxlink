import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Section panes own their own data/providers and have their own tests; stub them
// here so the SettingsPanel shell test asserts nav + pane wiring only.
vi.mock('./IdentitiesSettings', () => ({
  IdentitiesSettings: () => <div data-testid="identities-settings" />,
}));
vi.mock('../location/LocationSettingsPane', () => ({
  LocationSettingsPane: () => <div data-testid="location-settings" />,
}));
vi.mock('./WinlinkAccountSettings', () => ({
  WinlinkAccountSettings: () => <div data-testid="winlink-account-settings" />,
}));
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

/** Open the panel and switch to the GPS state & privacy section. */
function openGpsState() {
  render(<SettingsPanel open onClose={vi.fn()} />);
  fireEvent.click(screen.getByTestId('settings-nav-gpsstate'));
}

describe('SettingsPanel', () => {
  it('renders nothing when closed', () => {
    const { container } = render(<SettingsPanel open={false} onClose={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('defaults to the Location & GPS section (inline pane, no popup)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    expect(await screen.findByTestId('settings-pane-location')).toBeInTheDocument();
    expect(screen.getByTestId('location-settings')).toBeInTheDocument();
    // No "Open …" button — the feature is inline, not a nested window.
    expect(screen.queryByTestId('open-location-settings')).toBeNull();
  });

  it('navigates between sections in place (nav + inline pane)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('settings-nav-identities'));
    expect(await screen.findByTestId('settings-pane-identities')).toBeInTheDocument();
    expect(screen.getByTestId('identities-settings')).toBeInTheDocument();
  });

  // tuxlink-vfb3: the Winlink Account section hosts CMS password change + the
  // keyring-only re-enter recovery.
  it('renders the Winlink Account section (nav + inline pane)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId('settings-nav-account'));
    expect(await screen.findByTestId('settings-pane-account')).toBeInTheDocument();
    expect(screen.getByTestId('winlink-account-settings')).toBeInTheDocument();
  });

  // tuxlink-vfb3: opening directly on the account section (the menu entry point).
  it('honors initialSection=account', async () => {
    render(<SettingsPanel open onClose={vi.fn()} initialSection="account" />);
    expect(await screen.findByTestId('settings-pane-account')).toBeInTheDocument();
    expect(screen.getByTestId('winlink-account-settings')).toBeInTheDocument();
  });

  it('loads current config and checks the matching radios (GPS state section)', async () => {
    openGpsState();
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    await waitFor(() => expect(broadcast).toBeChecked());
    expect(screen.getByRole('radio', { name: /4-char grid/i })).toBeChecked();
  });

  it('persists a gps_state change via config_set_privacy (keeps current precision)', async () => {
    openGpsState();
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    await waitFor(() => expect(broadcast).toBeChecked());
    fireEvent.click(screen.getByRole('radio', { name: /^off/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_privacy', {
        gpsState: 'Off',
        positionPrecision: 'FourCharGrid',
      });
    });
  });

  it('persists a precision change via config_set_privacy (keeps current gps_state)', async () => {
    openGpsState();
    const broadcast = await screen.findByRole('radio', { name: /broadcast at precision/i });
    await waitFor(() => expect(broadcast).toBeChecked());
    fireEvent.click(screen.getByRole('radio', { name: /6-char grid/i }));
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
    await screen.findByTestId('settings-panel');
    expect(screen.queryByText(/ARDOP HF/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/ardopcf binary/i)).not.toBeInTheDocument();
  });
});
