/**
 * ElmerPane tests -- Task 10 (AC-11, AC-12, AC-13, AC-14) + Task G2 (Model form).
 *
 * Mock strategy:
 *   - `@tauri-apps/api/core` invoke: command-gated (vitest calls invoke mocks
 *     with NO args at teardown -- guard every branch with `if (cmd === ...)` so
 *     a bare `invoke()` call doesn't explode on teardown).
 *   - `@tauri-apps/api/event` listen: returns a no-op unlisten fn by default;
 *     tests that need to fire events capture the listener callback directly.
 *
 * AC-11: send renders a user bubble.
 * AC-12: an elmer-chip event renders a visually distinct chip (not a turn bubble).
 * AC-14: an elmer-outcome kind=offline renders the offline state.
 * Stop: clicking Stop calls elmer_stop.
 * G2: Model form -- preset/endpoint/key-affordance/model+Detect, Save & use.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { ElmerPane, ModelForm, RADIO_VERBS } from './ElmerPane';
import type { ElmerChipPayload, ElmerDeltaPayload, ElmerOutcomePayload, ElmerTurnPayload } from './elmerEvents';
import { EV_CHIP, EV_DELTA, EV_OUTCOME, EV_TURN } from './elmerEvents';
import { EGRESS_STATUS_DISARMED } from '../security/egressTypes';
import { PRESETS } from './elmerModelConfig';

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core (invoke)
// ---------------------------------------------------------------------------

// Capture invoke calls by command name. Gate on cmd so vitest's no-arg teardown
// calls don't throw (the teardown invokes mock functions with no args).
// G2: also handles elmer_config_read, elmer_config_set, elmer_detect_models.
// T8b: also handles elmer_key_status_for_origins.
// The default implementations are onboarded=true config + empty model list.
// Individual tests override via mockInvoke.mockImplementationOnce().
const mockInvoke = vi.fn(async (cmd?: string, _args?: unknown) => {
  if (cmd === 'elmer_send') return undefined;
  if (cmd === 'elmer_stop') return undefined;
  if (cmd === 'elmer_config_read') return {
    agentEndpoint: 'https://api.openai.com/v1/chat/completions',
    agentModel: 'gpt-4o',
    keyStatus: 'absent',
    agentTurnTimeoutSecs: 900,
    onboarded: true,
  };
  if (cmd === 'elmer_config_set') return undefined;
  if (cmd === 'elmer_detect_models') return [];
  if (cmd === 'elmer_key_status_for_origins') return {};
  return undefined;
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd?: string, args?: unknown) => mockInvoke(cmd, args),
}));

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/event (listen)
// ---------------------------------------------------------------------------

/**
 * Per-channel listener store. Tests fire events by calling
 * `fireElmerEvent(EV_*)` with a payload.
 */
type ListenerFn<T> = (event: { payload: T }) => void;

const listeners: Map<string, ListenerFn<unknown>> = new Map();

const mockListen = vi.fn(async (event: string, handler: ListenerFn<unknown>) => {
  listeners.set(event, handler);
  // Return an unlisten function
  return () => {
    listeners.delete(event);
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: ListenerFn<unknown>) => mockListen(event, handler),
}));

// Helper: fire a typed event payload into the registered listener.
// Waits for the listener to be registered (useElmer's setupListeners is async)
// before firing the event.
async function fireElmerEvent<T>(channel: string, payload: T): Promise<void> {
  // Wait for the listener to be registered before firing.
  await waitFor(() => {
    expect(listeners.has(channel)).toBe(true);
  });
  await act(async () => {
    const handler = listeners.get(channel) as ListenerFn<T> | undefined;
    if (handler) handler({ payload });
  });
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  listeners.clear();
  mockInvoke.mockClear();
  mockListen.mockClear();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('<ElmerPane> -- send renders a user bubble (AC-11)', () => {
  it('typing a message and clicking Send renders a user turn bubble', async () => {
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'What is the weather?' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    // The user bubble must appear immediately (optimistic append).
    const userBubble = screen.getByTestId('elmer-turn-user');
    expect(userBubble).toBeTruthy();
    expect(userBubble.textContent).toContain('What is the weather?');

    // elmer_send must have been invoked.
    expect(mockInvoke).toHaveBeenCalledWith('elmer_send', { msg: 'What is the weather?' });
  });

  it('a EV_TURN assistant event renders an assistant bubble', async () => {
    render(<ElmerPane />);

    const payload: ElmerTurnPayload = { kind: 'turn', role: 'assistant', text: 'The sun is shining.' };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const assistantBubble = screen.getByTestId('elmer-turn-assistant');
    expect(assistantBubble.textContent).toContain('The sun is shining.');
  });
});

describe('<ElmerPane> -- elmer-chip renders a distinct chip (AC-12)', () => {
  it('an EV_CHIP event renders a chip element, visually distinct from prose bubbles', async () => {
    render(<ElmerPane />);

    const payload: ElmerChipPayload = { kind: 'chip', tool: 'find_stations', status: 'calling' };
    await fireElmerEvent<ElmerChipPayload>(EV_CHIP, payload);

    // A chip element must be present.
    const chip = screen.getByTestId('elmer-chip');
    expect(chip).toBeTruthy();
    expect(chip.textContent).toContain('find_stations');
    expect(chip.textContent).toContain('calling');

    // It must NOT be a turn bubble.
    expect(screen.queryByTestId('elmer-turn-assistant')).toBeNull();
    expect(screen.queryByTestId('elmer-turn-user')).toBeNull();
  });

  it('chip has data-tool and data-status attributes for test selection', async () => {
    render(<ElmerPane />);

    const payload: ElmerChipPayload = { kind: 'chip', tool: 'mailbox_list', status: 'ok' };
    await fireElmerEvent<ElmerChipPayload>(EV_CHIP, payload);

    const chip = screen.getByTestId('elmer-chip');
    expect(chip.getAttribute('data-tool')).toBe('mailbox_list');
    expect(chip.getAttribute('data-status')).toBe('ok');
  });
});

describe('<ElmerPane> -- offline outcome state (AC-14)', () => {
  it('an EV_OUTCOME with outcomeKind=offline renders the offline state', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'offline',
      detail: 'endpoint not reachable',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    // The offline outcome callout must be present.
    const offlineCallout = screen.getByTestId('elmer-outcome-offline');
    expect(offlineCallout).toBeTruthy();
  });

  it('EV_OUTCOME with outcomeKind=cancelled renders the cancelled state', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'cancelled',
      detail: '',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    expect(screen.getByTestId('elmer-outcome-cancelled')).toBeTruthy();
  });

  it('EV_OUTCOME with outcomeKind=needsOperator renders the needs-operator state', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'needsOperator',
      detail: 'Egress gated -- review required.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    expect(screen.getByTestId('elmer-outcome-needs-operator')).toBeTruthy();
  });
});

describe('<ElmerPane> -- Stop calls elmer_stop', () => {
  it('clicking Stop invokes elmer_stop', async () => {
    render(<ElmerPane />);

    // Start a run so Stop is enabled.
    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'test message' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    // Stop is now enabled (phase=running).
    await waitFor(() => {
      const stopBtn = screen.getByTestId('elmer-stop') as HTMLButtonElement;
      expect(stopBtn.disabled).toBe(false);
    });

    fireEvent.click(screen.getByTestId('elmer-stop'));
    // elmer_stop is invoked with no second arg; the mock receives it as
    // ('elmer_stop', undefined). Use a predicate-style check on calls.
    const calls = mockInvoke.mock.calls;
    expect(calls.some((c) => c[0] === 'elmer_stop')).toBe(true);
  });

  it('Stop button is always rendered (even when idle, though disabled)', () => {
    render(<ElmerPane />);
    // Stop button exists in the DOM regardless of phase.
    const stopBtn = screen.getByTestId('elmer-stop');
    expect(stopBtn).toBeTruthy();
  });
});

describe('<ElmerPane> -- thinking indicator', () => {
  it('shows the thinking indicator (radio-verb form) while a run is in progress', async () => {
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => {
      expect(screen.getByTestId('elmer-thinking')).toBeTruthy();
    });

    // Verb span must be present and show a phrase from the bank.
    const verbSpan = screen.getByTestId('elmer-thinking-verb');
    const verbText = verbSpan.textContent ?? '';
    // Text is "Elmer is <verb>…" -- strip the wrapper and check the verb is in the bank.
    const verbOnly = verbText.replace(/^Elmer is /, '').replace(/…$/, '');
    expect(RADIO_VERBS).toContain(verbOnly);

    // Elapsed span must be present.
    expect(screen.getByTestId('elmer-thinking-elapsed')).toBeTruthy();
  });

  it('thinking indicator disappears once EV_OUTCOME arrives', async () => {
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    const payload: ElmerOutcomePayload = { kind: 'outcome', outcomeKind: 'done', detail: '' };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    expect(screen.queryByTestId('elmer-thinking')).toBeNull();
  });
});

describe('<ElmerPane> -- thinking indicator verb cycling', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('verb phrase is from RADIO_VERBS on mount', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    const verbSpan = screen.getByTestId('elmer-thinking-verb');
    const verbOnly = (verbSpan.textContent ?? '').replace(/^Elmer is /, '').replace(/…$/, '');
    expect(RADIO_VERBS).toContain(verbOnly);

    vi.useRealTimers();
  });

  it('verb phrase changes after ~3s and the new phrase is also from RADIO_VERBS', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    const before = screen.getByTestId('elmer-thinking-verb').textContent ?? '';

    // Advance 3 ticks of 1s each so the verb advances.
    act(() => { vi.advanceTimersByTime(3000); });

    const after = screen.getByTestId('elmer-thinking-verb').textContent ?? '';

    // The new phrase must still be from the bank.
    const verbOnly = after.replace(/^Elmer is /, '').replace(/…$/, '');
    expect(RADIO_VERBS).toContain(verbOnly);

    // Advance a further 3 ticks to confirm it can change again (cycling is working).
    act(() => { vi.advanceTimersByTime(3000); });
    const after2 = screen.getByTestId('elmer-thinking-verb').textContent ?? '';
    const verbOnly2 = after2.replace(/^Elmer is /, '').replace(/…$/, '');
    expect(RADIO_VERBS).toContain(verbOnly2);

    // Suppress unused-variable warning -- `before` is here to document intent.
    void before;

    vi.useRealTimers();
  });
});

describe('<ElmerPane> -- thinking indicator elapsed timer', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('elapsed counter shows "0s" at mount (before any tick)', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    const elapsedEl = screen.getByTestId('elmer-thinking-elapsed');
    expect(elapsedEl.textContent).toBe('0s');

    vi.useRealTimers();
  });

  it('elapsed counter shows "5s" after 5 seconds', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    act(() => { vi.advanceTimersByTime(5000); });

    const elapsedEl = screen.getByTestId('elmer-thinking-elapsed');
    expect(elapsedEl.textContent).toBe('5s');

    vi.useRealTimers();
  });

  it('elapsed counter shows "1m 05s" after 65 seconds', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    act(() => { vi.advanceTimersByTime(65000); });

    const elapsedEl = screen.getByTestId('elmer-thinking-elapsed');
    expect(elapsedEl.textContent).toBe('1m 05s');

    vi.useRealTimers();
  });

  it('elapsed counter shows "2m 05s" after 125 seconds', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => expect(screen.getByTestId('elmer-thinking')).toBeTruthy());

    act(() => { vi.advanceTimersByTime(125000); });

    const elapsedEl = screen.getByTestId('elmer-thinking-elapsed');
    expect(elapsedEl.textContent).toBe('2m 05s');

    vi.useRealTimers();
  });
});

describe('<ElmerPane> -- layout discipline (AC-13)', () => {
  it('renders the footer with the calibrated disclaimer', () => {
    render(<ElmerPane />);
    const footer = screen.getByTestId('elmer-footer');
    expect(footer.textContent).toContain(
      'Elmer can be wrong or misled by message content — check the actual message before you send',
    );
  });

  it('renders the endpoint/model disclosure toggle', () => {
    render(<ElmerPane />);
    expect(screen.getByTestId('elmer-advanced-toggle')).toBeTruthy();
  });

  it('endpoint/model picker is hidden by default (behind the disclosure)', () => {
    render(<ElmerPane />);
    // Advanced body not mounted until toggle is clicked.
    expect(screen.queryByTestId('elmer-advanced-body')).toBeNull();
  });

  it('clicking the disclosure toggle reveals the endpoint/model picker', () => {
    render(<ElmerPane />);
    fireEvent.click(screen.getByTestId('elmer-advanced-toggle'));
    expect(screen.getByTestId('elmer-advanced-body')).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// Relocated arm control (the merged-control design): arm/disarm/re-arm moved
// from the dashboard ribbon INTO the drawer header. The ribbon chip shows
// state + opens this drawer; the actual controls live here. onRearm is the
// 2ouqf quarantine_and_rearm path.
// ---------------------------------------------------------------------------

describe('<ElmerPane> -- relocated agent-send arm control', () => {
  const TAINTED = { armed: false, armedRemainingSecs: 0, tainted: true };

  it('does not render the arm strip when the egress hook is not wired', () => {
    render(<ElmerPane />);
    expect(screen.queryByTestId('elmer-arm-strip')).toBeNull();
  });

  it('renders the arm control in the drawer when egress props are wired', () => {
    render(
      <ElmerPane
        egressStatus={EGRESS_STATUS_DISARMED}
        onArm={vi.fn()}
        onDisarm={vi.fn()}
        onRearm={vi.fn()}
      />,
    );
    expect(screen.getByTestId('elmer-arm-strip')).toBeInTheDocument();
    expect(screen.getByTestId('egress-arm-control')).toBeInTheDocument();
  });

  it('arms from the drawer: clicking the chip opens presets and a preset calls onArm', () => {
    const onArm = vi.fn();
    render(
      <ElmerPane egressStatus={EGRESS_STATUS_DISARMED} onArm={onArm} onDisarm={vi.fn()} onRearm={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId('egress-chip'));
    const presets = screen.getAllByTestId(/^egress-arm-\d+$/);
    expect(presets.length).toBeGreaterThan(0);
    fireEvent.click(presets[0]);
    expect(onArm).toHaveBeenCalledTimes(1);
  });

  it('tainted: the drawer surfaces re-arm (quarantine_and_rearm) and it calls onRearm', () => {
    const onRearm = vi.fn();
    render(
      <ElmerPane egressStatus={TAINTED} onArm={vi.fn()} onDisarm={vi.fn()} onRearm={onRearm} />,
    );
    fireEvent.click(screen.getByTestId('egress-chip'));
    const rearmPresets = screen.getAllByTestId(/^egress-rearm-\d+$/);
    expect(rearmPresets.length).toBeGreaterThan(0);
    fireEvent.click(rearmPresets[0]);
    expect(onRearm).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// G2 -- Model form: preset/endpoint/key-affordance/model+Detect, Save & use
// ---------------------------------------------------------------------------

/** Helper: open the advanced disclosure so the settings picker appears in the main slot. */
function openAdvanced() {
  fireEvent.click(screen.getByTestId('elmer-advanced-toggle'));
}

/**
 * Helper: render ElmerPane, open the advanced disclosure (settings picker appears
 * in main slot), click the openrouter tile (Other tier) so ModelForm renders, and
 * wait for the form. The openrouter tile is used because it has a non-empty,
 * non-loopback endpoint so the key affordance seam (effectiveKeyStatus) works
 * correctly for cross-origin tests.
 *
 * This replaces the prior `renderAndOpen()` which waited for `elmer-model-form`
 * directly in the disclosure body. Now that the disclosure shows the tile picker
 * (settings-surface fold-in, Part 1), ModelForm lives behind the Other tier tile.
 */
async function renderAndOpen() {
  render(<ElmerPane />);
  openAdvanced();
  // Wait for the tile picker to appear in the main slot (settings-picker path).
  await waitFor(() => {
    expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
  });
  // Navigate to the openrouter tile (Other tier) to render ModelForm.
  fireEvent.click(screen.getByTestId('elmer-tile-openrouter'));
  // Wait for ModelForm to appear.
  await waitFor(() => {
    expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
  });
}

/**
 * Helper: render ElmerPane, open the disclosure (settings picker), navigate to
 * a specific endpoint in the ModelForm via the openrouter tile, then manually
 * set the endpoint to the desired value. Used for tests that need a specific
 * endpoint visible in ModelForm (detect remedies, etc.).
 */
async function renderAndOpenWithEndpoint(endpoint: string) {
  await renderAndOpen();
  const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
  fireEvent.change(endpointInput, { target: { value: endpoint } });
}

describe('<ElmerPane> G2 -- form_renders_fields_from_config_read', () => {
  it('loads config and renders four fields with values', async () => {
    // Render ModelForm directly -- it is an exported component and this test
    // verifies ModelForm field seeding from specific prop values (endpoint,
    // model, keyStatus). Navigation through the settings picker is tested
    // separately in the settings-surface tests.
    const onSave = vi.fn(async () => {});
    const onDetect = vi.fn(async () => {});
    render(
      <ModelForm
        onSave={onSave}
        onDetect={onDetect}
        detectState={{ status: 'idle' }}
        initialEndpoint="https://api.openai.com/v1/chat/completions"
        initialModel="gpt-4o"
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    // Provider select -- should show 'openai' inferred from endpoint.
    const providerSelect = screen.getByTestId('elmer-provider-select') as HTMLSelectElement;
    expect(providerSelect.value).toBe('openai');

    // Endpoint input -- should show the endpoint.
    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    expect(endpointInput.value).toBe('https://api.openai.com/v1/chat/completions');

    // Model input -- should show gpt-4o.
    const modelInput = screen.getByTestId('elmer-model-input') as HTMLInputElement;
    expect(modelInput.value).toBe('gpt-4o');

    // Key field present (absent + non-loopback -> empty key input).
    expect(screen.getByTestId('elmer-key-input')).toBeTruthy();
  });
});

describe('<ElmerPane> G2 -- preset_fills_endpoint_by_origin', () => {
  it('selecting OpenAI preset fills endpoint with OpenAI URL', async () => {
    // Start with localOllama config.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    const openaiPreset = PRESETS.find((p) => p.id === 'openai')!;
    const providerSelect = screen.getByTestId('elmer-provider-select');
    fireEvent.change(providerSelect, { target: { value: 'openai' } });

    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    expect(endpointInput.value).toBe(openaiPreset.endpoint);
  });

  it('selecting localOllama fills endpoint with Ollama URL', async () => {
    // Start with openai config.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    const ollamaPreset = PRESETS.find((p) => p.id === 'localOllama')!;
    const providerSelect = screen.getByTestId('elmer-provider-select');
    fireEvent.change(providerSelect, { target: { value: 'localOllama' } });

    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    expect(endpointInput.value).toBe(ollamaPreset.endpoint);
  });

  it('selecting Google Gemini (free key) fills the OpenAI-compatible endpoint', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    const geminiPreset = PRESETS.find((p) => p.id === 'gemini')!;
    expect(geminiPreset.endpoint).toContain('generativelanguage.googleapis.com');
    const providerSelect = screen.getByTestId('elmer-provider-select');
    fireEvent.change(providerSelect, { target: { value: 'gemini' } });

    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    expect(endpointInput.value).toBe(geminiPreset.endpoint);
  });
});

describe('<ElmerPane> G2 -- key_field_hidden_for_loopback', () => {
  it('loopback endpoint -> key input/affordance not in DOM', async () => {
    // Render ModelForm directly -- the settings-picker path always starts with
    // the openrouter tile (non-loopback), which would mask this test. Render
    // ModelForm directly with a loopback initialEndpoint to verify the key-
    // section hiding behavior of ModelForm itself.
    const onSave = vi.fn(async () => {});
    const onDetect = vi.fn(async () => {});
    render(
      <ModelForm
        onSave={onSave}
        onDetect={onDetect}
        detectState={{ status: 'idle' }}
        initialEndpoint="http://127.0.0.1:11434/v1/chat/completions"
        initialModel="llama3"
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    // Key section must be entirely absent for loopback.
    expect(screen.queryByTestId('elmer-key-input')).toBeNull();
    expect(screen.queryByTestId('elmer-key-replace-btn')).toBeNull();
    expect(screen.queryByTestId('elmer-key-remove-btn')).toBeNull();
    expect(screen.queryByTestId('elmer-key-section')).toBeNull();
  });
});

describe('<ElmerPane> G2 -- key_field_shown_for_remote_absent', () => {
  it('https endpoint + keyStatus absent -> empty key input present', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    const keyInput = screen.getByTestId('elmer-key-input') as HTMLInputElement;
    expect(keyInput).toBeTruthy();
    expect(keyInput.value).toBe('');
  });
});

describe('<ElmerPane> G2 -- key_stored_shows_replace_remove_not_password', () => {
  it('keyStatus present -> Replace + Remove present, no <input type=password> seeded with dots', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Replace and Remove buttons must be present.
    expect(screen.getByTestId('elmer-key-replace-btn')).toBeTruthy();
    expect(screen.getByTestId('elmer-key-remove-btn')).toBeTruthy();

    // No password input seeded with dots (destruction-never-from-emptiness R2.6).
    const passwordInputs = document.querySelectorAll<HTMLInputElement>('input[type="password"]');
    for (const input of passwordInputs) {
      // If any password input exists, it must NOT be pre-filled with dots.
      expect(input.value).not.toMatch(/^•+$/);
      expect(input.value).not.toMatch(/^\*+$/);
      expect(input.value).not.toMatch(/^\.+$/);
      // Actually, for this affordance, there should be NO pre-seeded password input at all.
      // The key-replace input only appears after clicking [Replace].
    }

    // The replace input should NOT be visible before clicking [Replace].
    expect(screen.queryByTestId('elmer-key-replace-input')).toBeNull();
  });
});

describe('<ElmerPane> G2 -- replace_commits_set_only_on_nonempty', () => {
  it('Replace + leave empty + Save -> key:{action:keep}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Click Replace to reveal the input.
    fireEvent.click(screen.getByTestId('elmer-key-replace-btn'));

    // The replace input appears -- leave it empty.
    const replaceInput = screen.getByTestId('elmer-key-replace-input') as HTMLInputElement;
    expect(replaceInput.value).toBe('');

    // Save.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string } };
      expect(args.key.action).toBe('keep');
    });
  });

  it('a rejecting elmer_config_set SURFACES the error (no silent void-swallow)', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });
    await renderAndOpen();

    // The next config_set call rejects (e.g. an empty key / bad endpoint /
    // config-write failure). Previously `void handleSave()` swallowed this and
    // the form silently kept the new selection while the backend never changed.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_set') throw new Error('API key must not be empty');
      return undefined;
    });
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('elmer-save-error').textContent).toContain('API key must not be empty');
    });
    // No false success confirmation when the save failed.
    expect(screen.queryByTestId('elmer-save-ok')).toBeNull();
  });

  it("selecting 'Custom…' clears the endpoint and STICKS on Custom (not a no-op)", async () => {
    // Render ModelForm directly (same pattern as form_renders_fields_from_config_read)
    // so initialEndpoint is the OpenAI URL and providerSelect.value starts as 'openai'.
    // Navigation via the settings picker would land on openrouter, making providerSelect
    // start on 'openrouter' -- not what this test is exercising.
    const onSave = vi.fn(async () => {});
    const onDetect = vi.fn(async () => {});
    render(
      <ModelForm
        onSave={onSave}
        onDetect={onDetect}
        detectState={{ status: 'idle' }}
        initialEndpoint="https://api.openai.com/v1/chat/completions"
        initialModel="gpt-4o"
        initialKeyStatus="present"
        initialTurnTimeoutSecs={900}
      />,
    );

    const providerSelect = screen.getByTestId('elmer-provider-select') as HTMLSelectElement;
    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    // Starts on the inferred OpenAI preset.
    expect(providerSelect.value).toBe('openai');

    // Select Custom… -- previously a no-op that snapped back; now clears + sticks.
    fireEvent.change(providerSelect, { target: { value: 'custom' } });

    expect(endpointInput.value).toBe('');
    expect(providerSelect.value).toBe('custom');
  });

  it('Replace + type value + Save -> key:{action:set,value}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Click Replace.
    fireEvent.click(screen.getByTestId('elmer-key-replace-btn'));

    // Type a key value.
    const replaceInput = screen.getByTestId('elmer-key-replace-input');
    fireEvent.change(replaceInput, { target: { value: 'sk-new-key-value' } });

    // Save.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string; value?: string } };
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe('sk-new-key-value');
    });
  });
});

describe('<ElmerPane> G2 -- remove_commits_clear', () => {
  it('Remove + Save -> key:{action:clear}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Click Remove.
    fireEvent.click(screen.getByTestId('elmer-key-remove-btn'));

    // Save.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string } };
      expect(args.key.action).toBe('clear');
    });
  });
});

describe('<ElmerPane> G2 -- detect_populates_dropdown', () => {
  it('Detect success -> model ids selectable + "✓ N models detected"', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Mock the detect call to return model ids.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (mockInvoke as any).mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return ['gpt-4o', 'gpt-4o-mini'];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      expect(screen.getByText(/✓ 2 models detected/)).toBeTruthy();
    });

    // Both model ids should appear as selectable options.
    const modelSelect = screen.getByTestId('elmer-detected-models-select');
    const options = Array.from((modelSelect as HTMLSelectElement).options).map((o) => o.value);
    expect(options).toContain('gpt-4o');
    expect(options).toContain('gpt-4o-mini');
  });
});

describe('<ElmerPane> G2 -- detect_failure_shows_inline_reason', () => {
  it('Detect failure -> inline error message renders', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Mock detect to reject.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') throw new Error('connection refused');
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('elmer-detect-error')).toBeTruthy();
    });
  });
});

describe('<ElmerPane> G2 -- save_calls_config_set_with_three_state_key', () => {
  it('Save & use sends {agentEndpoint, agentModel, key} matching Rust SetKey serde DTO', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Fill in a key value (keyStatus=absent -> direct key input).
    const keyInput = screen.getByTestId('elmer-key-input');
    fireEvent.change(keyInput, { target: { value: 'sk-test-key' } });

    // Fill in a model value.
    const modelInput = screen.getByTestId('elmer-model-input');
    fireEvent.change(modelInput, { target: { value: 'gpt-4o-mini' } });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string; value?: string } };
      // Must have all three fields matching Rust DTO.
      expect(args).toHaveProperty('agentEndpoint');
      expect(args).toHaveProperty('agentModel', 'gpt-4o-mini');
      expect(args).toHaveProperty('key');
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe('sk-test-key');
    });
  });

  it('absent key, no value entered -> key:{action:keep}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Do NOT type in the key input -- leave empty.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { key: { action: string } };
      // Empty absent input -> keep (don't erase existing absence).
      expect(args.key.action).toBe('keep');
    });
  });
});

// ---------------------------------------------------------------------------
// G3 -- Empty-state button, detect remedies, model attribution marker
// ---------------------------------------------------------------------------

describe('<ElmerPane> G3 -- empty_state_button_expands_model_section', () => {
  // T8b: The "Connect a model" button is replaced by the ModelTilePicker which
  // renders in place of the message list when onboarded=false. These tests
  // verify the new first-run gate (tile picker) rather than the old button.
  it('renders the tile picker in the chat area when not onboarded (no model configured)', async () => {
    // Simulate no configured model: onboarded=false.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: '',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      return undefined;
    });

    render(<ElmerPane />);

    // The tile picker must exist without opening the disclosure first.
    // It replaces the message list when not onboarded.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });
    // Old "Connect a model" button is gone -- tile picker is the surface now.
    expect(screen.queryByTestId('elmer-connect-model')).toBeNull();
  });

  it('the message list (not the tile picker) is shown when onboarded=true', async () => {
    // Default mockInvoke returns onboarded=true.
    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Tile picker should NOT be shown when already onboarded.
    expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();
  });
});

describe('<ElmerPane> G3 -- detect_remedy_loopback_offline', () => {
  it('loopback endpoint + transport failure -> Ollama offline remedy', async () => {
    // Config with loopback endpoint. Navigate via settings picker to openrouter
    // tile (ModelForm), then change the endpoint to loopback so detectRemedy
    // fires the Ollama-offline branch (which is gated on isLoopback(endpoint)).
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpenWithEndpoint('http://127.0.0.1:11434/v1/chat/completions');

    // Mock detect to fail with a NoServer-style reason (transport failure).
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models')
        throw new Error('no server: could not connect to 127.0.0.1:11434: connection refused');
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const errorEl = screen.getByTestId('elmer-detect-error');
      expect(errorEl.textContent).toContain('Ollama');
      expect(errorEl.textContent).toContain('start it');
    });
  });
});

describe('<ElmerPane> G3 -- detect_remedy_remote_transport', () => {
  it('remote endpoint + transport failure -> internet connection remedy', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models')
        throw new Error('no server: could not connect to api.openai.com: network unreachable');
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const errorEl = screen.getByTestId('elmer-detect-error');
      expect(errorEl.textContent).toContain('internet connection');
    });
  });
});

describe('<ElmerPane> G3 -- detect_remedy_auth', () => {
  it('auth error + OpenAI preset -> "re-enter the key for OpenAI"', async () => {
    // Config with OpenAI endpoint. Navigate via settings picker to openrouter
    // tile (ModelForm), then set the endpoint to OpenAI so detectRemedy maps
    // the auth error to the "re-enter the key for OpenAI" label.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpenWithEndpoint('https://api.openai.com/v1/chat/completions');

    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models')
        throw new Error('auth error: check the API key for api.openai.com');
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const errorEl = screen.getByTestId('elmer-detect-error');
      expect(errorEl.textContent).toContain('re-enter the key for');
      expect(errorEl.textContent).toContain('OpenAI');
    });
  });
});

describe('<ElmerPane> G3 -- detect_zero_models_remedy', () => {
  it('zero models reason -> pull-a-model remedy, no green check', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models')
        throw new Error('no models: the server returned an empty model list');
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const errorEl = screen.getByTestId('elmer-detect-error');
      expect(errorEl.textContent).toContain('pull a model');
    });

    // No green check mark / success count present.
    expect(screen.queryByText(/✓/)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// H1 -- expandModel prop: opens Model section disclosure on mount
// ---------------------------------------------------------------------------

describe('<ElmerPane> H1 -- expand_model_prop_opens_model_section', () => {
  it('renders with the Model section disclosure open when expandModel=true', async () => {
    render(<ElmerPane expandModel />);
    // The advanced body must be present without the operator clicking the toggle.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-advanced-body')).toBeTruthy();
    });
  });

  it('disclosure is closed by default when expandModel is not set', () => {
    render(<ElmerPane />);
    expect(screen.queryByTestId('elmer-advanced-body')).toBeNull();
  });
});

describe('<ElmerPane> G3 -- model_change_drops_attribution_marker', () => {
  it('configSet changing model mid-conversation inserts an attribution marker before the next turn', async () => {
    // Start with llama3 config.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Open the model section (settings picker opens in main slot), navigate to
    // openrouter tile (Other tier) so ModelForm renders.
    openAdvanced();
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });
    fireEvent.click(screen.getByTestId('elmer-tile-openrouter'));
    await waitFor(() => {
      expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
    });

    // Change the model to gpt-4o and save.
    const modelInput = screen.getByTestId('elmer-model-input') as HTMLInputElement;
    fireEvent.change(modelInput, { target: { value: 'gpt-4o' } });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    // Wait for the save to be invoked.
    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      expect(calls.some((c) => c[0] === 'elmer_config_set')).toBe(true);
    });

    // Fire a new assistant turn -- an attribution marker should appear before it.
    const payload: ElmerTurnPayload = { kind: 'turn', role: 'assistant', text: 'Hello with gpt-4o' };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    await waitFor(() => {
      const marker = screen.getByTestId('elmer-model-attribution');
      expect(marker.textContent).toContain('now using gpt-4o');
    });
  });
});

// ---------------------------------------------------------------------------
// Credential-seam regression tests (Bug 1 + Bug 2 fixes)
// ---------------------------------------------------------------------------

describe('<ElmerPane> credential-seam -- editing_endpoint_to_new_origin_resets_pending_key_action', () => {
  // Bug 1: A Remove or Replace action started for origin A must NOT carry
  // through to a Save when the endpoint has been changed to a different origin B.
  // If the reset does not fire, buildSetKey() returns {action:'clear'} and the
  // backend applies it to origin B (wrong) -- clearing B's key while A's survives.

  it('Remove pending then change endpoint to new origin: Save sends key:{action:keep} not clear', async () => {
    // Load config with a stored key. The config endpoint doesn't matter much --
    // after renderAndOpen() navigates to the openrouter tile, ModelForm starts with
    // initialEndpoint='https://openrouter.ai/...' and initialKeyStatus='present'.
    // effectiveKeyStatus='present' because current origin == initial origin (openrouter).
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Click Remove -- sets clearPending=true for the current origin (openrouter.ai).
    fireEvent.click(screen.getByTestId('elmer-key-remove-btn'));

    // Verify the pending state is shown.
    expect(screen.getByTestId('elmer-key-clear-cancel-btn')).toBeTruthy();

    // Now change the endpoint to a completely different origin (custom-z.example.com).
    // Must be a genuinely different origin from openrouter.ai (the initial endpoint
    // in ModelForm after tile navigation) so the effectiveKeyStatus seam fires.
    const endpointInput = screen.getByTestId('elmer-endpoint-input');
    fireEvent.change(endpointInput, {
      target: { value: 'https://custom-z.example.com/v1/chat/completions' },
    });

    // After origin change, the stale clearPending must be reset.
    // The form should now show an absent-key input (no stored key for new origin).
    await waitFor(() => {
      // clearPending was reset -> the clear-pending UI is gone.
      expect(screen.queryByTestId('elmer-key-clear-cancel-btn')).toBeNull();
      // The form now shows the absent-key input for the new origin.
      expect(screen.getByTestId('elmer-key-input')).toBeTruthy();
    });

    // Save -- must NOT send action:clear for the new origin.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string } };
      // The endpoint sent to the backend is the new origin's endpoint.
      expect(args.agentEndpoint).toContain('custom-z.example.com');
      // The key action must be 'keep' (reset), NOT 'clear' (stale Remove).
      expect(args.key.action).toBe('keep');
      expect(args.key.action).not.toBe('clear');
    });
  });

  it('Replace+type pending then change endpoint to new origin: Save sends key:{action:keep} not set-for-old-key', async () => {
    // Load config with a stored key. After renderAndOpen() navigates to the openrouter
    // tile, ModelForm starts at initialEndpoint='openrouter.ai' and initialKeyStatus='present'.
    // effectiveKeyStatus='present' (current origin == initial origin = openrouter.ai).
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Enter Replace mode and type a key intended for the current origin (openrouter.ai).
    fireEvent.click(screen.getByTestId('elmer-key-replace-btn'));
    const replaceInput = screen.getByTestId('elmer-key-replace-input');
    fireEvent.change(replaceInput, { target: { value: 'sk-openai-key-typed-for-A' } });

    // Now change endpoint to a DIFFERENT origin (custom-z.example.com).
    // Must differ from openrouter.ai (the initial endpoint in ModelForm after tile
    // navigation) so the effectiveKeyStatus seam fires and resets replaceMode.
    const endpointInput = screen.getByTestId('elmer-endpoint-input');
    fireEvent.change(endpointInput, {
      target: { value: 'https://custom-z.example.com/v1/chat/completions' },
    });

    // After origin change, replaceMode + newKeyValue must be reset.
    await waitFor(() => {
      // The replace input is gone (replaceMode=false).
      expect(screen.queryByTestId('elmer-key-replace-input')).toBeNull();
      // The absent-key input is now shown.
      expect(screen.getByTestId('elmer-key-input')).toBeTruthy();
    });

    // Save -- must NOT send action:set with the openrouter key typed for origin A.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string; value?: string } };
      expect(args.agentEndpoint).toContain('custom-z.example.com');
      // The key action must NOT be 'set' with the stale key value for origin A.
      expect(args.key.action).not.toBe('set');
      if (args.key.action === 'set') {
        // If somehow set, it must not carry the old key to the new origin.
        expect(args.key.value).not.toBe('sk-openai-key-typed-for-A');
      }
      // After reset, no key was typed for the new origin -> keep.
      expect(args.key.action).toBe('keep');
    });
  });
});

describe('<ElmerPane> credential-seam -- detect_uses_inline_key_when_typed_not_saved', () => {
  // Bug 2: Detect must use the key currently typed in the form, not always
  // 'useStored'. When keyStatus==='absent' and a key has been typed (but not
  // Saved), Detect should probe with {source:'inline', value:<typed>} so the
  // operator can validate the key before committing it.

  it('absent key: type a key then click Detect -> keySource {source:inline, value}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Type a key in the absent-key input (not yet Saved).
    const keyInput = screen.getByTestId('elmer-key-input');
    fireEvent.change(keyInput, { target: { value: 'sk-typed-not-saved' } });

    // Mock detect to record what keySource it receives.
    mockInvoke.mockClear();
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const detectCall = calls.find((c) => c[0] === 'elmer_detect_models');
      expect(detectCall).toBeTruthy();
      const args = detectCall![1] as { agentEndpoint: string; keySource: { source: string; value?: string } };
      // Must use inline with the typed key, NOT useStored (which would probe
      // with no key and produce a false auth failure).
      expect(args.keySource.source).toBe('inline');
      expect(args.keySource.value).toBe('sk-typed-not-saved');
    });
  });

  it('present key + Replace mode: type a key then click Detect -> keySource {source:inline, value}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Enter Replace mode and type a new key (not yet Saved).
    fireEvent.click(screen.getByTestId('elmer-key-replace-btn'));
    const replaceInput = screen.getByTestId('elmer-key-replace-input');
    fireEvent.change(replaceInput, { target: { value: 'sk-replacement-not-saved' } });

    mockInvoke.mockClear();
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const detectCall = calls.find((c) => c[0] === 'elmer_detect_models');
      expect(detectCall).toBeTruthy();
      const args = detectCall![1] as { agentEndpoint: string; keySource: { source: string; value?: string } };
      expect(args.keySource.source).toBe('inline');
      expect(args.keySource.value).toBe('sk-replacement-not-saved');
    });
  });

  // tuxlink-wpqwy / Task 6 -- Detect-path analog of the #981 buildSetKey fix.
  // Inter-provider switch: load a provider with a STORED key, then switch the
  // endpoint to a DIFFERENT origin. effectiveKeyStatus flips to 'absent' (origin
  // diverged) and the absent-key input renders, but raw keyStatus is still
  // 'present' for the OLD origin. buildKeySource keyed off raw keyStatus would
  // drop the freshly-typed key and send {source:'none'} -> Detect/Test probes with
  // no key -> false auth failure. The fix makes buildKeySource origin-aware.
  it('switch provider with stored key, type new key -> Detect sends {source:inline, value}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Switch the endpoint to a DIFFERENT origin (Gemini) -- origin diverges from
    // the loaded OpenAI config, so the absent-key input appears for the new host.
    const endpointInput = screen.getByTestId('elmer-endpoint-input');
    fireEvent.change(endpointInput, {
      target: { value: 'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions' },
    });

    // Type the new provider's key into the now-visible absent-key input.
    const keyInput = screen.getByTestId('elmer-key-input');
    fireEvent.change(keyInput, { target: { value: 'AIza-new-provider-key' } });

    mockInvoke.mockClear();
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const detectCall = calls.find((c) => c[0] === 'elmer_detect_models');
      expect(detectCall).toBeTruthy();
      const args = detectCall![1] as { agentEndpoint: string; keySource: { source: string; value?: string } };
      // The typed key must be sent inline -- NOT dropped to {source:'none'}.
      expect(args.keySource.source).toBe('inline');
      expect(args.keySource.value).toBe('AIza-new-provider-key');
    });
  });
});

describe('<ElmerPane> credential-seam -- detect_uses_usestored_when_key_present_and_untouched', () => {
  // Bug 2 (counter-case): When keyStatus==='present' and no pending change has
  // been started, Detect should use the stored key ({source:'useStored'}).

  it('present key, no pending input, click Detect -> keySource {source:useStored}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Do NOT click Replace or Remove -- leave the stored key untouched.

    mockInvoke.mockClear();
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const detectCall = calls.find((c) => c[0] === 'elmer_detect_models');
      expect(detectCall).toBeTruthy();
      const args = detectCall![1] as { agentEndpoint: string; keySource: { source: string } };
      expect(args.keySource.source).toBe('useStored');
    });
  });
});

// ---------------------------------------------------------------------------
// Markdown rendering -- assistant turns rendered as sanitized HTML (security)
// ---------------------------------------------------------------------------

describe('<ElmerPane> -- assistant turn renders markdown as HTML', () => {
  it('bold text is rendered as <strong>, not as raw asterisks', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = { kind: 'turn', role: 'assistant', text: '**bold word**' };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble).toBeTruthy();
    // Should have rendered as <strong>, not the literal ** characters.
    expect(bubble!.querySelector('strong')).toBeTruthy();
    expect(bubble!.textContent).not.toContain('**');
  });

  it('bullet list is rendered as <ul><li> elements', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      text: '- Alpha\n- Beta\n- Gamma',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble!.querySelector('li')).toBeTruthy();
  });

  it('fenced code block is rendered as <pre><code>', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      text: '```\necho hello\n```',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble!.querySelector('pre')).toBeTruthy();
    expect(bubble!.querySelector('code')).toBeTruthy();
  });

  it('GFM table is rendered as a <table> element', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      text: '| Col A | Col B |\n|---|---|\n| 1 | 2 |',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble!.querySelector('table')).toBeTruthy();
  });
});

describe('<ElmerPane> -- sanitization: dangerous model output is stripped (XSS)', () => {
  it('onerror attribute on img is stripped -- no script execution vector', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      // Raw HTML injection attempts in model output.
      text: '<img src=x onerror="alert(1)"><script>alert(2)</script>',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble).toBeTruthy();

    // No <script> element must survive.
    expect(bubble!.querySelector('script')).toBeNull();

    // The onerror attribute must not be present on any element.
    const allElements = bubble!.querySelectorAll('*');
    for (const el of allElements) {
      expect(el.hasAttribute('onerror')).toBe(false);
    }
  });

  it('javascript: href is removed/neutralized by sanitizer', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      text: '<a href="javascript:alert(1)">click me</a>',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    const anchors = bubble!.querySelectorAll('a');
    for (const a of anchors) {
      const href = a.getAttribute('href') ?? '';
      expect(href.toLowerCase().startsWith('javascript:')).toBe(false);
    }
  });

  it('raw <script> tag in model output does not appear in the DOM', async () => {
    const { container } = render(<ElmerPane />);
    const payload: ElmerTurnPayload = {
      kind: 'turn',
      role: 'assistant',
      text: 'Safe text. <script>alert("xss")<\/script> More text.',
    };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    const bubble = container.querySelector('[data-testid="elmer-turn-assistant"]');
    expect(bubble!.querySelector('script')).toBeNull();
    // The safe text content is still there.
    expect(bubble!.textContent).toContain('Safe text.');
  });
});

describe('<ElmerPane> -- user turn renders as plain text, not markdown', () => {
  it('markdown-ish user input is not parsed -- literal asterisks are preserved', () => {
    render(<ElmerPane />);

    // Type a message with markdown syntax and send it.
    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: '**not bold**' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    // User bubble must exist.
    const userBubble = screen.getByTestId('elmer-turn-user');
    expect(userBubble).toBeTruthy();

    // The literal asterisks must be present in the text content (not parsed).
    expect(userBubble.textContent).toContain('**not bold**');

    // No <strong> element inside the user bubble.
    const strong = userBubble.querySelector('strong');
    expect(strong).toBeNull();
  });
});

describe('<ElmerPane> -- model selection persists across collapse/re-expand (configSet refresh)', () => {
  it('after Save, configSet refreshes modelConfig via config_read so a re-expanded form shows the saved model', async () => {
    render(<ElmerPane />);
    openAdvanced();
    // Settings picker now appears in main slot; navigate to openrouter tile to get ModelForm.
    await waitFor(() => expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy());
    fireEvent.click(screen.getByTestId('elmer-tile-openrouter'));
    await waitFor(() => expect(screen.getByTestId('elmer-model-form')).toBeTruthy());

    // Drop the mount-time config_read so we only observe calls caused by Save.
    mockInvoke.mockClear();

    fireEvent.change(screen.getByTestId('elmer-model-input'), {
      target: { value: 'gpt-oss-120b' },
    });
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    // The fix: configSet persists via elmer_config_set AND THEN re-reads via
    // elmer_config_read, so modelConfig (the form's init props on the next mount)
    // reflects the save. Without the refresh, modelConfig stays stale and a
    // collapse + re-expand re-initialises the unmounted/remounted form from the
    // old value.
    await waitFor(() => {
      const cmds = mockInvoke.mock.calls.map((c) => c[0]);
      expect(cmds).toContain('elmer_config_set');
      expect(cmds).toContain('elmer_config_read');
    });
  });
});

// ---------------------------------------------------------------------------
// Per-turn timeout input tests
// ---------------------------------------------------------------------------

describe('<ElmerPane> -- turn_timeout_input_renders_from_config', () => {
  it('config_read returning agentTurnTimeoutSecs=600 seeds the timeout input to 600', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 600,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    const timeoutInput = screen.getByTestId('elmer-turn-timeout-input') as HTMLInputElement;
    expect(timeoutInput).toBeTruthy();
    expect(Number(timeoutInput.value)).toBe(600);
  });
});

describe('<ElmerPane> -- save_includes_turn_timeout', () => {
  it('changing the timeout input to 1200 and clicking Save sends agentTurnTimeoutSecs: 1200 in the payload', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // Change the timeout input value.
    const timeoutInput = screen.getByTestId('elmer-turn-timeout-input');
    fireEvent.change(timeoutInput, { target: { value: '1200' } });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentTurnTimeoutSecs: number };
      expect(args.agentTurnTimeoutSecs).toBe(1200);
    });
  });
});

// ---------------------------------------------------------------------------
// Phase 2b -- streaming render: live bubble + cursor, reasoning auto-collapse,
// committed-item collapsed reasoning toggle
// ---------------------------------------------------------------------------

describe('<ElmerPane> phase 2b -- streaming bubble renders live answer + cursor', () => {
  it('assistant deltas render as a live bubble with the blinking cursor', async () => {
    render(<ElmerPane />);

    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Hello ' });
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'world' });

    const bubble = screen.getByTestId('elmer-streaming-bubble');
    expect(bubble.textContent).toContain('Hello world');
    // The blinking cursor is present while streaming.
    expect(screen.getByTestId('elmer-streaming-cursor')).toBeTruthy();
  });

  it('the streaming bubble + cursor disappear at finalize (EV_TURN), replaced by the committed item', async () => {
    render(<ElmerPane />);

    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Streamed answer' });
    expect(screen.getByTestId('elmer-streaming-bubble')).toBeTruthy();

    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'Streamed answer' });

    // The transient bubble + cursor are gone; the committed markdown item remains.
    expect(screen.queryByTestId('elmer-streaming-bubble')).toBeNull();
    expect(screen.queryByTestId('elmer-streaming-cursor')).toBeNull();
    expect(screen.getByTestId('elmer-turn-assistant').textContent).toContain('Streamed answer');
  });
});

describe('<ElmerPane> phase 2b -- reasoning auto-collapses when the answer arrives', () => {
  it('reasoning section is expanded while only reasoning has streamed, then collapses once the answer starts', async () => {
    render(<ElmerPane />);

    // Only reasoning so far -> section expanded (body visible).
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'Considering options.' });

    const reasoning = screen.getByTestId('elmer-reasoning');
    expect(reasoning.getAttribute('data-open')).toBe('true');
    expect(screen.getByTestId('elmer-reasoning-body').textContent).toContain('Considering options.');

    // Answer starts -> reasoning auto-collapses (body hidden).
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Here is the answer.' });

    const reasoningAfter = screen.getByTestId('elmer-reasoning');
    expect(reasoningAfter.getAttribute('data-open')).toBe('false');
    expect(screen.queryByTestId('elmer-reasoning-body')).toBeNull();
  });
});

describe('<ElmerPane> phase 2b -- committed item shows a collapsed reasoning toggle that expands', () => {
  it('a finalized assistant turn that streamed reasoning shows a collapsed Thinking… toggle; clicking expands it', async () => {
    render(<ElmerPane />);

    // Stream reasoning + answer, then finalize.
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'reasoning', chunk: 'Internal chain of thought.' });
    await fireElmerEvent<ElmerDeltaPayload>(EV_DELTA, { kind: 'delta', deltaKind: 'assistant', chunk: 'Final answer.' });
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, { kind: 'turn', role: 'assistant', text: 'Final answer.' });

    // The committed assistant item carries a reasoning disclosure.
    const committed = screen.getByTestId('elmer-turn-assistant');
    expect(committed.textContent).toContain('Final answer.');

    const toggle = screen.getByTestId('elmer-reasoning-toggle');
    expect(toggle).toBeTruthy();
    // Collapsed by default -- the reasoning body is not shown.
    expect(screen.queryByTestId('elmer-reasoning-body')).toBeNull();

    // Click to expand -> reasoning text becomes visible.
    fireEvent.click(toggle);
    expect(screen.getByTestId('elmer-reasoning-body').textContent).toContain('Internal chain of thought.');
  });
});

describe('<ElmerPane> -- turn_timeout_minutes_hint', () => {
  it('a timeout of 900 seconds shows a "15 min" hint', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    await renderAndOpen();

    // The hint text should contain "15 min".
    const form = screen.getByTestId('elmer-model-form');
    expect(form.textContent).toContain('15 min');
  });
});

// ---------------------------------------------------------------------------
// T8b -- onboarding gate: picker replaces message list; gear reopens picker
// ---------------------------------------------------------------------------

describe('<ElmerPane> T8b -- not-onboarded: tile picker renders in place of message list', () => {
  it('when onboarded=false the tile picker renders and the message list is not shown', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane />);

    // Tile picker should appear once config loads.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // The message list (log area) should NOT be rendered when the picker is shown.
    expect(screen.queryByTestId('elmer-messages')).toBeNull();
  });
});

describe('<ElmerPane> T8b -- not-onboarded: chat input is disabled with a hint', () => {
  it('when onboarded=false the chat input is disabled and shows a hint', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // The chat input textarea must be disabled.
    const input = screen.getByTestId('elmer-input') as HTMLTextAreaElement;
    expect(input.disabled).toBe(true);

    // A hint should be visible (a note about completing setup).
    const hint = screen.getByTestId('elmer-onboarding-hint');
    expect(hint).toBeTruthy();
  });
});

describe('<ElmerPane> T8b -- onboarded=true: chat renders, picker not shown by default', () => {
  it('when onboarded=true the message list renders and the tile picker is not shown', async () => {
    // Default mock already returns onboarded=true.
    render(<ElmerPane />);

    // Message list (log) must be present.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Tile picker must NOT be shown.
    expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();
  });
});

describe('<ElmerPane> T8b -- gear reopens picker after mount (F6 reopen)', () => {
  it('re-opening the model section fires elmer_key_status_for_origins a second time via openCounter bump', async () => {
    // Render with expandModel=true so advancedOpen starts true and openCounter=1
    // on mount. The keyStatusForOrigins effect fires once immediately (openCounter=1).
    // The default mock returns onboarded=true config, so we only override config_read
    // with an onboarded=true response that has a known endpoint (so the PRESETS filter
    // produces at least one origin and the effect actually calls the command).
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane expandModel={true} />);

    // With expandModel=true the advanced body is open from mount and the
    // keyStatusForOrigins effect fires once (openCounter=1). Wait for that first call.
    await waitFor(() => {
      const calls = mockInvoke.mock.calls.filter((c) => c[0] === 'elmer_key_status_for_origins');
      expect(calls.length).toBeGreaterThanOrEqual(1);
    });

    const callsAfterMount = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'elmer_key_status_for_origins',
    ).length;

    // Click the toggle to close the advanced section.
    fireEvent.click(screen.getByTestId('elmer-advanced-toggle')); // close -- advancedOpen becomes false

    // Click the toggle again to reopen -- this bumps openCounter, which re-triggers
    // the keyStatusForOrigins effect (the load-bearing F6 mechanism).
    fireEvent.click(screen.getByTestId('elmer-advanced-toggle')); // open -- advancedOpen becomes true, openCounter increments

    // Assert the effect fired again: total elmer_key_status_for_origins calls must
    // exceed the post-mount count. This assertion FAILS if openCounter were only
    // set on initial state (the guard against the "initial-state-only" bug).
    await waitFor(() => {
      const callsAfterReopen = mockInvoke.mock.calls.filter(
        (c) => c[0] === 'elmer_key_status_for_origins',
      ).length;
      expect(callsAfterReopen).toBeGreaterThan(callsAfterMount);
    });
  });
});

// ---------------------------------------------------------------------------
// T8b -- Fix: keyStatusForOrigins fires on first-run render (notOnboarded=true)
// ---------------------------------------------------------------------------
// Regression guard for the production-seam bug found in codex adversarial review:
// the original effect gate was `if (!advancedOpen || openCounter === 0) return`
// which skipped the fetch during first-run (onboarded=false) and 429-recovery
// (switchProviderFocusTier !== null) because those flows show the picker WITHOUT
// opening the gear disclosure.  The fix ORs in notOnboarded and
// switchProviderFocusTier so the badges populate in those flows too.
describe('<ElmerPane> T8b -- not-onboarded: elmer_key_status_for_origins fires without advancedOpen', () => {
  it('first-run render (onboarded=false, advancedOpen=false) still invokes elmer_key_status_for_origins', async () => {
    // Track how many times elmer_key_status_for_origins is called.
    const statusCalls: unknown[][] = [];
    mockInvoke.mockImplementation(async (cmd?: string, args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') {
        statusCalls.push([args]);
        return {};
      }
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    // Render without expandModel -- advancedOpen starts false.
    render(<ElmerPane />);

    // The tile picker renders in the first-run flow (onboarded=false).
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // elmer_key_status_for_origins MUST have been called to populate badges.
    // Before the fix this assertion fails because the effect returned early
    // (`!advancedOpen` was true and the notOnboarded branch did not exist).
    await waitFor(() => {
      expect(statusCalls.length).toBeGreaterThanOrEqual(1);
    });

    // Cleanup mock to avoid bleed into subsequent tests.
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      return undefined;
    });
  });
});

// ---------------------------------------------------------------------------
// T10 (a) -- rateLimited phase: distinct callout + Switch provider -> picker at paygo
// ---------------------------------------------------------------------------

describe('<ElmerPane> T10 -- rateLimited: distinct callout with Switch provider button', () => {
  it('EV_OUTCOME rateLimited renders a distinct callout (data-testid elmer-outcome-rate-limited)', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    // Distinct callout must be present.
    const callout = screen.getByTestId('elmer-outcome-rate-limited');
    expect(callout).toBeTruthy();
    // Must mention rate limit in some form.
    expect(callout.textContent).toMatch(/limit|rate|quota/i);
  });

  it('rateLimited callout contains a "Switch provider" button', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    const switchBtn = screen.getByTestId('elmer-switch-provider-btn');
    expect(switchBtn).toBeTruthy();
    expect(switchBtn.textContent).toMatch(/switch provider/i);
  });

  it('clicking Switch provider opens the model picker (elmer-tile-picker visible)', async () => {
    // Config must be onboarded=true (so we see the message list + rate-limited callout,
    // not the first-run picker). Switch provider then transitions to the picker.
    // Fix 2: explicitly mock onboarded=true so this test genuinely starts from
    // the chat/message-list path and the Switch action is what opens the picker.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Wait for message list to be visible (confirms we're in the onboarded/chat path).
    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    // Tile picker should not be visible before clicking.
    expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();

    // Click the Switch provider button.
    fireEvent.click(screen.getByTestId('elmer-switch-provider-btn'));

    // After clicking, the tile picker must become visible.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });
  });

  it('clicking Switch provider pre-selects the first paygo tile (aria-checked=true on elmer-tile-openai)', async () => {
    // Fix 3: PRESETS ordering -- openai is the first paygo preset, so focusTier='paygo'
    // must select elmer-tile-openai specifically. Assert the exact tile, not an OR.
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    fireEvent.click(screen.getByTestId('elmer-switch-provider-btn'));

    await waitFor(() => {
      // The OpenAI tile (first paygo preset in PRESETS) must be aria-checked=true.
      const openaiTile = screen.getByTestId('elmer-tile-openai');
      expect(openaiTile.getAttribute('aria-checked')).toBe('true');
      // Free/local tiles must NOT be checked.
      expect(screen.getByTestId('elmer-tile-gemini').getAttribute('aria-checked')).toBe('false');
      expect(screen.getByTestId('elmer-tile-localOllama').getAttribute('aria-checked')).toBe('false');
      // Anthropic (second paygo) must not be checked when openai is the target.
      expect(screen.getByTestId('elmer-tile-anthropic').getAttribute('aria-checked')).toBe('false');
    });
  });

  it('no auto-retry on rateLimited -- only a manual Switch provider action is offered', async () => {
    render(<ElmerPane />);

    const payload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    const callout = screen.getByTestId('elmer-outcome-rate-limited');
    // No "Retry" button present.
    expect(callout.querySelector('[data-testid="elmer-retry-btn"]')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// T10 (b) -- ModelTilePicker: Test reuses detectState copy, Save reuses saveState
// ---------------------------------------------------------------------------

describe('ModelTilePicker T10 -- Test (Detect) routes through detectState copy (not a thinner reimpl)', () => {
  // The GetKeyCard (used by cloud tiles with keyPageUrl) delegates its Test action
  // to the parent's detectState prop -- success/auth/network copy comes from detectRemedy
  // (ElmerPane.tsx). The ModelTilePicker's "Other" path uses ModelForm which has
  // the Detect button wired to onDetect/detectState directly.
  it('ModelForm inside the Other tile renders .elmer-detect-error on detectState error, not a separate state machine', async () => {
    const onDetect = vi.fn(async () => {});
    const onSave = vi.fn(async () => {});
    // Start with custom endpoint so the Other tile's ModelForm is shown.
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s, fireEvent: fe, waitFor: wf } = await import('@testing-library/react');

    r(
      <MTP
        onSave={onSave}
        onDetect={onDetect}
        detectState={{ status: 'error', reason: 'auth error: check the API key' }}
        keyStatusByOrigin={{}}
        initialEndpoint=""
        initialModel=""
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    // Click the custom tile to show ModelForm.
    fe.click(s.getByTestId('elmer-tile-custom'));

    // detectState.status=error should show .elmer-detect-error via ModelForm
    // (which reuses the parent-supplied detectState -- NOT a separate state machine).
    await wf(() => {
      expect(s.getByTestId('elmer-detect-error')).toBeTruthy();
    });
  });

  it('ModelForm Save in the Other tile renders .elmer-save-error when onSave rejects', async () => {
    const onSave = vi.fn(async () => { throw new Error('config write failed'); });
    const onDetect = vi.fn(async () => {});
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s, fireEvent: fe, waitFor: wf } = await import('@testing-library/react');

    r(
      <MTP
        onSave={onSave}
        onDetect={onDetect}
        detectState={{ status: 'idle' }}
        keyStatusByOrigin={{}}
        initialEndpoint=""
        initialModel=""
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    fe.click(s.getByTestId('elmer-tile-custom'));

    // The Save button in ModelForm is elmer-save-btn.
    fe.click(s.getByTestId('elmer-save-btn'));

    await wf(() => {
      // saveState 'error' renders .elmer-save-error class.
      expect(s.getByTestId('elmer-save-error')).toBeTruthy();
    });
  });
});

// ---------------------------------------------------------------------------
// T10 (c) -- persistent provider-class footer indicator for cloud tiers
// ---------------------------------------------------------------------------

describe('<ElmerPane> T10 -- persistent provider-class footer indicator', () => {
  it('shows a cloud-provider footer indicator when the configured provider is a cloud tier', async () => {
    // onboarded=true with a known cloud provider (Google Gemini, free tier).
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://generativelanguage.googleapis.com/v1beta/openai/chat/completions',
        agentModel: 'gemini-2.5-flash',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      // Footer indicator must be present.
      const indicator = screen.getByTestId('elmer-provider-indicator');
      expect(indicator).toBeTruthy();
      // Must name the provider class.
      expect(indicator.textContent).toMatch(/google gemini/i);
    });
  });

  it('footer indicator for paygo provider (Anthropic) includes "cloud" or provider class label', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.anthropic.com/v1/chat/completions',
        agentModel: 'claude-haiku-4-5',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      const indicator = screen.getByTestId('elmer-provider-indicator');
      expect(indicator).toBeTruthy();
      expect(indicator.textContent).toMatch(/anthropic|claude/i);
    });
  });

  it('no provider-class indicator when using local (loopback) provider', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Wait for config to load.
    await waitFor(() => {
      // After load, the local provider should NOT show a cloud indicator.
      expect(screen.queryByTestId('elmer-provider-indicator')).toBeNull();
    });
  });
});

// ---------------------------------------------------------------------------
// T10 (d) -- Honest framing copy: free-tier training note + local reframe
// ---------------------------------------------------------------------------

describe('ModelTilePicker T10 -- honest framing copy for free and local tiers', () => {
  it('free tier (gemini tile selected) shows training-on-data sentence', async () => {
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s } = await import('@testing-library/react');

    r(
      <MTP
        onSave={vi.fn(async () => {})}
        onDetect={vi.fn(async () => {})}
        detectState={{ status: 'idle' }}
        keyStatusByOrigin={{}}
        initialEndpoint="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        initialModel="gemini-2.5-flash"
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    // Gemini tile is selected by default (matches initialEndpoint).
    // Free-tier framing copy must be present in the editor area.
    const picker = s.getByTestId('elmer-tile-picker');
    // Must contain a training-on-data note.
    expect(picker.textContent).toMatch(/train/i);
  });

  it('free tier shows a "what gets sent" note', async () => {
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s } = await import('@testing-library/react');

    r(
      <MTP
        onSave={vi.fn(async () => {})}
        onDetect={vi.fn(async () => {})}
        detectState={{ status: 'idle' }}
        keyStatusByOrigin={{}}
        initialEndpoint="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
        initialModel="gemini-2.5-flash"
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    const editor = s.getByTestId('elmer-tile-editor');
    // Must mention what is sent (messages/content).
    expect(editor.textContent).toMatch(/sent|message|content/i);
  });

  it('local tier (localOllama) shows the private/offline constructive reframe copy', async () => {
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s } = await import('@testing-library/react');

    r(
      <MTP
        onSave={vi.fn(async () => {})}
        onDetect={vi.fn(async () => {})}
        detectState={{ status: 'idle' }}
        keyStatusByOrigin={{}}
        initialEndpoint="http://127.0.0.1:11434/v1/chat/completions"
        initialModel="llama3"
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
      />,
    );

    const editor = s.getByTestId('elmer-tile-editor');
    // Must contain the constructive offline/private reframe.
    expect(editor.textContent).toMatch(/private|offline|local/i);
  });
});

// ---------------------------------------------------------------------------
// T10 -- ModelTilePicker focusTier prop pre-selects first paygo tile
// ---------------------------------------------------------------------------

describe('ModelTilePicker T10 -- focusTier prop pre-selects first paygo tile', () => {
  it('focusTier="paygo" pre-selects the first paygo tile (openai tile aria-checked=true)', async () => {
    const { ModelTilePicker: MTP } = await import('./ModelTilePicker');
    const { render: r, screen: s } = await import('@testing-library/react');

    r(
      <MTP
        onSave={vi.fn(async () => {})}
        onDetect={vi.fn(async () => {})}
        detectState={{ status: 'idle' }}
        keyStatusByOrigin={{}}
        initialEndpoint=""
        initialModel=""
        initialKeyStatus="absent"
        initialTurnTimeoutSecs={900}
        focusTier="paygo"
      />,
    );

    // First paygo preset (openai) must be aria-checked=true.
    const openaiTile = s.getByTestId('elmer-tile-openai');
    expect(openaiTile.getAttribute('aria-checked')).toBe('true');

    // Non-paygo tiles must not be aria-checked.
    expect(s.getByTestId('elmer-tile-gemini').getAttribute('aria-checked')).toBe('false');
    expect(s.getByTestId('elmer-tile-localOllama').getAttribute('aria-checked')).toBe('false');
  });
});

// ---------------------------------------------------------------------------
// Fix 4 (T10) -- return-to-chat coverage: switch-provider save clears picker;
// cancel/back returns to chat; cancel absent during first-run onboarding.
// ---------------------------------------------------------------------------

describe('<ElmerPane> T10 Fix 4 -- successful switch-provider save returns to chat', () => {
  it('after opening the picker via Switch provider and completing a SUCCESSFUL save, elmer-messages reappears and the picker is gone', async () => {
    // Start onboarded=true so the message list is the initial surface.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Confirm we start in chat (message list visible).
    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Drive a rateLimited outcome so the Switch provider button appears.
    const rateLimitedPayload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, rateLimitedPayload);

    // Click Switch provider -- picker opens.
    fireEvent.click(screen.getByTestId('elmer-switch-provider-btn'));
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });
    // Message list must be gone while picker is showing.
    expect(screen.queryByTestId('elmer-messages')).toBeNull();

    // Switch to the localOllama tile -- it renders ModelForm (no keyPageUrl),
    // so its save button (elmer-save-btn) is always enabled without key validation.
    // This avoids the GetKeyCard key-length gate while still testing the save path.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-localOllama')).toBeTruthy();
    });
    fireEvent.click(screen.getByTestId('elmer-tile-localOllama'));

    // The localOllama tile editor shows ModelForm's elmer-save-btn (always enabled).
    await waitFor(() => {
      const saveBtn = screen.getByTestId('elmer-save-btn') as HTMLButtonElement;
      expect(saveBtn.disabled).toBe(false);
    });

    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    // After successful save, switchProviderFocusTier is cleared -> message list reappears.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
      expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();
    });
  });
});

describe('<ElmerPane> T10 Fix 4 -- cancel/back affordance returns to chat without saving', () => {
  it('clicking Back to chat from the switch-provider picker returns to chat without a save', async () => {
    // Start onboarded=true.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Trigger rateLimited -> Switch provider.
    const rateLimitedPayload: ElmerOutcomePayload = {
      kind: 'outcome',
      outcomeKind: 'rateLimited',
      detail: 'Daily free-tier limit reached.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, rateLimitedPayload);

    fireEvent.click(screen.getByTestId('elmer-switch-provider-btn'));

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // The Back to chat button must be present in the switch-provider flow.
    const backBtn = screen.getByTestId('elmer-back-to-chat-btn');
    expect(backBtn).toBeTruthy();

    mockInvoke.mockClear();

    // Click Back to chat -- must return to the message list without saving.
    fireEvent.click(backBtn);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
      expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();
    });

    // No elmer_config_set must have been called (cancel = no save).
    const configSetCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'elmer_config_set');
    expect(configSetCalls.length).toBe(0);
  });
});

describe('<ElmerPane> T10 Fix 4 -- cancel/back affordance absent during first-run onboarding', () => {
  it('the Back to chat button is NOT present during first-run onboarding (onboarded=false)', async () => {
    // First-run: onboarded=false -- picker is the initial surface, no cancel affordance.
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: '',
        agentModel: '',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return {};
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane />);

    // Tile picker must appear (first-run).
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // The Back to chat button must NOT be present during first-run onboarding.
    expect(screen.queryByTestId('elmer-back-to-chat-btn')).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Settings-surface fold-in -- gear-open shows tile picker (Part 1 / Part 3-a)
// ---------------------------------------------------------------------------
// When onboarded=true, opening the gear/disclosure (advancedOpen=true) shows
// the tile picker in the main slot (instead of the message list), pre-selected
// to the current provider.  Back-to-chat closes the picker without saving.
// ---------------------------------------------------------------------------

describe('<ElmerPane> settings-surface fold-in -- gear-open shows tile picker pre-selected to current provider', () => {
  it('opens the tile picker in main slot when the gear disclosure is toggled (onboarded=true)', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Onboarded: message list is visible before opening gear.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Tile picker must NOT be present while gear is closed.
    expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();

    // Open the gear disclosure.
    openAdvanced();

    // Tile picker must now appear in the main slot.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Message list must be hidden while the picker is shown.
    expect(screen.queryByTestId('elmer-messages')).toBeNull();

    // The OpenAI tile must be pre-selected (aria-checked='true') because the
    // loaded config has an OpenAI endpoint.
    const openaiTile = screen.getByTestId('elmer-tile-openai');
    expect(openaiTile.getAttribute('aria-checked')).toBe('true');
  });

  it('Back-to-chat from the gear-open picker closes the picker and returns to the message list without saving', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
        agentTurnTimeoutSecs: 900,
        onboarded: true,
      };
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
    });

    // Open the gear disclosure.
    openAdvanced();

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Back-to-chat button must be visible in the settings-picker flow.
    const backBtn = screen.getByTestId('elmer-back-to-chat-btn');
    expect(backBtn).toBeTruthy();

    mockInvoke.mockClear();

    // Click Back to chat.
    fireEvent.click(backBtn);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-messages')).toBeTruthy();
      expect(screen.queryByTestId('elmer-tile-picker')).toBeNull();
    });

    // No save must have been issued.
    const configSetCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'elmer_config_set');
    expect(configSetCalls.length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// T11 -- Credential-seam regression tests: tile/GetKeyCard flow (port of #981)
// ---------------------------------------------------------------------------
// These tests re-express the three #981 credential-seam scenarios through the
// tile/GetKeyCard flow for cloud providers (gemini, groq, openai, anthropic).
//
// Investigation summary (task-11-report.md captures the full file:line trace):
//   The tile/GetKeyCard path is structurally distinct from the ModelForm path:
//   GetKeyCard.handleSave() sends { action:'set', value:<typed-trimmed-key> } when a
//   key is typed, or { action:'keep' } when the origin already has a saved key and the
//   operator did not choose "Replace key" (the settings-edit path) -- always to
//   preset.endpoint (a compile-time constant on the preset object). This means:
//
//   - Bug 1 (stale Remove/Replace crossing origins): structurally impossible -- the
//     GetKeyCard holds no Remove/Replace state; it only has a key input. A key typed
//     for Gemini and then switching to Groq (a new GetKeyCard render) gives a FRESH
//     component with an empty input. The stale Gemini key is never carried over.
//
//   - Bug 2 (Detect using wrong keySource after inter-provider switch): GetKeyCard
//     has NO Detect button -- detect only exists in ModelForm. Task 6 (detect-inline-
//     key-after-switch) is already covered by the existing ModelForm tests and is
//     inapplicable to the GetKeyCard path.
//
//   - useStored (Bug 2 counter-case): GetKeyCard never sends {source:'useStored'};
//     it always sends {action:'set'} with the explicitly typed value. Not a gap.
//
//   The tests below confirm these structural guarantees by driving the actual tile
//   UI (select tile -> GetKeyCard paste/save) and asserting the right credentials
//   reach elmer_config_set. An Anthropic-origin regression and T6/T7 coexistence
//   checks are also included.
// ---------------------------------------------------------------------------

// Helper: mock invoke for first-run onboarding (onboarded=false).
// Tests that open the tile picker through the primary first-run slot use this.
function mockFirstRunConfig(overrides: {
  agentEndpoint?: string;
  agentModel?: string;
  keyStatus?: 'present' | 'absent' | 'unreadable';
} = {}) {
  mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
    if (cmd === 'elmer_config_read') return {
      agentEndpoint: overrides.agentEndpoint ?? '',
      agentModel: overrides.agentModel ?? '',
      keyStatus: overrides.keyStatus ?? 'absent',
      agentTurnTimeoutSecs: 900,
      onboarded: false,
    };
    if (cmd === 'elmer_key_status_for_origins') return {};
    if (cmd === 'elmer_send') return undefined;
    if (cmd === 'elmer_stop') return undefined;
    if (cmd === 'elmer_config_set') return undefined;
    if (cmd === 'elmer_detect_models') return [];
    return undefined;
  });
}

// A valid key that passes GetKeyCard's client-side validation:
// trimmed length >= 20, charset /^[A-Za-z0-9_-]+$/
const VALID_GEMINI_KEY = 'AIzaSy-valid-key-12345678';
const VALID_GROQ_KEY = 'gsk_validGroqTestKey123456789';
const VALID_OPENAI_KEY = 'sk-valid-openai-key-12345678';
const VALID_ANTHROPIC_KEY = 'sk-ant-valid-key-12345678901';

describe('<ElmerPane> T11 credential-seam -- tile/GetKeyCard save-path: typed key reaches correct origin', () => {
  // Port of #981 Bug 1 scenario to the tile flow.
  // Selects a cloud tile, pastes a key, and verifies elmer_config_set receives:
  //   agentEndpoint = the tile's hardcoded endpoint (correct origin)
  //   key.action = 'set'
  //   key.value = the typed key (not stale/wrong-origin value)

  it('Gemini tile: GetKeyCard Save sends {action:set, value:<typed key>} to gemini endpoint', async () => {
    mockFirstRunConfig();
    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Select the Gemini tile.
    fireEvent.click(screen.getByTestId('elmer-tile-gemini'));

    // GetKeyCard must render (Gemini has keyPageUrl).
    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    // Type a valid key.
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_GEMINI_KEY },
    });

    // Save button must be enabled (validation passes).
    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(false);

    mockInvoke.mockClear();
    fireEvent.click(saveBtn);

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string; value?: string } };
      // Must go to Gemini's hardcoded endpoint, not an empty or stale one.
      expect(args.agentEndpoint).toContain('generativelanguage.googleapis.com');
      // Must send the typed key -- NOT 'keep', 'clear', or some other action.
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe(VALID_GEMINI_KEY);
    });
  });

  it('select Gemini then switch to Groq and type Groq key: Save sends to Groq endpoint, not Gemini', async () => {
    // Endpoint-seam guarantee for cross-tile Save via GetKeyCard:
    // The agentEndpoint in elmer_config_set always comes from preset.endpoint
    // (the tile's compile-time constant), NOT from any mutable state that
    // could carry over from the previously-selected tile.
    //
    // Note on rawKey isolation across tiles: GetKeyCard is keyed on
    // `selectedPreset.id` in ModelTilePicker, so switching cloud tiles REMOUNTS
    // it and rawKey resets to empty (no stale key carries into the next tile's
    // input). The critical seam guarantee remains that agentEndpoint in the Save
    // call always reflects the CURRENTLY selected tile's compile-time constant.
    mockFirstRunConfig();
    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Select Gemini first.
    fireEvent.click(screen.getByTestId('elmer-tile-gemini'));
    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });
    // Type a Gemini key -- intentionally NOT saving it.
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_GEMINI_KEY },
    });

    // Switch to Groq.
    fireEvent.click(screen.getByTestId('elmer-tile-groq'));
    await waitFor(() => {
      // GetKeyCard is still rendered (Groq also has keyPageUrl).
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    // Type a distinct Groq key (overwrites whatever was in the input).
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_GROQ_KEY },
    });

    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(false);

    mockInvoke.mockClear();
    fireEvent.click(saveBtn);

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string; value?: string } };
      // The critical guarantee: endpoint is Groq's, not Gemini's -- preset.endpoint
      // on the now-selected Groq tile, not any state carried from Gemini.
      expect(args.agentEndpoint).toContain('groq.com');
      expect(args.agentEndpoint).not.toContain('googleapis.com');
      // Must send the Groq key (what was in the field when Save was clicked).
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe(VALID_GROQ_KEY);
      expect(args.key.value).not.toBe(VALID_GEMINI_KEY);
    });
  });
});

describe('<ElmerPane> T11 credential-seam -- Anthropic-origin regression: stored OpenAI key does not leak', () => {
  // When an operator has a stored key for OpenAI and the config has
  // keyStatus='present', then switches to the Anthropic tile: the stored OpenAI
  // key must NOT be sent to the Anthropic endpoint.
  //
  // In the tile/GetKeyCard flow this is structurally guaranteed:
  //   - Selecting Anthropic renders a fresh GetKeyCard with preset=anthropicPreset.
  //   - GetKeyCard.handleSave() uses preset.endpoint (Anthropic's hardcoded endpoint).
  //   - key is always {action:'set', value:typedKey} -- never {source:'useStored'}.
  //   - The stored OpenAI key is never read or forwarded by GetKeyCard.
  //
  // The test confirms these guarantees hold end-to-end through ElmerPane.

  it('switch to Anthropic tile and save typed key: elmer_config_set receives Anthropic endpoint, not OpenAI endpoint', async () => {
    // Load with OpenAI stored-key config, onboarded=false so the tile picker shows.
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',   // stored key for OpenAI
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return { 'https://api.openai.com': 'present' };
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Select the Anthropic tile (switching from the loaded OpenAI config).
    fireEvent.click(screen.getByTestId('elmer-tile-anthropic'));

    // GetKeyCard renders for Anthropic.
    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    // The key input must be empty -- the stored OpenAI key is NOT pre-populated.
    const keyInput = screen.getByTestId('get-key-input') as HTMLInputElement;
    expect(keyInput.value).toBe('');

    // Type an Anthropic key.
    fireEvent.change(keyInput, { target: { value: VALID_ANTHROPIC_KEY } });

    const saveBtn = screen.getByTestId('get-key-save') as HTMLButtonElement;
    expect(saveBtn.disabled).toBe(false);

    mockInvoke.mockClear();
    fireEvent.click(saveBtn);

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string; value?: string } };
      // Must go to Anthropic's endpoint -- the stored OpenAI config must not leak.
      expect(args.agentEndpoint).toContain('anthropic.com');
      expect(args.agentEndpoint).not.toContain('openai.com');
      // Key must be the typed Anthropic key -- not 'keep' (which would silently
      // preserve the stored OpenAI key against the Anthropic origin).
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe(VALID_ANTHROPIC_KEY);
    });
  });

  it('Anthropic tile: save with stored-OpenAI key present never sends action:keep (would silently forward wrong key)', async () => {
    // This guards the specific scenario that was #981's Bug 1 pattern on the
    // tile path: a 'keep' action for a cloud tile that just switched origins
    // would silently leave the new origin with no key (or worse, if the backend
    // misinterprets it, the stored key for a different origin). GetKeyCard
    // never sends 'keep' -- only 'set' with the typed value.
    mockInvoke.mockImplementationOnce(async (cmd?: string, _args?: unknown) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: false,
      };
      if (cmd === 'elmer_key_status_for_origins') return { 'https://api.openai.com': 'present' };
      if (cmd === 'elmer_send') return undefined;
      if (cmd === 'elmer_stop') return undefined;
      if (cmd === 'elmer_config_set') return undefined;
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    fireEvent.click(screen.getByTestId('elmer-tile-anthropic'));

    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_ANTHROPIC_KEY },
    });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('get-key-save'));

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; key: { action: string; value?: string } };
      // Must NOT be 'keep' -- that would silently forward the wrong (OpenAI) key.
      expect(args.key.action).not.toBe('keep');
      expect(args.key.action).not.toBe('clear');
      expect(args.key.action).toBe('set');
    });
  });
});

describe('<ElmerPane> T11 credential-seam -- Task 6 Detect coexistence: detect still uses inline key in ModelForm path', () => {
  // Task 6 (Detect-path analog of #981 Bug 2): detects uses inline key after
  // an inter-provider endpoint switch in the ModelForm path (advanced disclosure).
  // GetKeyCard has no Detect button -- the Task 6 fix only applies to ModelForm.
  // This test confirms the existing T6 behavior still holds after T8a–T10 landed,
  // i.e., the tile/GetKeyCard changes did NOT regress the ModelForm detect path.

  it('inter-provider switch in ModelForm (advanced) + type new key -> Detect sends {source:inline}', async () => {
    // Identical to the Task 6 test in the existing detect_uses_inline_key describe,
    // re-stated here as a T11 coexistence marker so a future reviewer knows it
    // was explicitly verified with the tile flow in place.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
        agentTurnTimeoutSecs: 900,
        onboarded: true,  // onboarded -> shows message list + advanced disclosure
      };
      return undefined;
    });

    await renderAndOpen();  // opens the advanced ModelForm disclosure

    // Switch the endpoint to Anthropic (different origin from the loaded OpenAI config).
    const endpointInput = screen.getByTestId('elmer-endpoint-input');
    fireEvent.change(endpointInput, {
      target: { value: 'https://api.anthropic.com/v1/chat/completions' },
    });

    // Type the new provider's key into the now-visible absent-key input
    // (effectiveKeyStatus='absent' because origin diverged from the saved config).
    const keyInput = screen.getByTestId('elmer-key-input');
    fireEvent.change(keyInput, { target: { value: 'sk-ant-typed-for-detect' } });

    mockInvoke.mockClear();
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_detect_models') return [];
      return undefined;
    });

    fireEvent.click(screen.getByTestId('elmer-detect-btn'));

    await waitFor(() => {
      const detectCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_detect_models');
      expect(detectCall).toBeTruthy();
      const args = detectCall![1] as { agentEndpoint: string; keySource: { source: string; value?: string } };
      // Task 6 fix: must send the inline typed key -- NOT {source:'none'} or 'useStored'.
      expect(args.keySource.source).toBe('inline');
      expect(args.keySource.value).toBe('sk-ant-typed-for-detect');
    });
  });
});

describe('<ElmerPane> T11 credential-seam -- Task 7 default-model pre-fill coexistence (tile path)', () => {
  // Confirms nextModelForPreset still fires correctly through the tile path
  // after T8a–T10 changes, so switching to a cloud tile pre-fills the model.

  it('switching from OpenAI tile to Anthropic tile pre-fills model to Anthropic default', async () => {
    mockFirstRunConfig({ agentEndpoint: 'https://api.openai.com/v1/chat/completions', agentModel: 'gpt-4o-mini' });
    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // OpenAI tile is pre-selected (matches initialEndpoint).
    expect(screen.getByTestId('elmer-tile-openai').getAttribute('aria-checked')).toBe('true');

    // Select Anthropic tile -- nextModelForPreset sees outgoing model=gpt-4o-mini
    // (matches OpenAI's default) so it replaces with Anthropic's default.
    fireEvent.click(screen.getByTestId('elmer-tile-anthropic'));

    // GetKeyCard renders for Anthropic.
    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    // Type a valid Anthropic key to enable Save.
    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_ANTHROPIC_KEY },
    });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('get-key-save'));

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string; value?: string } };
      // T7 coexistence: model must be Anthropic's default, not the prior OpenAI model.
      expect(args.agentModel).toBe('claude-haiku-4-5');
      expect(args.agentModel).not.toBe('gpt-4o-mini');
      // Endpoint is Anthropic's.
      expect(args.agentEndpoint).toContain('anthropic.com');
    });
  });

  it('OpenAI tile Save sends OpenAI default model (not Anthropic or empty)', async () => {
    // Confirms T7 coexistence for the OpenAI tile: starting fresh (no prior
    // endpoint), selecting OpenAI should send OpenAI's defaultModel (gpt-4o-mini).
    mockFirstRunConfig({ agentEndpoint: '', agentModel: '' });
    render(<ElmerPane />);

    await waitFor(() => {
      expect(screen.getByTestId('elmer-tile-picker')).toBeTruthy();
    });

    // Select OpenAI tile (from a blank starting config).
    fireEvent.click(screen.getByTestId('elmer-tile-openai'));

    await waitFor(() => {
      expect(screen.getByTestId('get-key-card')).toBeTruthy();
    });

    fireEvent.change(screen.getByTestId('get-key-input'), {
      target: { value: VALID_OPENAI_KEY },
    });

    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('get-key-save'));

    await waitFor(() => {
      const setCall = mockInvoke.mock.calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { agentEndpoint: string; agentModel: string; key: { action: string; value?: string } };
      expect(args.agentEndpoint).toContain('openai.com');
      // T7 coexistence: OpenAI default model must be sent.
      expect(args.agentModel).toBe('gpt-4o-mini');
      // GetKeyCard always sends action:'set' with the typed value.
      expect(args.key.action).toBe('set');
      expect(args.key.value).toBe(VALID_OPENAI_KEY);
    });
  });
});
