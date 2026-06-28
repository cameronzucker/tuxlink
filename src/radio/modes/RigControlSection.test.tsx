// src/radio/modes/RigControlSection.test.tsx
//
// Tests for the shared RigControlSection component. Mocks config_get_rig to
// return a known RigConfig and asserts load + render + persist-on-change
// behavior. Mirrors the invoke-mock pattern from ArdopRadioPanel.test.tsx.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { RigControlSection } from './RigControlSection';
import type { RigConfig } from './RigControlSection';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const knownConfig: RigConfig = {
  rig_hamlib_model: 1049,
  rigctld_host: '127.0.0.1',
  rigctld_port: 4534,
  rigctld_binary: 'rigctld',
  close_serial_sequencing: false,
  live_vfo_poll: false,
  qsy_on_fail: false,
  cat_serial_path: '/dev/ttyUSB0',
  cat_baud: 38400,
};

describe('<RigControlSection>', () => {
  beforeEach(async () => {
    localStorage.clear();
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      return undefined;
    });
  });

  it('renders the Rig control expander', () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    expect(screen.getByTestId('rig-control-expander')).toBeInTheDocument();
    expect(screen.getByTestId('rig-control-expander-summary')).toHaveTextContent('Rig control');
  });

  it('is collapsed by default (no localStorage entry)', () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    const details = screen.getByTestId('rig-control-expander') as HTMLDetailsElement;
    expect(details.open).toBe(false);
  });

  it('restores expanded state from localStorage', () => {
    localStorage.setItem('tuxlink.ardop.rigCfgOpen', '1');
    render(<RigControlSection storageKeyPrefix="ardop" />);
    const details = screen.getByTestId('rig-control-expander') as HTMLDetailsElement;
    expect(details.open).toBe(true);
  });

  it('uses a separate localStorage key per storageKeyPrefix', () => {
    localStorage.setItem('tuxlink.vara.rigCfgOpen', '1');
    render(<RigControlSection storageKeyPrefix="vara" />);
    const details = screen.getByTestId('rig-control-expander') as HTMLDetailsElement;
    expect(details.open).toBe(true);
    // A fresh ardop instance (no key) stays collapsed.
    localStorage.removeItem('tuxlink.ardop.rigCfgOpen');
    const { unmount } = render(<RigControlSection storageKeyPrefix="ardop" />);
    const detailsEls = document.querySelectorAll('[data-testid="rig-control-expander"]');
    // The last rendered one is the ardop instance.
    const last = detailsEls[detailsEls.length - 1] as HTMLDetailsElement;
    expect(last.open).toBe(false);
    unmount();
  });

  it('calls config_get_rig on mount', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    const core = await import('@tauri-apps/api/core');
    await waitFor(() => {
      expect(core.invoke).toHaveBeenCalledWith('config_get_rig');
    });
  });

  it('renders the rig model select with the loaded value', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    const sel = (await screen.findByTestId('rig-model')) as HTMLSelectElement;
    await waitFor(() => {
      expect(sel.value).toBe('1049');
    });
  });

  it('renders the CAT port input with the loaded value', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      const input = screen.getByTestId('rig-cat-port') as HTMLInputElement;
      expect(input.value).toBe('/dev/ttyUSB0');
    });
  });

  it('renders the CAT baud input with the loaded value', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      const input = screen.getByTestId('rig-cat-baud') as HTMLInputElement;
      expect(input.value).toBe('38400');
    });
  });

  it('renders the close-serial checkbox', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-close-serial')).toBeInTheDocument();
    });
  });

  it('renders the live VFO checkbox', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-live-vfo')).toBeInTheDocument();
    });
  });

  // tuxlink-qevsf (SAFETY/Part 97): the QSY-on-fail checkbox was removed because
  // auto-QSY transmitted on candidate frequencies the operator never saw or
  // selected. The control must no longer render (the connect commands clamp the
  // candidate list to the operator-chosen channel, so it would be inert anyway).
  it('does not render the QSY on fail checkbox (tuxlink-qevsf)', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    // Wait for a control that IS rendered, then assert the QSY checkbox is absent.
    await waitFor(() => {
      expect(screen.getByTestId('rig-live-vfo')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('rig-qsy-on-fail')).not.toBeInTheDocument();
  });

  it('persists rig model change via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-model') as HTMLSelectElement).value).toBe('1049');
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '' } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ rig_hamlib_model: null }),
        }),
      );
    });
  });

  it('persists CAT port on blur via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-cat-port') as HTMLInputElement).value).toBe('/dev/ttyUSB0');
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-port'), { target: { value: '/dev/ttyUSB1' } });
    fireEvent.blur(screen.getByTestId('rig-cat-port'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ cat_serial_path: '/dev/ttyUSB1' }),
        }),
      );
    });
  });

  it('empty CAT port persists as null', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-cat-port') as HTMLInputElement).value).toBe('/dev/ttyUSB0');
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-port'), { target: { value: '' } });
    fireEvent.blur(screen.getByTestId('rig-cat-port'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ cat_serial_path: null }),
        }),
      );
    });
  });

  it('persists CAT baud on blur via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-cat-baud') as HTMLInputElement).value).toBe('38400');
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-baud'), { target: { value: '9600' } });
    fireEvent.blur(screen.getByTestId('rig-cat-baud'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ cat_baud: 9600 }),
        }),
      );
    });
  });

  it('reverts CAT baud input on invalid value (non-positive)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-cat-baud') as HTMLInputElement).value).toBe('38400');
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-baud'), { target: { value: 'abc' } });
    fireEvent.blur(screen.getByTestId('rig-cat-baud'));
    await waitFor(() => {
      expect((screen.getByTestId('rig-cat-baud') as HTMLInputElement).value).toBe('38400');
    });
    // config_set_rig should NOT be called for invalid input.
    const setCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === 'config_set_rig');
    expect(setCalls).toHaveLength(0);
  });

  it('persists close-serial-sequencing toggle via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-close-serial')).toBeInTheDocument();
    });
    invokeMock.mockClear();
    fireEvent.click(screen.getByTestId('rig-close-serial'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ close_serial_sequencing: true }),
        }),
      );
    });
  });

  it('enabling close-serial-sequencing also forces live_vfo_poll to false', async () => {
    // Seed config with live_vfo_poll=true.
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, live_vfo_poll: true };
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-live-vfo') as HTMLInputElement).checked).toBe(true);
    });
    invokeMock.mockClear();
    fireEvent.click(screen.getByTestId('rig-close-serial'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({
            close_serial_sequencing: true,
            live_vfo_poll: false,
          }),
        }),
      );
    });
  });

  it('live-VFO poll checkbox is disabled when close-serial-sequencing is on', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, close_serial_sequencing: true };
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      const vfo = screen.getByTestId('rig-live-vfo') as HTMLInputElement;
      expect(vfo.disabled).toBe(true);
    });
  });

  // tuxlink-qevsf (SAFETY/Part 97): the "persists QSY-on-fail toggle" test was
  // removed alongside the checkbox — there is no longer a control to toggle. The
  // `qsy_on_fail` field stays in the RigConfig DTO (config-schema stability) but
  // is not user-settable from this section.
});
