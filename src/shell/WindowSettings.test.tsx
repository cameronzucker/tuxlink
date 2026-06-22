// WindowSettings.test.tsx — tuxlink-5rvp / #882.
// Asserts: loads close_to_tray from config_read, hydrates the toggle, and
// toggling invokes set_close_to_tray with the new value. Mirrors
// MailboxSettings.test.tsx.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { WindowSettings } from './WindowSettings';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function mockConfigRead(closeToTray: boolean) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'config_read') {
      return {
        connect_to_cms: false,
        transport: 'Telnet',
        host: 'cms-z.winlink.org',
        callsign: null,
        identifier: 'W1TEST',
        grid: null,
        gps_state: 'Off',
        position_precision: 'FourCharGrid',
        position_source: 'Gps',
        review_inbound_before_download: false,
        trash_auto_purge: true,
        trash_retention_days: 30,
        close_to_tray: closeToTray,
      };
    }
    return undefined;
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  mockConfigRead(true);
});

describe('WindowSettings', () => {
  it('renders the close-to-tray toggle', async () => {
    render(<WindowSettings />);
    expect(await screen.findByTestId('window-settings')).toBeInTheDocument();
    expect(screen.getByTestId('close-to-tray-toggle')).toBeInTheDocument();
  });

  it('hydrates the toggle from the loaded config (true)', async () => {
    render(<WindowSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('close-to-tray-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
  });

  it('hydrates the toggle from the loaded config (false)', async () => {
    invokeMock.mockReset();
    mockConfigRead(false);
    render(<WindowSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('close-to-tray-toggle');
    await waitFor(() => expect(toggle.checked).toBe(false));
  });

  it('invokes set_close_to_tray when the toggle is changed', async () => {
    render(<WindowSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('close-to-tray-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('set_close_to_tray', { value: false });
    });
  });

  it('shows an error when config_read fails', async () => {
    invokeMock.mockReset();
    invokeMock.mockRejectedValue(new Error('backend error'));
    render(<WindowSettings />);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });
});
