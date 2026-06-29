/**
 * ElmerPane tests — Task 10 (AC-11, AC-12, AC-13, AC-14).
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
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { ElmerPane } from './ElmerPane';
import type { ElmerChipPayload, ElmerOutcomePayload, ElmerTurnPayload } from './elmerEvents';
import { EV_CHIP, EV_OUTCOME, EV_TURN } from './elmerEvents';

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core (invoke)
// ---------------------------------------------------------------------------

// Capture invoke calls by command name. Gate on cmd so vitest's no-arg teardown
// calls don't throw (the teardown invokes mock functions with no args).
const mockInvoke = vi.fn(async (cmd?: string, _args?: unknown) => {
  if (cmd === 'elmer_send') return undefined;
  if (cmd === 'elmer_stop') return undefined;
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
