// CloseBehaviorPrompt.test.tsx — tuxlink-5rvp / #882.
//
// Verifies the one-time close-behavior modal:
//   - renders only after the `show-close-prompt` backend event fires;
//   - "Keep running on close" invokes resolve_close_prompt({ quitOnClose: false });
//   - "Quit on close" invokes resolve_close_prompt({ quitOnClose: true });
//   - Escape defaults to the safe keep-running outcome (quitOnClose: false).
//
// Mocks @tauri-apps/api/core (invoke) and @tauri-apps/api/event (listen) the
// way sibling tests do (useInboundSelection.test.tsx): the listen mock captures
// the handler so the test can dispatch a synthetic event into the tree.

import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

let promptHandlers: Array<(e: { payload: unknown }) => void> = [];
const listenMock = vi.fn(
  async (event: string, cb: (e: { payload: unknown }) => void): Promise<() => void> => {
    if (event === 'show-close-prompt') promptHandlers.push(cb);
    return () => {
      if (event === 'show-close-prompt') {
        promptHandlers = promptHandlers.filter((h) => h !== cb);
      }
    };
  },
);
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) =>
    (listenMock as (...a: unknown[]) => Promise<() => void>)(...args),
}));

import { invoke } from '@tauri-apps/api/core';
import { CloseBehaviorPrompt } from './CloseBehaviorPrompt';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function fireShowPrompt() {
  act(() => {
    promptHandlers.forEach((h) => h({ payload: null }));
  });
}

async function renderAndOpen() {
  render(<CloseBehaviorPrompt />);
  // Let the async listen() registration resolve so the handler is captured.
  await waitFor(() => expect(listenMock).toHaveBeenCalled());
  fireShowPrompt();
  await screen.findByTestId('close-prompt-panel');
}

describe('CloseBehaviorPrompt', () => {
  beforeEach(() => {
    promptHandlers = [];
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('does not render before the show-close-prompt event fires', async () => {
    render(<CloseBehaviorPrompt />);
    await waitFor(() => expect(listenMock).toHaveBeenCalled());
    expect(screen.queryByTestId('close-prompt-panel')).toBeNull();
  });

  it('renders the modal with the approved copy when the event fires', async () => {
    await renderAndOpen();
    expect(
      screen.getByText('Tuxlink keeps running when you close the window'),
    ).toBeInTheDocument();
    expect(screen.getByTestId('close-prompt-keep')).toHaveTextContent(
      'Keep running on close',
    );
    expect(screen.getByTestId('close-prompt-quit')).toHaveTextContent('Quit on close');
    expect(
      screen.getByText('You can change this later in Settings.'),
    ).toBeInTheDocument();
  });

  it('"Keep running on close" invokes resolve_close_prompt with quitOnClose=false', async () => {
    await renderAndOpen();
    fireEvent.click(screen.getByTestId('close-prompt-keep'));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('resolve_close_prompt', {
        quitOnClose: false,
      }),
    );
    // Modal closes after answering.
    await waitFor(() => expect(screen.queryByTestId('close-prompt-panel')).toBeNull());
  });

  it('"Quit on close" invokes resolve_close_prompt with quitOnClose=true', async () => {
    await renderAndOpen();
    fireEvent.click(screen.getByTestId('close-prompt-quit'));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('resolve_close_prompt', {
        quitOnClose: true,
      }),
    );
  });

  it('Escape defaults to keep-running (quitOnClose=false)', async () => {
    await renderAndOpen();
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith('resolve_close_prompt', {
        quitOnClose: false,
      }),
    );
  });
});
