// WebviewFormHost — CSS-blind vitest (jsdom). Real-app smoke (the child
// webview actually attaches inside the Compose window, the loopback HTTP
// fetch succeeds, the form-submitted event round-trips) lives in the PR
// test plan; this suite just verifies the chrome layer renders +
// dispatches lifecycle callbacks correctly. The Tauri APIs are mocked so
// the React tree mounts under jsdom.
//
// Mocked APIs:
//   - @tauri-apps/api/webview     → Webview class
//   - @tauri-apps/api/window      → getCurrentWindow()
//   - @tauri-apps/api/dpi         → LogicalPosition, LogicalSize
//   - @tauri-apps/api/core        → invoke()
//   - @tauri-apps/api/event       → listen()
//
// jsdom also lacks `ResizeObserver`; a no-op stub is installed at module
// load so the in-effect `new ResizeObserver(...)` doesn't throw.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 9.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.2.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

// jsdom doesn't implement ResizeObserver. Stub it before any component
// import so the useEffect's `new ResizeObserver(...)` finds something
// callable. The stub records nothing — these tests don't exercise the
// reposition path; they assert chrome rendering + lifecycle.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
(globalThis as unknown as { ResizeObserver: typeof ResizeObserver }).ResizeObserver =
  ResizeObserverStub as unknown as typeof ResizeObserver;

// vi.mock() factories are hoisted to the top of the file, so any
// helper/spies they reference must be hoisted alongside them via
// vi.hoisted(). Otherwise the factory runs before the closed-over
// variables are initialized and the mock throws ReferenceError.
const mocks = vi.hoisted(() => {
  const webviewClose = vi.fn(async () => {});
  const webviewSetPosition = vi.fn(async () => {});
  const webviewSetSize = vi.fn(async () => {});
  const WebviewMock = vi.fn().mockImplementation(function (this: object) {
    Object.assign(this, {
      close: webviewClose,
      setPosition: webviewSetPosition,
      setSize: webviewSetSize,
    });
  });
  // Stand-in for the parent `Window` returned by getCurrentWindow(). The
  // real component just passes this object through to `new Webview(...)`;
  // it doesn't call methods on it, so an empty marker object suffices.
  const parentWindowStub = { __isParentWindowStub: true };
  const getCurrentWindow = vi.fn(() => parentWindowStub);
  // LogicalPosition / LogicalSize are constructors that the component
  // instantiates and passes to webview.setPosition / setSize. We mock
  // them as identity-style constructors so the test can introspect
  // the `{x, y}` / `{width, height}` shapes if needed.
  const LogicalPositionMock = vi.fn().mockImplementation(function (this: object, x: number, y: number) {
    Object.assign(this, { x, y });
  });
  const LogicalSizeMock = vi.fn().mockImplementation(function (this: object, width: number, height: number) {
    Object.assign(this, { width, height });
  });
  const listenUnlisten = vi.fn();
  const listen = vi.fn(async () => listenUnlisten);
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === 'open_webview_form') {
      return { url: 'http://127.0.0.1:54321/', port: 54321, token: 'tok-xyz' };
    }
    return undefined;
  });
  return {
    webviewClose,
    webviewSetPosition,
    webviewSetSize,
    WebviewMock,
    parentWindowStub,
    getCurrentWindow,
    LogicalPositionMock,
    LogicalSizeMock,
    listenUnlisten,
    listen,
    invoke,
  };
});

vi.mock('@tauri-apps/api/webview', () => ({
  Webview: mocks.WebviewMock,
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: mocks.getCurrentWindow,
}));
vi.mock('@tauri-apps/api/dpi', () => ({
  LogicalPosition: mocks.LogicalPositionMock,
  LogicalSize: mocks.LogicalSizeMock,
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
    mocks.webviewSetPosition.mockClear();
    mocks.webviewSetSize.mockClear();
    mocks.WebviewMock.mockClear();
    mocks.getCurrentWindow.mockClear();
    mocks.LogicalPositionMock.mockClear();
    mocks.LogicalSizeMock.mockClear();
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

  it('renders the placeholder embed div the child Webview overlays', () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // The placeholder is the layout-reservation div the in-window
    // Webview is pixel-positioned over. Confirms the chrome includes it
    // (the absence would mean we accidentally reverted to the no-embed
    // separate-window topology).
    expect(screen.getByTestId('webview-form-host-embed')).toBeInTheDocument();
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

  it('constructs an in-window Webview attached to the current Compose window, with the compose-form-<token> label', async () => {
    render(<WebviewFormHost formId="ICS213_Initial" onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(mocks.WebviewMock).toHaveBeenCalled();
    });
    // Webview constructor signature: new Webview(parentWindow, label, options)
    const call = mocks.WebviewMock.mock.calls[0] as unknown as [unknown, string, { url?: string }];
    expect(call[0]).toBe(mocks.parentWindowStub);
    expect(call[1]).toBe('compose-form-tok-xyz');
    expect(call[2]?.url).toBe('http://127.0.0.1:54321/');
    // Confirms getCurrentWindow was the source of the parent — this is
    // the load-bearing test for "embedded in the existing Compose window,
    // not a new top-level window."
    expect(mocks.getCurrentWindow).toHaveBeenCalled();
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
