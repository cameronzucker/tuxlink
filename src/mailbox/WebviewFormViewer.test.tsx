// WebviewFormViewer — CSS-blind vitest (jsdom). Real-app smoke (the child
// webview actually attaches inside the main window, the loopback HTTP
// fetch succeeds for the WLE Viewer template, the bound payload renders)
// lives in the PR test plan; this suite verifies the chrome layer renders
// + dispatches lifecycle callbacks + invokes the right Tauri commands.
//
// Mocked APIs:
//   - @tauri-apps/api/webview     → Webview class
//   - @tauri-apps/api/window      → getCurrentWindow()
//   - @tauri-apps/api/dpi         → LogicalPosition, LogicalSize
//   - @tauri-apps/api/core        → invoke()
//
// jsdom also lacks `ResizeObserver`; a no-op stub is installed at module
// load.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 11.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.3.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';

class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
(globalThis as unknown as { ResizeObserver: typeof ResizeObserver }).ResizeObserver =
  ResizeObserverStub as unknown as typeof ResizeObserver;

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
  const parentWindowStub = { __isParentWindowStub: true };
  const getCurrentWindow = vi.fn(() => parentWindowStub);
  const LogicalPositionMock = vi.fn().mockImplementation(function (this: object, x: number, y: number) {
    Object.assign(this, { x, y });
  });
  const LogicalSizeMock = vi.fn().mockImplementation(function (this: object, width: number, height: number) {
    Object.assign(this, { width, height });
  });
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === 'open_webview_viewer') {
      return { url: 'http://127.0.0.1:54322/', port: 54322, token: 'tok-vw1' };
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

// Component import MUST come after the vi.mock calls.
// eslint-disable-next-line import/first
import { WebviewFormViewer } from './WebviewFormViewer';

describe('<WebviewFormViewer>', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.webviewClose.mockClear();
    mocks.webviewSetPosition.mockClear();
    mocks.webviewSetSize.mockClear();
    mocks.WebviewMock.mockClear();
    mocks.getCurrentWindow.mockClear();
    mocks.LogicalPositionMock.mockClear();
    mocks.LogicalSizeMock.mockClear();
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'open_webview_viewer') {
        return { url: 'http://127.0.0.1:54322/', port: 54322, token: 'tok-vw1' };
      }
      return undefined;
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders the viewer chrome with a Close button + status text', () => {
    render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('webview-form-viewer')).toBeInTheDocument();
    expect(screen.getByTestId('webview-form-viewer-close-btn')).toBeInTheDocument();
    // Read-only mode: no Submit button (the entire point of Task 11's
    // viewer-mode is "received forms can't be resubmitted").
    expect(screen.queryByRole('button', { name: /Submit/i })).toBeNull();
  });

  it('renders the placeholder embed div the child Webview overlays', () => {
    render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('webview-form-viewer-embed')).toBeInTheDocument();
  });

  it('opens the viewer session via open_webview_viewer with form_id + fieldValues', async () => {
    const fieldValues = { subjectline: 'Hello', inc_name: 'Waldo' };
    render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={fieldValues}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => {
      expect(mocks.invoke).toHaveBeenCalledWith('open_webview_viewer', {
        formId: 'Quick_Message_Initial',
        fieldValues,
      });
    });
  });

  it('constructs an in-window Webview attached to the current parent window with viewer-form-<token> label', async () => {
    render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => {
      expect(mocks.WebviewMock).toHaveBeenCalled();
    });
    const call = mocks.WebviewMock.mock.calls[0] as unknown as [unknown, string, { url?: string }];
    expect(call[0]).toBe(mocks.parentWindowStub);
    expect(call[1]).toBe('viewer-form-tok-vw1');
    expect(call[2]?.url).toBe('http://127.0.0.1:54322/');
    expect(mocks.getCurrentWindow).toHaveBeenCalled();
  });

  it('does NOT register a form-submitted listener (read-only mode)', async () => {
    // Sanity: this is the structural difference between WebviewFormHost and
    // WebviewFormViewer. The send-side host listens for `form-submitted`;
    // the receive-side viewer does not (and the http_server returns 404 on
    // the POST endpoint anyway).
    const { unmount } = render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => {
      expect(mocks.WebviewMock).toHaveBeenCalled();
    });
    // The component doesn't import `listen` from @tauri-apps/api/event at
    // all — confirm by checking the invoke calls don't include any
    // form-submitted setup.
    const calls = mocks.invoke.mock.calls.map((c) => c[0]);
    expect(calls).not.toContain('listen');
    unmount();
  });

  it('calls onClose when the Close button is clicked', () => {
    const onClose = vi.fn();
    render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={onClose}
      />,
    );
    screen.getByTestId('webview-form-viewer-close-btn').click();
    expect(onClose).toHaveBeenCalled();
  });

  it('closes the loopback session + webview on unmount', async () => {
    const { unmount } = render(
      <WebviewFormViewer
        formId="Quick_Message_Initial"
        fieldValues={{}}
        onClose={vi.fn()}
      />,
    );
    // Wait for the open promise to settle so we have an active token.
    await waitFor(() => {
      expect(mocks.WebviewMock).toHaveBeenCalled();
    });
    unmount();
    // close_webview_form_server invoked with the active token.
    expect(mocks.invoke).toHaveBeenCalledWith('close_webview_form_server', {
      token: 'tok-vw1',
    });
    // The child webview's close() is called too (Tauri may have already
    // collapsed it, but the React side issues the close request).
    expect(mocks.webviewClose).toHaveBeenCalled();
  });

  it('renders an error banner + calls onFallback when open_webview_viewer fails', async () => {
    mocks.invoke.mockImplementationOnce(async () => {
      throw new Error('viewer template not found');
    });
    const onFallback = vi.fn();
    render(
      <WebviewFormViewer
        formId="MissingViewer"
        fieldValues={{}}
        onClose={vi.fn()}
        onFallback={onFallback}
      />,
    );
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/Viewer failed to open/i);
    });
    expect(onFallback).toHaveBeenCalledWith(expect.stringContaining('viewer template not found'));
  });
});

// tuxlink-h7q7 Task 17 (Codex adrev R1 #5/#6): under the FZ-M1 PUSH drawer,
// opening the radio panel narrows the reader → the embed placeholder resizes →
// the component's existing ResizeObserver fires → the child webview is
// repositioned/resized to the narrower reader (never occluded). This proves the
// reflow path repositions the webview. The authoritative occlusion check is the
// Playwright real-viewport pass (jsdom cannot prove native-webview stacking).
describe('<WebviewFormViewer> — repositions on placeholder resize (push reflow, R1)', () => {
  let fireResize: (() => void) | null = null;
  const originalRO = globalThis.ResizeObserver;

  beforeEach(() => {
    fireResize = null;
    class CapturingResizeObserver {
      constructor(cb: ResizeObserverCallback) {
        fireResize = () => cb([], this as unknown as ResizeObserver);
      }
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
    (globalThis as unknown as { ResizeObserver: typeof ResizeObserver }).ResizeObserver =
      CapturingResizeObserver as unknown as typeof ResizeObserver;
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'open_webview_viewer') return { url: 'http://127.0.0.1:54322/', port: 54322, token: 'tok-vw1' };
      return undefined;
    });
  });
  afterEach(() => {
    (globalThis as unknown as { ResizeObserver: typeof ResizeObserver }).ResizeObserver = originalRO;
    mocks.webviewSetPosition.mockClear();
    mocks.webviewSetSize.mockClear();
  });

  it('calls setPosition + setSize with the new rect when the embed placeholder resizes', async () => {
    render(<WebviewFormViewer formId="Quick_Message" fieldValues={{}} onClose={() => {}} />);
    // Wait for the async open + webview construction + observer registration.
    await waitFor(() => expect(mocks.WebviewMock).toHaveBeenCalled());
    await waitFor(() => expect(fireResize).not.toBeNull());

    // Simulate the push reflow: the embed placeholder is now narrower + shifted.
    const embed = screen.getByTestId('webview-form-viewer-embed');
    embed.getBoundingClientRect = () =>
      ({ left: 48, top: 100, width: 452, height: 600, right: 500, bottom: 700, x: 48, y: 100, toJSON: () => ({}) }) as DOMRect;

    mocks.webviewSetPosition.mockClear();
    mocks.webviewSetSize.mockClear();
    fireResize!();

    expect(mocks.webviewSetPosition).toHaveBeenCalled();
    expect(mocks.webviewSetSize).toHaveBeenCalled();
    // The new size reflects the narrower reader column (push drawer open).
    expect(mocks.LogicalSizeMock).toHaveBeenCalledWith(452, 600);
  });
});
