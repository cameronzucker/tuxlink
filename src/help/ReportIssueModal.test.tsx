/**
 * Tests for ReportIssueModal (tuxlink-qjgx Task 8.2).
 *
 * Covers (per plan §10.1 + spec §8.5):
 *   - choosing-path state renders correctly
 *   - exporting state renders correctly
 *   - canceled state: shows "canceled" message, no invoke called
 *   - success state (browser opened): shows archive path
 *   - success state (no browser): shows URL textarea + Copy URL button
 *   - error state: shows error message + Copy URL / Copy path buttons
 *   - Esc key closes the modal
 *   - Copy archive path calls navigator.clipboard.writeText
 *   - Copy URL calls navigator.clipboard.writeText
 *   - Open in browser opens via the Tauri shell plugin (NOT window.open) — tuxlink-uxvn
 *   - intro state: explanation gates the OS Save As (tuxlink-uxvn)
 *   - useReportIssueController.start() → full flow: saveDialog → report_issue_flow → success state
 *   - useReportIssueController.start() → saveDialog cancel → canceled state
 *   - useReportIssueController.start() → invoke error → error state
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import React, { useState } from 'react';
import {
  ReportIssueModal,
  useReportIssueController,
  type ReportIssueState,
} from './ReportIssueModal';

// ── Mocks ────────────────────────────────────────────────────────────────────

const { mockInvoke, mockSaveDialog, mockShellOpen } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockSaveDialog: vi.fn(),
  mockShellOpen: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mockSaveDialog }));
vi.mock('@tauri-apps/plugin-shell', () => ({ open: mockShellOpen }));

beforeEach(() => {
  vi.resetAllMocks();
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
    configurable: true,
    writable: true,
  });
  mockShellOpen.mockResolvedValue(undefined);
});

// ── Render helpers ────────────────────────────────────────────────────────────

function renderModal(initialState: ReportIssueState) {
  let externalSetState: React.Dispatch<React.SetStateAction<ReportIssueState>>;
  const onClose = vi.fn();
  const onProceed = vi.fn();

  function Wrapper() {
    const [state, setState] = useState<ReportIssueState>(initialState);
    externalSetState = setState;
    return <ReportIssueModal state={state} onClose={onClose} onProceed={onProceed} />;
  }

  const utils = render(<Wrapper />);
  return { ...utils, onClose, onProceed, getState: () => undefined, setState: (s: ReportIssueState) => externalSetState(s) };
}

// ── Idle state ────────────────────────────────────────────────────────────────

describe('ReportIssueModal — idle', () => {
  it('renders nothing when state is idle', () => {
    renderModal({ kind: 'idle' });
    expect(screen.queryByTestId('report-issue-backdrop')).toBeNull();
  });
});

// ── intro state (tuxlink-uxvn: context before the OS Save As) ────────────────────

describe('ReportIssueModal — intro', () => {
  it('shows an explanation and does NOT open any OS dialog until confirmed', () => {
    renderModal({ kind: 'intro' });
    expect(screen.getByTestId('report-issue-intro')).toBeInTheDocument();
    // The OS Save As must NOT have fired just from opening the modal.
    expect(mockSaveDialog).not.toHaveBeenCalled();
  });

  it('"Create report" calls onProceed (begins the export flow)', () => {
    const { onProceed } = renderModal({ kind: 'intro' });
    fireEvent.click(screen.getByTestId('report-issue-proceed'));
    expect(onProceed).toHaveBeenCalledOnce();
  });

  it('"Cancel" closes without proceeding', () => {
    const { onClose, onProceed } = renderModal({ kind: 'intro' });
    fireEvent.click(screen.getByTestId('report-issue-cancel-intro'));
    expect(onClose).toHaveBeenCalledOnce();
    expect(onProceed).not.toHaveBeenCalled();
  });
});

// ── choosing-path state ────────────────────────────────────────────────────────

describe('ReportIssueModal — choosing-path', () => {
  it('renders the panel and opening message', () => {
    renderModal({ kind: 'choosing-path' });
    expect(screen.getByTestId('report-issue-panel')).toBeInTheDocument();
    expect(screen.getByText(/Opening Save As dialog/i)).toBeInTheDocument();
  });
});

// ── exporting state ────────────────────────────────────────────────────────────

describe('ReportIssueModal — exporting', () => {
  it('renders "Exporting logs to …" message with the path', () => {
    renderModal({ kind: 'exporting', path: '/tmp/test.tar.zst' });
    expect(screen.getByText(/exporting logs to/i)).toBeInTheDocument();
    expect(screen.getByText('/tmp/test.tar.zst')).toBeInTheDocument();
  });
});

// ── canceled state ────────────────────────────────────────────────────────────

describe('ReportIssueModal — canceled', () => {
  it('shows "Report Issue canceled" message', () => {
    renderModal({ kind: 'canceled' });
    expect(screen.getByTestId('report-issue-canceled-msg')).toHaveTextContent(
      /Report Issue canceled — no archive produced/i,
    );
  });
});

// ── success state (browser opened) ────────────────────────────────────────────

describe('ReportIssueModal — success (browser opened)', () => {
  const successState: ReportIssueState = {
    kind: 'success',
    archivePath: '/home/user/tuxlink-logs.tar.zst',
    archiveSizeBytes: 2048,
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new/choose',
    browserOpened: true,
    diagnostics: 'Build: tuxlink v0.41.1 (git abc1234, release)\nPlatform: Ubuntu 24.04 · 6.8.0',
  };

  it('renders archive path', () => {
    renderModal(successState);
    expect(screen.getByTestId('report-issue-archive-path')).toHaveTextContent('/home/user/tuxlink-logs.tar.zst');
  });

  it('guides the operator to the Bug report template + Logs field (uhpn)', () => {
    renderModal(successState);
    expect(screen.getByText(/GitHub opened in your browser/i)).toBeInTheDocument();
    expect(screen.getByText(/Bug report/i)).toBeInTheDocument();
  });

  it('Copy diagnostics copies the build/env summary (uhpn)', async () => {
    renderModal(successState);
    fireEvent.click(screen.getByTestId('report-issue-copy-diagnostics-btn'));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(successState.diagnostics);
    });
  });

  it('does NOT show URL textarea when browser opened', () => {
    renderModal(successState);
    expect(screen.queryByTestId('report-issue-url-textarea')).toBeNull();
  });

  it('shows Copy archive path button', () => {
    renderModal(successState);
    expect(screen.getByTestId('report-issue-copy-path-btn')).toBeInTheDocument();
  });

  it('Copy archive path calls clipboard.writeText', async () => {
    renderModal(successState);
    fireEvent.click(screen.getByTestId('report-issue-copy-path-btn'));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(successState.archivePath);
    });
  });
});

// ── success state (no browser) ────────────────────────────────────────────────

describe('ReportIssueModal — success (no browser)', () => {
  const successState: ReportIssueState = {
    kind: 'success',
    archivePath: '/home/user/tuxlink-logs.tar.zst',
    archiveSizeBytes: 4096,
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new/choose',
    browserOpened: false,
    diagnostics: 'Build: tuxlink v0.41.1 (git abc1234, release)\nPlatform: Ubuntu 24.04 · 6.8.0',
  };

  it('renders URL textarea', () => {
    renderModal(successState);
    const ta = screen.getByTestId('report-issue-url-textarea') as HTMLTextAreaElement;
    expect(ta.value).toBe(successState.githubUrl);
  });

  it('shows Copy URL button', () => {
    renderModal(successState);
    expect(screen.getByTestId('report-issue-copy-url-btn')).toBeInTheDocument();
  });

  it('Copy URL calls clipboard.writeText with the URL', async () => {
    renderModal(successState);
    fireEvent.click(screen.getByTestId('report-issue-copy-url-btn'));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(successState.githubUrl);
    });
  });

  it('shows Open in browser button', () => {
    renderModal(successState);
    expect(screen.getByTestId('report-issue-open-browser-btn')).toBeInTheDocument();
  });

  it('Open in browser opens the URL via the Tauri shell plugin, NOT window.open (tuxlink-uxvn)', () => {
    const windowOpenSpy = vi.spyOn(window, 'open');
    renderModal(successState);
    fireEvent.click(screen.getByTestId('report-issue-open-browser-btn'));
    // The external open must go through the shell plugin (real browser), never
    // window.open (which embeds the page inside the WebKitGTK app).
    expect(mockShellOpen).toHaveBeenCalledWith(successState.githubUrl);
    expect(windowOpenSpy).not.toHaveBeenCalled();
  });
});

// ── error state ───────────────────────────────────────────────────────────────

describe('ReportIssueModal — error', () => {
  const errorStateWithUrl: ReportIssueState = {
    kind: 'error',
    message: 'Report Issue failed: disk full',
    archivePath: '/home/user/partial.tar.zst',
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new/choose',
  };

  it('renders error message', () => {
    renderModal(errorStateWithUrl);
    expect(screen.getByTestId('report-issue-error-msg')).toHaveTextContent(/disk full/i);
  });

  it('shows URL textarea', () => {
    renderModal(errorStateWithUrl);
    const ta = screen.getByTestId('report-issue-error-url-textarea') as HTMLTextAreaElement;
    expect(ta.value).toBe(errorStateWithUrl.githubUrl);
  });

  it('Copy URL calls clipboard.writeText', async () => {
    renderModal(errorStateWithUrl);
    fireEvent.click(screen.getByTestId('report-issue-error-copy-url-btn'));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(errorStateWithUrl.githubUrl);
    });
  });

  it('Copy archive path calls clipboard.writeText', async () => {
    renderModal(errorStateWithUrl);
    fireEvent.click(screen.getByTestId('report-issue-error-copy-path-btn'));
    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(errorStateWithUrl.archivePath);
    });
  });

  it('error without URL or path still shows error message + close button', () => {
    renderModal({ kind: 'error', message: 'Export failed: not initialized' });
    expect(screen.getByTestId('report-issue-error-msg')).toBeInTheDocument();
    expect(screen.getByTestId('report-issue-close-btn')).toBeInTheDocument();
    expect(screen.queryByTestId('report-issue-error-url-textarea')).toBeNull();
    expect(screen.queryByTestId('report-issue-error-copy-path-btn')).toBeNull();
  });
});

// ── Close button + Esc ────────────────────────────────────────────────────────

describe('ReportIssueModal — close', () => {
  it('Close button calls onClose', () => {
    const { onClose } = renderModal({ kind: 'canceled' });
    fireEvent.click(screen.getByTestId('report-issue-close-btn'));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('Esc key calls onClose', () => {
    const { onClose } = renderModal({ kind: 'canceled' });
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('clicking backdrop calls onClose', () => {
    const { onClose } = renderModal({ kind: 'canceled' });
    fireEvent.click(screen.getByTestId('report-issue-backdrop'));
    expect(onClose).toHaveBeenCalledOnce();
  });
});

// ── useReportIssueController ────────────────────────────────────────────────────

describe('useReportIssueController — full flow', () => {
  function Harness() {
    const [state, setState] = useState<ReportIssueState>({ kind: 'idle' });
    const controller = useReportIssueController(setState);
    return (
      <div>
        <button data-testid="start-btn" onClick={() => controller.start()}>Start</button>
        <ReportIssueModal state={state} onClose={() => setState({ kind: 'idle' })} onProceed={() => controller.start()} />
      </div>
    );
  }

  it('start() → Save As cancel → state becomes canceled', async () => {
    mockSaveDialog.mockResolvedValue(null);
    render(<Harness />);
    await act(() => { fireEvent.click(screen.getByTestId('start-btn')); });
    await waitFor(() => {
      expect(screen.getByTestId('report-issue-canceled-msg')).toBeInTheDocument();
    });
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('start() → Save As confirms → invoke report_issue_flow → success (browser opened)', async () => {
    mockSaveDialog.mockResolvedValue('/tmp/tuxlink-issue.tar.zst');
    mockInvoke.mockResolvedValue({
      archive_path: '/tmp/tuxlink-issue.tar.zst',
      archive_size_bytes: 8192,
      github_url: 'https://github.com/cameronzucker/tuxlink/issues/new/choose',
      browser_opened: true,
      correlation_id: 'abc123',
      diagnostics: 'Build: tuxlink v0.41.1 (git abc1234, release)',
    });

    render(<Harness />);
    await act(() => { fireEvent.click(screen.getByTestId('start-btn')); });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('report_issue_flow', {
        outputPath: '/tmp/tuxlink-issue.tar.zst',
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId('report-issue-archive-path')).toHaveTextContent(
        '/tmp/tuxlink-issue.tar.zst',
      );
    });
  });

  it('start() → invoke error → error state with message', async () => {
    mockSaveDialog.mockResolvedValue('/tmp/tuxlink-issue.tar.zst');
    mockInvoke.mockRejectedValue('logging not available (degraded)');

    render(<Harness />);
    await act(() => { fireEvent.click(screen.getByTestId('start-btn')); });

    await waitFor(() => {
      expect(screen.getByTestId('report-issue-error-msg')).toHaveTextContent(
        /Report Issue failed/i,
      );
    });
  });
});
