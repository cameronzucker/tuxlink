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
      };
    }
    if (cmd === 'config_get_ardop') {
      // Wire format is snake_case — ArdopUiConfig lacks #[serde(rename_all = "camelCase")].
      return {
        binary: 'ardopcf',
        capture_device: 'plughw:1,0',
        playback_device: 'plughw:1,0',
        ptt_serial_path: null,
        cmd_port: 8515,
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

});

describe('SettingsPanel ARDOP HF section', () => {
  it('renders the ARDOP HF section with binary/capture/playback/PTT/cmd-port fields', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    expect(await screen.findByLabelText(/ardopcf binary/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/capture device/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/playback device/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/ptt serial/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/cmd port/i)).toBeInTheDocument();
  });

  it('initial-loads ARDOP config via config_get_ardop', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const capture = await screen.findByLabelText(/capture device/i);
    await waitFor(() => {
      expect((capture as HTMLInputElement).value).toBe('plughw:1,0');
    });
    expect(invokeMock).toHaveBeenCalledWith('config_get_ardop');
  });

  it('persists via config_set_ardop on blur (snake_case wire fields)', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const capture = await screen.findByLabelText(/capture device/i);
    fireEvent.change(capture, { target: { value: 'plughw:2,0' } });
    fireEvent.blur(capture);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_ardop', expect.objectContaining({
        value: expect.objectContaining({ capture_device: 'plughw:2,0' }),
      }));
    });
  });

  it('converts blank PTT serial input to null on persist', async () => {
    render(<SettingsPanel open onClose={vi.fn()} />);
    const ptt = await screen.findByLabelText(/ptt serial/i);
    fireEvent.change(ptt, { target: { value: '' } });
    fireEvent.blur(ptt);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('config_set_ardop', expect.objectContaining({
        value: expect.objectContaining({ ptt_serial_path: null }),
      }));
    });
  });
});
