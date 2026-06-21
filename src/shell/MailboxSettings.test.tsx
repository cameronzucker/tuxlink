// MailboxSettings.test.tsx — TDD: written before MailboxSettings.tsx exists.
// Asserts: loads config_read values, toggling auto-purge invokes the setter,
// changing the days input invokes the setter, days input is disabled when
// auto-purge is off.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { invoke } from '@tauri-apps/api/core';
import { MailboxSettings } from './MailboxSettings';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function mockConfigRead(trashAutoPurge: boolean, trashRetentionDays: number) {
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
        trash_auto_purge: trashAutoPurge,
        trash_retention_days: trashRetentionDays,
      };
    }
    return undefined;
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  mockConfigRead(true, 30);
});

describe('MailboxSettings', () => {
  it('renders the auto-purge toggle and retention days input', async () => {
    render(<MailboxSettings />);
    expect(await screen.findByTestId('mailbox-settings')).toBeInTheDocument();
    expect(screen.getByTestId('auto-purge-toggle')).toBeInTheDocument();
    expect(screen.getByTestId('retention-days-input')).toBeInTheDocument();
  });

  it('hydrates the toggle and days input from the loaded config', async () => {
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput.value).toBe('30'));
  });

  it('hydrates with auto-purge=false and 14 days', async () => {
    invokeMock.mockReset();
    mockConfigRead(false, 14);
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(false));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput.value).toBe('14'));
  });

  it('disables the days input when auto-purge is off', async () => {
    invokeMock.mockReset();
    mockConfigRead(false, 30);
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(false));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput).toBeDisabled());
  });

  it('enables the days input when auto-purge is on', async () => {
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput).not.toBeDisabled());
  });

  it('invokes config_set_trash_auto_purge when the toggle is changed', async () => {
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    fireEvent.click(toggle);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_trash_auto_purge', {
        enabled: false,
        retentionDays: 30,
      });
    });
  });

  it('invokes config_set_trash_auto_purge when the days input is changed', async () => {
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput).not.toBeDisabled());
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    fireEvent.change(daysInput, { target: { value: '14' } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_trash_auto_purge', {
        enabled: true,
        retentionDays: 14,
      });
    });
  });

  it('does NOT persist when the days input is cleared (Number(\'\')===0 guard, review I1)', async () => {
    render(<MailboxSettings />);
    const toggle = await screen.findByTestId<HTMLInputElement>('auto-purge-toggle');
    await waitFor(() => expect(toggle.checked).toBe(true));
    const daysInput = screen.getByTestId<HTMLInputElement>('retention-days-input');
    await waitFor(() => expect(daysInput).not.toBeDisabled());
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
    // Clearing the field yields Number('') === 0 — out of range; must be ignored,
    // not persisted as retentionDays: 0 (which would desync local state).
    fireEvent.change(daysInput, { target: { value: '' } });
    fireEvent.change(daysInput, { target: { value: '0' } });
    fireEvent.change(daysInput, { target: { value: '400' } });
    await new Promise((r) => setTimeout(r, 50));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('shows an error when config_read fails', async () => {
    invokeMock.mockReset();
    invokeMock.mockRejectedValue(new Error('backend error'));
    render(<MailboxSettings />);
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });
});
