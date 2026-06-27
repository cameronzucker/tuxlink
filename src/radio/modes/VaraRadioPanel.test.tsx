// src/radio/modes/VaraRadioPanel.test.tsx
//
// Behavioral tests for the Phase 2 VARA panel. Mocks `@tauri-apps/api/core`
// so the panel can render + transition without a Tauri runtime. The mock
// returns command-specific defaults; individual tests override via
// `mockImplementation` for failure-path coverage.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import type { ReactElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { VaraRadioPanel } from './VaraRadioPanel';
import type { RadioPanelMode } from '../types';
import { emitGatewayPrefill } from '../../favorites/prefillEvent';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

// The VARA pane now mounts FavoritesTabs/useFavorites (react-query, B6 mirror
// of ARDOP/Packet), so every render must be wrapped in a QueryClientProvider or
// the queries throw "No QueryClient set". retry:false keeps a rejected favorites
// read from retry-looping in jsdom.
const renderPanel = (ui: ReactElement) => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
};

// Mirror of ARDOP's test helper — clicks the Manual tab trigger by role.
const switchToManualTab = async () => {
  const manual = await screen.findByRole('tab', { name: 'Manual' });
  fireEvent.mouseDown(manual, { button: 0 });
};

const HF_MODE: RadioPanelMode = { kind: 'vara-hf', intent: 'cms' };
const FM_MODE: RadioPanelMode = { kind: 'vara-fm', intent: 'cms' };

const defaultConfig = {
  host: '127.0.0.1',
  cmd_port: 8300,
  data_port: 8301,
  bandwidth_hz: null as number | null,
};

const closedStatus = {
  state: 'closed',
  lastError: null,
  boundHost: null,
  boundCmdPort: null,
};

const openStatus = {
  state: 'open',
  lastError: null,
  boundHost: '127.0.0.1',
  boundCmdPort: 8300,
};

const x86Platform = { arch: 'x86_64', os: 'linux', varaSupported: true };
const armPlatform = { arch: 'aarch64', os: 'linux', varaSupported: false };

function makeInvoke(overrides: Record<string, unknown> = {}) {
  return async (cmd: string, _args?: unknown) => {
    if (cmd in overrides) {
      const v = overrides[cmd];
      if (v instanceof Error) throw v;
      return v;
    }
    if (cmd === 'config_get_vara') return defaultConfig;
    // tuxlink-8fkkk Task A1UI: RigControlSection (rendered in VaraRadioPanel)
    // calls config_get_rig on mount — return the Rust defaults.
    if (cmd === 'config_get_rig') {
      return {
        rig_hamlib_model: null,
        rigctld_host: '127.0.0.1',
        rigctld_port: 4534,
        rigctld_binary: 'rigctld',
        close_serial_sequencing: false,
        live_vfo_poll: false,
        qsy_on_fail: false,
        cat_serial_path: null,
        cat_baud: 38400,
      };
    }
    if (cmd === 'vara_status') return closedStatus;
    // vara_open_session defaults to a successful open (the real command always
    // returns a VaraStatusDto). Tests override it for failure-path coverage.
    if (cmd === 'vara_open_session') return openStatus;
    if (cmd === 'platform_info') return x86Platform;
    if (cmd === 'session_log_snapshot') return [];
    // Favorites surface (B6 VARA mirror). The mounted FavoritesTabs/useFavorites
    // issue these reads; return empty/benign shapes so the queries RESOLVE
    // (rejecting noisily fails in jsdom). Tests needing a clickable favorite
    // override favorites_read / favorites_recents per-test.
    if (cmd === 'favorites_read') return { schema_version: 1, favorites: [], log: [] };
    if (cmd === 'favorites_recents') return [];
    if (cmd === 'position_current_fix') return { grid: null };
    if (cmd === 'favorite_tod_hint') return null;
    return undefined;
  };
}

describe('<VaraRadioPanel>', () => {
  beforeEach(async () => {
    // tuxlink-ypz3 (3a): the panel restores its target from
    // localStorage['tuxlink.lastTarget.vara-hf' | '...vara-fm'] on mount; prefill
    // tests write that key — clear it so a persisted target can't leak across
    // tests.
    localStorage.clear();
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(makeInvoke());
  });

  it('renders the VARA HF panel title for vara-hf mode', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('VARA HF');
  });

  it('renders the VARA FM panel title for vara-fm mode', async () => {
    renderPanel(<VaraRadioPanel mode={FM_MODE} onClose={() => {}} />);
    expect(screen.getByTestId('radio-panel-title')).toHaveTextContent('VARA FM');
  });

  it('hydrates host + ports from config_get_vara', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({
        config_get_vara: { host: '10.0.0.5', cmd_port: 8400, data_port: 8401, bandwidth_hz: 2300 },
      }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('10.0.0.5');
    });
    expect((screen.getByTestId('vara-cmd-port-input') as HTMLInputElement).value).toBe('8400');
    expect((screen.getByTestId('vara-data-port-input') as HTMLInputElement).value).toBe('8401');
  });

  it('uses defaults when config_get_vara rejects (pre-wizard)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ config_get_vara: new Error('NotConfigured') }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('127.0.0.1');
    });
    expect((screen.getByTestId('vara-cmd-port-input') as HTMLInputElement).value).toBe('8300');
  });

  it('hydrates status from vara_status', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: open');
    });
  });

  it('disables Start when transport is already open', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).toBeDisabled();
    });
  });

  it('disables Stop when transport is closed', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-stop-btn')).toBeDisabled();
    });
  });

  it('invokes vara_open_session on Start click and updates status', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({ vara_open_session: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: open');
    });
    expect(invokeSpy).toHaveBeenCalledWith('vara_open_session', {
      intent: 'cms',
      transportKind: 'vara-hf',
    });
  });

  it('passes transportKind: vara-fm when mounted in vara-fm mode (Codex Round 3 P1 #3)', async () => {
    // Regression: the panel must pass mode.kind through as transportKind so
    // the backend records the operator-meaningful vara-hf vs vara-fm
    // discriminator on session state. Without this, the two modes share an
    // open session and the frontend can't detect sidebar-nav drift mid-session.
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({ vara_open_session: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={FM_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith('vara_open_session', {
        intent: 'cms',
        transportKind: 'vara-fm',
      });
    });
  });

  // tuxlink-o0c8: the panel must thread the sidebar-selected intent
  // (cms / p2p / radio-only) — NOT hardcode 'cms' — into both backend calls, so
  // p2p and radio-only VARA sessions route correctly. Mirrors ARDOP (tuxlink-nnws).
  it('threads the selected intent (radio-only) into vara_open_session', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(makeInvoke({ vara_open_session: openStatus }));
    renderPanel(
      <VaraRadioPanel mode={{ kind: 'vara-hf', intent: 'radio-only' }} onClose={() => {}} />,
    );
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith('vara_open_session', {
        intent: 'radio-only',
        transportKind: 'vara-hf',
      });
    });
  });

  it('threads the selected intent (p2p) into modem_vara_b2f_exchange', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    // Hydrate an open session so Send/Receive is reachable.
    invokeSpy.mockImplementation(makeInvoke({ vara_status: openStatus }));
    renderPanel(
      <VaraRadioPanel mode={{ kind: 'vara-hf', intent: 'p2p' }} onClose={() => {}} />,
    );
    await switchToManualTab();
    const input = await screen.findByTestId('vara-target-input');
    fireEvent.change(input, { target: { value: 'W7RMS-10' } });
    await waitFor(() => {
      expect(screen.getByTestId('vara-send-receive-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-send-receive-btn'));
    });

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith(
        'modem_vara_b2f_exchange',
        expect.objectContaining({
          target: 'W7RMS-10',
          intent: 'p2p',
          transportKind: 'vara-hf',
        }),
      );
    });
  });

  // tuxlink-8c9f: the dial-target field must name what the operator types per
  // intent — a PEER callsign in P2P, an RMS gateway otherwise — so P2P mode
  // doesn't mislabel the field "RMS gateway call sign".
  it('labels the dial target "peer station call sign" in P2P mode', async () => {
    renderPanel(
      <VaraRadioPanel mode={{ kind: 'vara-hf', intent: 'p2p' }} onClose={() => {}} />,
    );
    await switchToManualTab();
    const input = await screen.findByTestId('vara-target-input');
    expect(input.getAttribute('placeholder')).toBe('peer station call sign');
  });

  it('labels the dial target "RMS gateway call sign" for cms intent', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await switchToManualTab();
    const input = await screen.findByTestId('vara-target-input');
    expect(input.getAttribute('placeholder')).toBe('RMS gateway call sign');
  });

  it('surfaces start-failure error inline', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_open_session: new Error('TCP connect failed: Connection refused (os error 111)') }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-action-error')).toHaveTextContent('Start failed');
    });
  });

  it('invokes vara_close_session on Stop click', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({
        vara_status: openStatus,
        vara_close_session: closedStatus,
      }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-stop-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-stop-btn'));
    });

    await waitFor(() => {
      expect(screen.getByTestId('vara-state-display')).toHaveTextContent('State: closed');
    });
    // Task 3.3: backend renamed vara_stop_session → vara_close_session;
    // the Stop button still says "Stop" in the UI (operator-facing label),
    // but the underlying command is the renamed close-session lifecycle
    // (disarm + abort + clear active mode + transport teardown).
    expect(invokeSpy).toHaveBeenCalledWith('vara_close_session');
  });

  it('renders the Pi-availability banner on ARM but keeps controls editable (tuxlink-ze98)', async () => {
    // Pre-tuxlink-ze98 the panel disabled all controls when platformBlocked
    // — wrong, because tuxlink CAN connect to a REMOTE VARA over TCP from a
    // Pi (the modem just can't run LOCALLY on aarch64 due to no Wine).
    // Post-fix: banner is informational, controls stay editable, Start
    // remains clickable so the operator can point at a remote VARA host.
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ platform_info: armPlatform }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-platform-banner')).toBeInTheDocument();
    });
    // Form fields must NOT be disabled by platform-block alone — the
    // operator needs to edit the host to point at a remote VARA.
    expect(screen.getByTestId('vara-host-input')).not.toBeDisabled();
    expect(screen.getByTestId('vara-cmd-port-input')).not.toBeDisabled();
    expect(screen.getByTestId('vara-data-port-input')).not.toBeDisabled();
    expect(screen.getByTestId('vara-bandwidth-select')).not.toBeDisabled();
    // Start must remain clickable — TCP-connect to a remote host is the
    // supported path for Pi operators. (If it fails because nothing is
    // listening, the lastError will surface that honestly.)
    expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
  });

  it('actually invokes vara_open_session on Start click under armPlatform (tuxlink-poh6)', async () => {
    // Regression test for tuxlink-poh6: the previous fix (tuxlink-ze98)
    // removed platformBlocked from the disabled prop but LEFT it in the
    // onStartClick early-return guard. Button was clickable, handler
    // refused. The fix removes the guard from the handler too.
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(
      makeInvoke({
        platform_info: armPlatform,
        vara_open_session: openStatus,
      }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId('vara-start-btn'));
    });

    // The handler must have fired vara_open_session, NOT silently no-op'd.
    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith('vara_open_session', {
        intent: 'cms',
        transportKind: 'vara-hf',
      });
    });
  });

  it('does not render the banner on x86_64', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => {
      // Wait for at least one hydration so platform_info has been awaited.
      expect((screen.getByTestId('vara-host-input') as HTMLInputElement).value).toBe('127.0.0.1');
    });
    expect(screen.queryByTestId('vara-platform-banner')).toBeNull();
  });

  it('rejects an out-of-range cmd_port and reverts the input', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const input = screen.getByTestId('vara-cmd-port-input') as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe('8300'));

    await act(async () => {
      fireEvent.change(input, { target: { value: '99999' } });
      fireEvent.blur(input);
    });

    await waitFor(() => {
      expect(input.value).toBe('8300'); // reverted
    });
    expect(screen.getByTestId('vara-action-error')).toHaveTextContent('Invalid cmd port');
  });

  it('renders the bandwidth options and reflects null as Auto', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const select = await waitFor(() => screen.getByTestId('vara-bandwidth-select') as HTMLSelectElement);
    expect(select.value).toBe(''); // null bandwidth = "" (Auto)
    expect(screen.getByText(/2300 Hz \(HF Standard\)/)).toBeInTheDocument();
    expect(screen.getByText(/2750 Hz \(HF Tactical\)/)).toBeInTheDocument();
  });

  it('persists bandwidth change via setConfig → config_set_vara', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const select = await waitFor(() => screen.getByTestId('vara-bandwidth-select') as HTMLSelectElement);

    await act(async () => {
      fireEvent.change(select, { target: { value: '2750' } });
    });

    await waitFor(() => {
      const setCalls = invokeSpy.mock.calls.filter((c) => c[0] === 'config_set_vara');
      expect(setCalls.length).toBeGreaterThanOrEqual(1);
      expect(setCalls[setCalls.length - 1][1]).toEqual({
        value: { host: '127.0.0.1', cmd_port: 8300, data_port: 8301, bandwidth_hz: 2750 },
      });
    });
  });

  // tuxlink-tccc: the Listen-section arm button must NOT show the in-flight
  // "Arming…" label when the only reason it's disabled is the transport-not-
  // Open precondition. The PR #348 shipping version conflated the two by
  // passing busy={varaListener.busy || !isOpen}; ListenArmButton's label
  // branches on busy alone, so the button perpetually read "Arming…" on
  // mount despite no arm call ever firing.
  it('shows "Arm listener" (not "Arming…") on mount when VARA transport is Closed', async () => {
    // Default mock: vara_status returns closedStatus → status.state === "closed"
    // → isOpen === false. Pre-fix: button label was "Arming…". Post-fix: the
    // precondition gates `disabled`, not `busy`, so the label stays steady.
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const armBtn = await waitFor(
      () => screen.getByTestId('vara-listen-arm-btn') as HTMLButtonElement,
    );
    expect(armBtn.textContent).toBe('Arm listener');
    expect(armBtn.disabled).toBe(true);
  });

  it('shows "Arm listener" enabled once the VARA transport is Open', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const armBtn = await waitFor(() => {
      const b = screen.getByTestId('vara-listen-arm-btn') as HTMLButtonElement;
      expect(b.disabled).toBe(false);
      return b;
    });
    expect(armBtn.textContent).toBe('Arm listener');
  });
});

// ─────────────────────────────────────────────────────────────────────────
// VARA HF/FM dial (Connect section) — tuxlink-xglf. Mirrors the ARDOP/Packet
// FavoritesTabs + target + Send/Receive flow. VARA's b2f is a SINGLE blocking
// modem_vara_b2f_exchange (connect→B2F→disconnect, like Packet's packet_connect)
// requiring a prior open session — so `reached` is recorded on the call's
// resolve and `failed` in its catch. A favorite Connect is PREFILL-ONLY
// (RADIO-1); the operator's Send/Receive click is the Part 97 consent gate.
// ─────────────────────────────────────────────────────────────────────────
describe('<VaraRadioPanel> dial (Connect)', () => {
  const findRecordCalls = (invokeMock: ReturnType<typeof vi.fn>) =>
    invokeMock.mock.calls.filter(([cmd]) => cmd === 'favorite_record_attempt');

  beforeEach(async () => {
    // tuxlink-ypz3 (3a): the panel restores its target from
    // localStorage['tuxlink.lastTarget.vara-*'] on mount, and the prefill tests
    // in this block write that key — clear it so a persisted target can't leak
    // into the empty-target assertions.
    localStorage.clear();
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockReset();
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
  });

  it('renders Favorites/Recent/Manual tabs for VARA (M7 retired)', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    expect(await screen.findByRole('tab', { name: 'Favorites' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Recent' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Manual' })).toBeInTheDocument();
  });

  it('Send/Receive invokes modem_vara_b2f_exchange with the typed target + transportKind vara-hf', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);

    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    fireEvent.click(screen.getByTestId('vara-send-receive-btn'));

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith('modem_vara_b2f_exchange', {
        target: 'W7RPT-10',
        intent: 'cms',
        transportKind: 'vara-hf',
        // tuxlink-8fkkk A3/Task B: a manual target is a single dial with no freq
        // typed — freqHz null (no pre-audio tune), qsyCandidates null (legacy path).
        freqHz: null,
        qsyCandidates: null,
      });
    });
  });

  it('passes transportKind vara-fm when mounted in vara-fm mode', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<VaraRadioPanel mode={FM_MODE} onClose={() => {}} />);

    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    fireEvent.click(screen.getByTestId('vara-send-receive-btn'));

    await waitFor(() => {
      expect(invokeSpy).toHaveBeenCalledWith(
        'modem_vara_b2f_exchange',
        expect.objectContaining({ transportKind: 'vara-fm' }),
      );
    });
  });

  // ── tuxlink-8fkkk A3 + Task B: frequency element, Tune, freqHz + qsyCandidates
  describe('Frequency element + QSY candidates (tuxlink-8fkkk)', () => {
    it('Tune button is disabled when the frequency field is blank', async () => {
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      const tune = (await screen.findByTestId('vara-tune')) as HTMLButtonElement;
      expect(tune.disabled).toBe(true);
    });

    it('Tune button fires ardop_tune_rig with freqHz when a valid frequency is entered', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      const freq = (await screen.findByTestId('vara-freq')) as HTMLInputElement;
      fireEvent.change(freq, { target: { value: '14.105' } });
      const tune = (await screen.findByTestId('vara-tune')) as HTMLButtonElement;
      expect(tune.disabled).toBe(false);
      fireEvent.click(tune);
      await waitFor(() => {
        expect(invokeSpy).toHaveBeenCalledWith('ardop_tune_rig', { freqHz: 14105000 });
      });
    });

    it('prefill from Find a Station fills the frequency field (normalized MHz)', async () => {
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-freq');
      await act(async () => {
        emitGatewayPrefill({ mode: 'vara-hf', gateway: 'W7RMS-10', freq: '14105.0' });
      });
      await waitFor(() => {
        expect((screen.getByTestId('vara-freq') as HTMLInputElement).value).toBe('14.105');
      });
    });

    it('clears the frequency field when a prefilled dial carries no freq (C4 clear-on-empty)', async () => {
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      const freq = (await screen.findByTestId('vara-freq')) as HTMLInputElement;
      fireEvent.change(freq, { target: { value: '7.103' } });
      expect(freq.value).toBe('7.103');
      await act(async () => {
        emitGatewayPrefill({ mode: 'vara-hf', gateway: 'W7RMS-10' });
      });
      await waitFor(() => {
        expect((screen.getByTestId('vara-freq') as HTMLInputElement).value).toBe('');
      });
    });

    it('sends qsyCandidates on Send/Receive when a Use → supplied a ranked list', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      // Transport already open so Send/Receive enables once a target is prefilled.
      invokeSpy.mockImplementation(makeInvoke({ vara_status: openStatus }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-send-receive-btn');
      await act(async () => {
        emitGatewayPrefill({ mode: 'vara-hf', gateway: 'W7RMS-10', freq: '7.103' }, [
          { mode: 'vara-hf', gateway: 'W7RMS-10', freq: '7.103' },
          { mode: 'vara-hf', gateway: 'W7RMS-10', freq: '14.105' },
        ]);
      });
      // The prefill sets the target + freq; wait for the button to enable.
      await waitFor(() => expect(screen.getByTestId('vara-send-receive-btn')).not.toBeDisabled());
      fireEvent.click(screen.getByTestId('vara-send-receive-btn'));
      await waitFor(() => {
        expect(invokeSpy).toHaveBeenCalledWith(
          'modem_vara_b2f_exchange',
          expect.objectContaining({
            target: 'W7RMS-10',
            freqHz: 7103000,
            qsyCandidates: [
              { target: 'W7RMS-10', freq_hz: 7103000 },
              { target: 'W7RMS-10', freq_hz: 14105000 },
            ],
          }),
        );
      });
    });
  });

  // tuxlink-p6iq: Find-a-Station "Use →" (and a favorite Connect) must land the
  // operator CONNECTABLE, not on a dead-end of disabled Send/Receive. A
  // gateway-prefill auto-opens the transport (a socket, NOT a transmission —
  // Send/Receive stays the explicit consent click).
  describe('use-a-gateway auto-opens the transport (no dead-end)', () => {
    const dial = { mode: 'vara-hf' as const, gateway: 'W7RMS-10' };

    it('auto-opens the VARA transport when a gateway is used (prefill)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      invokeSpy.mockImplementation(makeInvoke()); // closed by default
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-send-receive-btn');
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await waitFor(() => {
        expect(invokeSpy).toHaveBeenCalledWith(
          'vara_open_session',
          expect.objectContaining({ intent: 'cms', transportKind: 'vara-hf' }),
        );
      });
    });

    it('lands connectable: after Use → the session opens and Send/Receive enables', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      invokeSpy.mockImplementation(makeInvoke({ vara_open_session: openStatus }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-send-receive-btn');
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await waitFor(() => {
        expect(screen.getByTestId('vara-send-receive-btn')).not.toBeDisabled();
      });
    });

    it('surfaces an error (not a silent dead-end) when the transport cannot open', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      invokeSpy.mockImplementation(makeInvoke({ vara_open_session: new Error('connection refused') }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-send-receive-btn');
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await waitFor(() => {
        expect(screen.getByTestId('vara-action-error')).toHaveTextContent(/Start failed/i);
      });
      // The operator can retry — Start stays available, no dead silence.
      expect(screen.getByTestId('vara-start-btn')).not.toBeDisabled();
    });

    it('does not re-open the transport when a gateway is used while already open', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      invokeSpy.mockImplementation(makeInvoke({ vara_status: openStatus }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await waitFor(() => expect(screen.getByTestId('vara-start-btn')).toBeDisabled()); // open
      invokeSpy.mockClear();
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await act(async () => {});
      expect(invokeSpy).not.toHaveBeenCalledWith('vara_open_session', expect.anything());
    });

    it('opens the transport exactly once for a single use (no double-open)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      invokeSpy.mockImplementation(makeInvoke({ vara_open_session: openStatus }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await screen.findByTestId('vara-send-receive-btn');
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await waitFor(() => {
        expect(screen.getByTestId('vara-send-receive-btn')).not.toBeDisabled();
      });
      const opens = invokeSpy.mock.calls.filter(([c]) => c === 'vara_open_session').length;
      expect(opens).toBe(1);
    });

    it('waits for the mount status poll before auto-opening (no spurious open when already open)', async () => {
      const core = await import('@tauri-apps/api/core');
      const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
      // Backend is ALREADY open; the mount poll reports it. A prefill racing the
      // poll must NOT fire vara_open_session against the indistinguishable initial
      // 'closed' (Codex p6iq [P1]).
      invokeSpy.mockImplementation(makeInvoke({ vara_status: openStatus }));
      renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
      await act(async () => {
        emitGatewayPrefill(dial);
      });
      await waitFor(() => expect(screen.getByTestId('vara-start-btn')).toBeDisabled()); // open
      await act(async () => {});
      expect(invokeSpy).not.toHaveBeenCalledWith('vara_open_session', expect.anything());
    });
  });

  it('shows a visible transport-closed hint pointing to Start (manual path)', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(makeInvoke({ vara_status: closedStatus }));
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    expect(await screen.findByTestId('vara-transport-hint')).toBeInTheDocument();
  });

  it('hides the transport-closed hint once the session is open', async () => {
    const core = await import('@tauri-apps/api/core');
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(makeInvoke({ vara_status: openStatus }));
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await waitFor(() => expect(screen.queryByTestId('vara-transport-hint')).toBeNull());
  });

  it('disables Send/Receive until the session is Open AND a target is present', async () => {
    const core = await import('@tauri-apps/api/core');

    // Session CLOSED: even WITH a target typed, Send/Receive stays disabled —
    // the exchange needs an open transport.
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: closedStatus }),
    );
    const { unmount } = renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await switchToManualTab();
    const closedTarget = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(closedTarget, { target: { value: 'W7RPT-10' } });
    });
    // Stays disabled despite a non-empty target (session is closed).
    await waitFor(() =>
      expect((screen.getByTestId('vara-send-receive-btn') as HTMLButtonElement).disabled).toBe(true),
    );
    unmount();

    // tuxlink-ypz3 (3a): phase 1's typed target persisted to
    // localStorage['tuxlink.lastTarget.vara-hf'] (writeLastTarget on change), and
    // the panel now RESTORES that on a fresh mount. Clear it so this phase
    // genuinely exercises the empty-target case it intends to assert.
    localStorage.clear();

    // Fresh mount, session OPEN but target empty → still disabled. (VARA loads
    // status via a one-time mount effect, so a fresh mount — not a rerender — is
    // what re-reads the new status mock.)
    (core.invoke as ReturnType<typeof vi.fn>).mockImplementation(
      makeInvoke({ vara_status: openStatus }),
    );
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    const btn = (await screen.findByTestId('vara-send-receive-btn')) as HTMLButtonElement;
    await waitFor(() => expect(btn.disabled).toBe(true));

    // Type a target → now Open + target present → enabled.
    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    await waitFor(() =>
      expect((screen.getByTestId('vara-send-receive-btn') as HTMLButtonElement).disabled).toBe(false),
    );
  });

  it('records reached on a resolved exchange and failed on a rejected one', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;

    // First: resolving exchange → reached.
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    fireEvent.click(screen.getByTestId('vara-send-receive-btn'));
    await waitFor(() => {
      const reached = findRecordCalls(invokeSpy).filter(
        ([, a]) => (a as { outcome: string }).outcome === 'reached',
      );
      expect(reached).toHaveLength(1);
      expect((reached[0][1] as { dial: { gateway: string } }).dial.gateway).toBe('W7RPT-10');
    });

    // Second: rejecting exchange → failed (recorded in the catch).
    invokeSpy.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'modem_vara_b2f_exchange') throw new Error('CMS rejected');
      return makeInvoke({ vara_status: openStatus })(cmd, args);
    });
    fireEvent.click(screen.getByTestId('vara-send-receive-btn'));
    await waitFor(() => {
      const failed = findRecordCalls(invokeSpy).filter(
        ([, a]) => (a as { outcome: string }).outcome === 'failed',
      );
      expect(failed).toHaveLength(1);
      expect((failed[0][1] as { dial: { gateway: string } }).dial.gateway).toBe('W7RPT-10');
    });
  });

  it('CONSENT NON-BYPASS: a favorite Connect pre-fills the target only, never transmits', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    const fav = {
      id: 'fav-1',
      mode: 'vara-hf' as const,
      gateway: 'W7RPT-10',
      band: '40m',
      starred: true,
      created_at: '2026-06-08T00:00:00-07:00',
      updated_at: '2026-06-08T00:00:00-07:00',
    };
    invokeSpy.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'favorites_read') return { schema_version: 1, favorites: [fav], log: [] };
      if (cmd === 'favorites_recents') return [];
      return makeInvoke({ vara_status: openStatus })(cmd, args);
    });
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);

    // Default tab is Favorites; the favorite's Connect appears there.
    const connectBtn = await screen.findByTestId('favorite-connect-fav-1');
    fireEvent.click(connectBtn);
    await new Promise((r) => setTimeout(r, 20));

    // RADIO-1: the prefill must NOT have transmitted.
    expect(
      invokeSpy.mock.calls.some(([cmd]) => cmd === 'modem_vara_b2f_exchange'),
    ).toBe(false);

    // Prefill worked: the Manual tab target now holds the gateway.
    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    expect(target.value).toBe('W7RPT-10');
  });

  it('station-picker prefill event fills the VARA target without transmitting', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);

    act(() => {
      emitGatewayPrefill({ mode: 'vara-hf', gateway: 'KQ4XYZ-10' });
    });

    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await waitFor(() => expect(target.value).toBe('KQ4XYZ-10'));
    expect(
      invokeSpy.mock.calls.some(([cmd]) => cmd === 'modem_vara_b2f_exchange'),
    ).toBe(false);
  });

  // Codex 2026-06-10 P2 #2: a pre-air ownership failure (the transport was never
  // available — e.g. the listener consumer holds it) never transmitted, so it is
  // NOT an on-air outcome and must not pollute the favorite's reach/fail history.
  it('does NOT record a failed attempt when the exchange fails pre-air (session not open)', async () => {
    const core = await import('@tauri-apps/api/core');
    const invokeSpy = core.invoke as ReturnType<typeof vi.fn>;
    invokeSpy.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'modem_vara_b2f_exchange') {
        throw new Error('VARA session not open — press Open Session (VARA HF/FM) before Send/Receive');
      }
      return makeInvoke({ vara_status: openStatus })(cmd, args);
    });
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    fireEvent.click(screen.getByTestId('vara-send-receive-btn'));
    // The error surfaces, but NO `failed` attempt is recorded (it never went on-air).
    await waitFor(() =>
      expect(screen.getByTestId('vara-action-error')).toHaveTextContent('session not open'),
    );
    expect(
      findRecordCalls(invokeSpy).filter(([, a]) => (a as { outcome: string }).outcome === 'failed'),
    ).toHaveLength(0);
  });

  // Codex 2026-06-10 P2 #2: while the listener is armed it owns the single VARA
  // transport, so an outbound dial would fail pre-air. Disable Send/Receive so
  // the operator disarms first rather than clicking into a guaranteed pre-air bail.
  it('disables Send/Receive while the VARA listener is armed', async () => {
    renderPanel(<VaraRadioPanel mode={HF_MODE} onClose={() => {}} />);
    await switchToManualTab();
    const target = (await screen.findByTestId('vara-target-input')) as HTMLInputElement;
    await act(async () => {
      fireEvent.change(target, { target: { value: 'W7RPT-10' } });
    });
    // Open session + target present + listener disarmed → enabled.
    await waitFor(() =>
      expect((screen.getByTestId('vara-send-receive-btn') as HTMLButtonElement).disabled).toBe(false),
    );
    // Arm the listener (enabled because the transport is Open).
    const armBtn = screen.getByTestId('vara-listen-arm-btn') as HTMLButtonElement;
    await waitFor(() => expect(armBtn.disabled).toBe(false));
    await act(async () => {
      fireEvent.click(armBtn);
    });
    // Armed → Send/Receive disabled (listener owns the transport).
    await waitFor(() =>
      expect((screen.getByTestId('vara-send-receive-btn') as HTMLButtonElement).disabled).toBe(true),
    );
  });
});
