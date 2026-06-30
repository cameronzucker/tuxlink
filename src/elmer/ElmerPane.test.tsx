/**
 * ElmerPane tests — Task 10 (AC-11, AC-12, AC-13, AC-14) + Task G2 (Model form).
 *
 * Mock strategy:
 *   - `@tauri-apps/api/core` invoke: command-gated (vitest calls invoke mocks
 *     with NO args at teardown — guard every branch with `if (cmd === ...)` so
 *     a bare `invoke()` call doesn't explode on teardown).
 *   - `@tauri-apps/api/event` listen: returns a no-op unlisten fn by default;
 *     tests that need to fire events capture the listener callback directly.
 *
 * AC-11: send renders a user bubble.
 * AC-12: an elmer-chip event renders a visually distinct chip (not a turn bubble).
 * AC-14: an elmer-outcome kind=offline renders the offline state.
 * Stop: clicking Stop calls elmer_stop.
 * G2: Model form — preset/endpoint/key-affordance/model+Detect, Save & use.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { ElmerPane } from './ElmerPane';
import type { ElmerChipPayload, ElmerOutcomePayload, ElmerTurnPayload } from './elmerEvents';
import { EV_CHIP, EV_OUTCOME, EV_TURN } from './elmerEvents';
import { EGRESS_STATUS_DISARMED } from '../security/egressTypes';
import { PRESETS } from './elmerModelConfig';

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core (invoke)
// ---------------------------------------------------------------------------

// Capture invoke calls by command name. Gate on cmd so vitest's no-arg teardown
// calls don't throw (the teardown invokes mock functions with no args).
// G2: also handles elmer_config_read, elmer_config_set, elmer_detect_models.
// The default implementations are "absent" config + empty model list.
// Individual tests override via mockInvoke.mockImplementationOnce().
const mockInvoke = vi.fn(async (cmd?: string, _args?: unknown) => {
  if (cmd === 'elmer_send') return undefined;
  if (cmd === 'elmer_stop') return undefined;
  if (cmd === 'elmer_config_read') return {
    agentEndpoint: 'https://api.openai.com/v1/chat/completions',
    agentModel: 'gpt-4o',
    keyStatus: 'absent',
  };
  if (cmd === 'elmer_config_set') return undefined;
  if (cmd === 'elmer_detect_models') return [];
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

describe('<ElmerPane> — send renders a user bubble (AC-11)', () => {
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

describe('<ElmerPane> — elmer-chip renders a distinct chip (AC-12)', () => {
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

describe('<ElmerPane> — offline outcome state (AC-14)', () => {
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
      detail: 'Egress gated — review required.',
    };
    await fireElmerEvent<ElmerOutcomePayload>(EV_OUTCOME, payload);

    expect(screen.getByTestId('elmer-outcome-needs-operator')).toBeTruthy();
  });
});

describe('<ElmerPane> — Stop calls elmer_stop', () => {
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

describe('<ElmerPane> — thinking indicator', () => {
  it('shows "Elmer is thinking…" while a run is in progress', async () => {
    render(<ElmerPane />);

    const input = screen.getByTestId('elmer-input');
    fireEvent.change(input, { target: { value: 'question' } });
    fireEvent.click(screen.getByTestId('elmer-send'));

    await waitFor(() => {
      expect(screen.getByTestId('elmer-thinking')).toBeTruthy();
    });
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

describe('<ElmerPane> — layout discipline (AC-13)', () => {
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

describe('<ElmerPane> — relocated agent-send arm control', () => {
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
// G2 — Model form: preset/endpoint/key-affordance/model+Detect, Save & use
// ---------------------------------------------------------------------------

/** Helper: open the advanced disclosure so form fields are visible. */
function openAdvanced() {
  fireEvent.click(screen.getByTestId('elmer-advanced-toggle'));
}

/** Helper: render ElmerPane and open the advanced section. */
async function renderAndOpen() {
  render(<ElmerPane />);
  openAdvanced();
  // Wait for the form to load config (elmer_config_read is async).
  await waitFor(() => {
    expect(screen.getByTestId('elmer-model-form')).toBeTruthy();
  });
}

describe('<ElmerPane> G2 — form_renders_fields_from_config_read', () => {
  it('loads config and renders four fields with values', async () => {
    // Default mockInvoke returns: endpoint=openai, model=gpt-4o, keyStatus=absent.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

    // Provider select — should show 'openai' inferred from endpoint.
    const providerSelect = screen.getByTestId('elmer-provider-select') as HTMLSelectElement;
    expect(providerSelect.value).toBe('openai');

    // Endpoint input — should show the endpoint.
    const endpointInput = screen.getByTestId('elmer-endpoint-input') as HTMLInputElement;
    expect(endpointInput.value).toBe('https://api.openai.com/v1/chat/completions');

    // Model input — should show gpt-4o.
    const modelInput = screen.getByTestId('elmer-model-input') as HTMLInputElement;
    expect(modelInput.value).toBe('gpt-4o');

    // Key field present (absent + non-loopback → empty key input).
    expect(screen.getByTestId('elmer-key-input')).toBeTruthy();
  });
});

describe('<ElmerPane> G2 — preset_fills_endpoint_by_origin', () => {
  it('selecting OpenAI preset fills endpoint with OpenAI URL', async () => {
    // Start with localOllama config.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
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
});

describe('<ElmerPane> G2 — key_field_hidden_for_loopback', () => {
  it('loopback endpoint → key input/affordance not in DOM', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

    // Key section must be entirely absent for loopback.
    expect(screen.queryByTestId('elmer-key-input')).toBeNull();
    expect(screen.queryByTestId('elmer-key-replace-btn')).toBeNull();
    expect(screen.queryByTestId('elmer-key-remove-btn')).toBeNull();
    expect(screen.queryByTestId('elmer-key-section')).toBeNull();
  });
});

describe('<ElmerPane> G2 — key_field_shown_for_remote_absent', () => {
  it('https endpoint + keyStatus absent → empty key input present', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

    const keyInput = screen.getByTestId('elmer-key-input') as HTMLInputElement;
    expect(keyInput).toBeTruthy();
    expect(keyInput.value).toBe('');
  });
});

describe('<ElmerPane> G2 — key_stored_shows_replace_remove_not_password', () => {
  it('keyStatus present → Replace + Remove present, no <input type=password> seeded with dots', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
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

describe('<ElmerPane> G2 — replace_commits_set_only_on_nonempty', () => {
  it('Replace + leave empty + Save → key:{action:keep}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
      };
      return undefined;
    });

    await renderAndOpen();

    // Click Replace to reveal the input.
    fireEvent.click(screen.getByTestId('elmer-key-replace-btn'));

    // The replace input appears — leave it empty.
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

  it('Replace + type value + Save → key:{action:set,value}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
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

describe('<ElmerPane> G2 — remove_commits_clear', () => {
  it('Remove + Save → key:{action:clear}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
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

describe('<ElmerPane> G2 — detect_populates_dropdown', () => {
  it('Detect success → model ids selectable + "✓ N models detected"', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
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

describe('<ElmerPane> G2 — detect_failure_shows_inline_reason', () => {
  it('Detect failure → inline error message renders', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
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

describe('<ElmerPane> G2 — save_calls_config_set_with_three_state_key', () => {
  it('Save & use sends {agentEndpoint, agentModel, key} matching Rust SetKey serde DTO', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

    // Fill in a key value (keyStatus=absent → direct key input).
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

  it('absent key, no value entered → key:{action:keep}', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

    // Do NOT type in the key input — leave empty.
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('elmer-save-btn'));

    await waitFor(() => {
      const calls = mockInvoke.mock.calls;
      const setCall = calls.find((c) => c[0] === 'elmer_config_set');
      expect(setCall).toBeTruthy();
      const args = setCall![1] as { key: { action: string } };
      // Empty absent input → keep (don't erase existing absence).
      expect(args.key.action).toBe('keep');
    });
  });
});

// ---------------------------------------------------------------------------
// G3 — Empty-state button, detect remedies, model attribution marker
// ---------------------------------------------------------------------------

describe('<ElmerPane> G3 — empty_state_button_expands_model_section', () => {
  it('renders a "Connect a model" button in the chat area when no model is configured', async () => {
    // Simulate no configured model: empty endpoint and model.
    // The config_read mock returns empty strings; the button appears immediately
    // without needing to open the disclosure first.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: '',
        agentModel: '',
        keyStatus: 'absent',
      };
      return undefined;
    });

    render(<ElmerPane />);

    // The button must exist without opening the disclosure first.
    // It appears in the messages area when no model is configured.
    const connectBtn = await screen.findByTestId('elmer-connect-model');
    expect(connectBtn).toBeTruthy();
    expect(connectBtn.textContent).toContain('Connect a model');
  });

  it('clicking the Connect a model button opens the Model section disclosure', async () => {
    // Simulate no configured model.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: '',
        agentModel: '',
        keyStatus: 'absent',
      };
      return undefined;
    });

    render(<ElmerPane />);

    // The advanced body should NOT be open initially.
    expect(screen.queryByTestId('elmer-advanced-body')).toBeNull();

    const connectBtn = await screen.findByTestId('elmer-connect-model');
    fireEvent.click(connectBtn);

    // The Model section disclosure should now be open.
    await waitFor(() => {
      expect(screen.getByTestId('elmer-advanced-body')).toBeTruthy();
    });
  });
});

describe('<ElmerPane> G3 — detect_remedy_loopback_offline', () => {
  it('loopback endpoint + transport failure → Ollama offline remedy', async () => {
    // Config with loopback endpoint.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
      };
      return undefined;
    });

    await renderAndOpen();

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

describe('<ElmerPane> G3 — detect_remedy_remote_transport', () => {
  it('remote endpoint + transport failure → internet connection remedy', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'absent',
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

describe('<ElmerPane> G3 — detect_remedy_auth', () => {
  it('auth error + OpenAI preset → "re-enter the key for OpenAI"', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'https://api.openai.com/v1/chat/completions',
        agentModel: 'gpt-4o',
        keyStatus: 'present',
      };
      return undefined;
    });

    await renderAndOpen();

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

describe('<ElmerPane> G3 — detect_zero_models_remedy', () => {
  it('zero models reason → pull-a-model remedy, no green check', async () => {
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
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
// H1 — expandModel prop: opens Model section disclosure on mount
// ---------------------------------------------------------------------------

describe('<ElmerPane> H1 — expand_model_prop_opens_model_section', () => {
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

describe('<ElmerPane> G3 — model_change_drops_attribution_marker', () => {
  it('configSet changing model mid-conversation inserts an attribution marker before the next turn', async () => {
    // Start with llama3 config.
    mockInvoke.mockImplementationOnce(async (cmd?: string) => {
      if (cmd === 'elmer_config_read') return {
        agentEndpoint: 'http://127.0.0.1:11434/v1/chat/completions',
        agentModel: 'llama3',
        keyStatus: 'absent',
      };
      return undefined;
    });

    render(<ElmerPane />);

    // Open the model section and load config.
    openAdvanced();
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

    // Fire a new assistant turn — an attribution marker should appear before it.
    const payload: ElmerTurnPayload = { kind: 'turn', role: 'assistant', text: 'Hello with gpt-4o' };
    await fireElmerEvent<ElmerTurnPayload>(EV_TURN, payload);

    await waitFor(() => {
      const marker = screen.getByTestId('elmer-model-attribution');
      expect(marker.textContent).toContain('now using gpt-4o');
    });
  });
});
