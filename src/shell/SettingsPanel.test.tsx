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
        // tuxlink-3o0: the CMS Server fieldset loads host + transport from config_read.
        host: 'cms-z.winlink.org',
        transport: 'Telnet',
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
    const off = await screen.findByRole('radio', { name: /^off/i });
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
    const six = await screen.findByRole('radio', { name: /6-char grid/i });
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

  // tuxlink-3o0 — CMS Server fieldset
  it('renders the CMS Server fieldset and loads the configured host + transport', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const hostInput = (await screen.findByTestId('conn-host')) as HTMLInputElement;
    expect(hostInput.value).toBe('cms-z.winlink.org');
    // Transport radios mirror the GPS radios; the configured Telnet (Plaintext) is
    // checked. Match the unique label prefix (the Plaintext option's help text
    // mentions "TLS", so a bare /tls/ would match both radios).
    expect(screen.getByRole('radio', { name: /Plaintext · 8772/ })).toBeChecked();
    expect(screen.getByRole('radio', { name: /TLS · 8773/ })).not.toBeChecked();
  });

  it('fills the host input from a quick-pick button', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const hostInput = (await screen.findByTestId('conn-host')) as HTMLInputElement;
    // The production quick-pick fills the input with server.winlink.org.
    fireEvent.click(screen.getByRole('button', { name: /server\.winlink\.org \(production\)/i }));
    expect(hostInput.value).toBe('server.winlink.org');
  });

  it('persists a transport change via config_set_connect (keeps the current host)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const tls = await screen.findByRole('radio', { name: /TLS · 8773/ });
    fireEvent.click(tls);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_connect', {
        host: 'cms-z.winlink.org',
        transport: 'CmsSsl',
      });
    });
  });

  it('persists a host change (on blur) via config_set_connect (keeps the current transport)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const hostInput = (await screen.findByTestId('conn-host')) as HTMLInputElement;
    fireEvent.change(hostInput, { target: { value: 'server.winlink.org' } });
    fireEvent.blur(hostInput);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_connect', {
        host: 'server.winlink.org',
        transport: 'Telnet',
      });
    });
  });
});
