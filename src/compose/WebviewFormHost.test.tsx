// WebviewFormHost — CSS-blind vitest (jsdom). Real-app smoke (the child
// webview actually opens, the loopback HTTP fetch succeeds, the
// form-submitted event round-trips) lives in the PR test plan; this
// suite just verifies the chrome layer renders + dispatches lifecycle
// callbacks correctly. The Tauri APIs are mocked so the React tree
// mounts under jsdom.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 9.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.2.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

// vi.mock() factories are hoisted to the top of the file, so any
// helper/spies they reference must be hoisted alongside them via
// vi.hoisted(). Otherwise the factory runs before the closed-over
// variables are initialized and the mock throws ReferenceError.
const mocks = vi.hoisted(() => {
  const webviewClose = vi.fn(async () => {});
  const WebviewWindowMock = vi.fn().mockImplementation(function (this: object) {
    Object.assign(this, { close: webviewClose });
  });
  const listenUnlisten = vi.fn();
  const listen = vi.fn(async () => listenUnlisten);
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === 'open_webview_form') {
      return { url: 'http://127.0.0.1:54321/', port: 54321, token: 'tok-xyz' };
    }
    return undefined;
  });
  return { webviewClose, WebviewWindowMock, listenUnlisten, listen, invoke };
});

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: mocks.WebviewWindowMock,
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));

// Component import MUST come after the vi.mock calls so its module-level
// `import` statements see the mocked modules.
// eslint-disable-next-line import/first
import { WebviewFormHost } from './WebviewFormHost';

// ---- Tests --------------------------------------------------------------

describe('<WebviewFormHost>', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.listen.mockClear();
    mocks.listenUnlisten.mockClear();
    mocks.webviewClose.mockClear();
    mocks.WebviewWindowMock.mockClear();
    // Reset the default invoke implementation between tests so an
    // earlier `mockImplementationOnce` (e.g. the error path) doesn't
    // leak into a later one that needs the success path.
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'open_webview_form') {
        return { url: 'http://127.0.0.1:54321/', port: 54321, token: 'tok-xyz' };
      }
      return undefined;
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders a container with the fallback submit + cancel buttons', () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('webview-form-host')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Submit \(fallback\)/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Cancel/i })).toBeInTheDocument();
  });

  it('calls onCancel when the cancel button is clicked', () => {
    const onCancel = vi.fn();
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={onCancel} />);
    screen.getByRole('button', { name: /Cancel/i }).click();
    expect(onCancel).toHaveBeenCalled();
  });

  it('disables the fallback Submit button (rescue-tool, not primary path)', () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    const fallback = screen.getByRole('button', { name: /Submit \(fallback\)/i });
    expect(fallback).toBeDisabled();
  });

  it('opens the webview session via open_webview_form on mount', async () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith('open_webview_form', { formId: 'ICS213_Initial' });
    });
  });

  it('constructs a WebviewWindow with the compose-form-<token> label', async () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(mocks.WebviewWindowMock).toHaveBeenCalled();
    });
    const call = mocks.WebviewWindowMock.mock.calls[0] as unknown as [string, unknown];
    expect(call[0]).toBe('compose-form-tok-xyz');
  });

  it('subscribes to form-submitted scoped to the webview label', async () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(mocks.listen).toHaveBeenCalled();
    });
    const call = mocks.listen.mock.calls[0] as unknown as [string, unknown, { target?: string } | undefined];
    const [event, , opts] = call;
    expect(event).toBe('form-submitted');
    // The listener is target-scoped to the child webview's label so events
    // emitted via emit_to(label, ...) on the Rust side are delivered here.
    expect(opts?.target).toBe('compose-form-tok-xyz');
  });

  it('renders an error banner when open_webview_form fails', async () => {
    mocks.invoke.mockImplementationOnce(async () => {
      throw new Error('unknown form: BAD');
    });
    render(<WebviewFormHost formId="BAD" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/Form failed to open/i);
    });
  });
});
