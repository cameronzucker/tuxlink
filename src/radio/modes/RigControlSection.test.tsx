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
  data_mode: 'PKTUSB',
  rig_field_overrides: [],
};

describe('<RigControlSection>', () => {
  beforeEach(async () => {
    localStorage.clear();
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [
        { path: '/dev/ttyUSB0', kind: 'usb', label: 'FT-710 CAT' },
      ];
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

  it('renders the CAT port select with the loaded value', async () => {
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      const sel = screen.getByTestId('rig-cat-port') as HTMLSelectElement;
      expect(sel.value).toBe('/dev/ttyUSB0');
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

  it('persists CAT port select change via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-cat-port')).toBeInTheDocument();
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-port'), { target: { value: '' } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ cat_serial_path: null }),
        }),
      );
    });
  });

  it('persists CAT port manual input on blur via config_set_rig', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-cat-port-manual')).toBeInTheDocument();
    });
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-cat-port-manual'), { target: { value: '/dev/ttyUSB1' } });
    fireEvent.blur(screen.getByTestId('rig-cat-port-manual'));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({ cat_serial_path: '/dev/ttyUSB1' }),
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
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
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
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      const vfo = screen.getByTestId('rig-live-vfo') as HTMLInputElement;
      expect(vfo.disabled).toBe(true);
    });
  });

  // ── New tests for Task 4 redesign ──────────────────────────────────────────

  it('renders models from rig_list_models, grouped by manufacturer', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_hamlib_model: null };
      if (cmd === 'rig_list_models') return [
        { id: 1049, manufacturer: 'Yaesu', model: 'FT-710' },
        { id: 3073, manufacturer: 'Icom', model: 'IC-7300' },
      ];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-model')).toBeInTheDocument();
    });
    // both manufacturers' models are options
    expect(screen.getByRole('option', { name: /FT-710/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /IC-7300/ })).toBeInTheDocument();
  });

  it('renders detected serial ports in the CAT-port picker', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [
        { path: '/dev/ttyUSB0', kind: 'usb', label: 'CP2102 USB-UART' },
      ];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect(screen.getByTestId('rig-cat-port')).toBeInTheDocument();
    });
    expect(screen.getByRole('option', { name: /\/dev\/ttyUSB0/ })).toBeInTheDocument();
  });

  it('renders a Mode row bound to data_mode', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, data_mode: 'USB-D' };
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => {
      expect((screen.getByTestId('rig-data-mode') as HTMLSelectElement).value).toBe('USB-D');
    });
  });

  it('no longer renders the QSY-on-fail control or the CAT backend label', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model-manual')).toBeInTheDocument());
    expect(screen.queryByTestId('rig-qsy-on-fail')).not.toBeInTheDocument();
    expect(screen.queryByText('CAT backend')).not.toBeInTheDocument();
  });

  // ── Task 5: override-respecting per-radio pre-fill ─────────────────────────

  it('pre-fills non-overridden rig fields when a radio is selected', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_hamlib_model: null, rig_field_overrides: [] };
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'config_set_rig',
        expect.objectContaining({
          value: expect.objectContaining({
            rig_hamlib_model: 1049,
            data_mode: 'PKTUSB',
            cat_baud: 38400,
            close_serial_sequencing: true,
          }),
        }),
      );
    });
  });

  it('editing a field marks it overridden and a later radio change leaves it untouched', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    // Start with cat_baud already overridden + a non-default value.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return {
        ...knownConfig, rig_hamlib_model: null, cat_baud: 9600, rig_field_overrides: ['cat_baud'],
      };
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });
    await waitFor(() => {
      // cat_baud is overridden → NOT clobbered by the FT-710 profile's 38400.
      const call = invokeMock.mock.calls.find((c) => c[0] === 'config_set_rig');
      expect(call?.[1].value.cat_baud).toBe(9600);
      // but data_mode (not overridden) IS pre-filled.
      expect(call?.[1].value.data_mode).toBe('PKTUSB');
    });
  });

  it('records an override key when the operator edits the Mode', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return { ...knownConfig, rig_field_overrides: [] };
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" />);
    await waitFor(() => expect(screen.getByTestId('rig-data-mode')).toBeInTheDocument());
    invokeMock.mockClear();
    fireEvent.change(screen.getByTestId('rig-data-mode'), { target: { value: 'USB-D' } });
    await waitFor(() => {
      const call = invokeMock.mock.calls.find((c) => c[0] === 'config_set_rig');
      expect(call?.[1].value.rig_field_overrides).toContain('data_mode');
      expect(call?.[1].value.data_mode).toBe('USB-D');
    });
  });

  // ── Task 6: variant="bare" render mode ────────────────────────────────────

  it('variant="bare" renders rows without the expander chrome', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="ardop" variant="bare" />);
    await waitFor(() => expect(screen.getByTestId('rig-model-manual')).toBeInTheDocument());
    expect(screen.queryByTestId('rig-control-expander')).not.toBeInTheDocument();
  });

  it('default variant still renders the expander', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') return knownConfig;
      if (cmd === 'rig_list_models') return [];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });
    render(<RigControlSection storageKeyPrefix="vara" />);
    await waitFor(() => expect(screen.getByTestId('rig-control-expander')).toBeInTheDocument());
  });

  // ── Task 7 fix: ptt_method override must survive a RigControlSection write ──

  it('regression: onModelSelected preserves ptt_method override written by ArdopRadioPanel', async () => {
    // Scenario: ARDOP panel's onPttMethodChange has already called config_set_rig
    // with rig_field_overrides: ['ptt_method']. The local RigControlSection state
    // (rigConfig) is stale and still has rig_field_overrides: []. When the operator
    // then changes the radio model, the stale local copy must NOT be used for the
    // write — the fresh backend value must be read first, preserving 'ptt_method'.
    const core = await import('@tauri-apps/api/core');
    const invokeMock = core.invoke as ReturnType<typeof vi.fn>;

    // Backend returns stale local state (no ptt_method override) on initial mount,
    // then returns the updated state (ptt_method already overridden by ARDOP panel)
    // on the read-modify-write's config_get_rig call.
    let getRigCallCount = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_get_rig') {
        getRigCallCount++;
        if (getRigCallCount === 1) {
          // Initial mount load: no overrides yet
          return { ...knownConfig, rig_hamlib_model: null, rig_field_overrides: [] };
        }
        // Subsequent reads (during rmwRig): ARDOP panel has since written ptt_method
        return { ...knownConfig, rig_hamlib_model: null, rig_field_overrides: ['ptt_method'] };
      }
      if (cmd === 'rig_list_models') return [{ id: 1049, manufacturer: 'Yaesu', model: 'FT-710' }];
      if (cmd === 'packet_list_serial_devices') return [];
      return undefined;
    });

    const onRadioSelected = vi.fn();
    render(<RigControlSection storageKeyPrefix="ardop" onRadioSelected={onRadioSelected} />);
    await waitFor(() => expect(screen.getByTestId('rig-model')).toBeInTheDocument());

    // Clear calls after mount so we only examine the model-change write
    invokeMock.mockClear();
    getRigCallCount = 1; // reset so next config_get_rig (in rmwRig) returns the updated overrides

    fireEvent.change(screen.getByTestId('rig-model'), { target: { value: '1049' } });

    await waitFor(() => {
      // The config_set_rig call must include 'ptt_method' in rig_field_overrides
      const setCall = invokeMock.mock.calls.find((c) => c[0] === 'config_set_rig');
      expect(setCall).toBeTruthy();
      expect(setCall![1].value.rig_field_overrides).toContain('ptt_method');
    });

    // onRadioSelected must be called with pttOverridden=true (fresh read saw the override)
    await waitFor(() => {
      expect(onRadioSelected).toHaveBeenCalledWith(1049, true);
    });
  });
});
