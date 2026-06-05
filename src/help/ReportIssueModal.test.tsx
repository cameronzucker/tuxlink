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
 *   - Open in browser calls window.open
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

const { mockInvoke, mockSaveDialog } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockSaveDialog: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mockSaveDialog }));

beforeEach(() => {
  vi.resetAllMocks();
  Object.defineProperty(navigator, 'clipboard', {
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
    configurable: true,
    writable: true,
  });
  vi.spyOn(window, 'open').mockReturnValue(null);
});

// ── Render helpers ────────────────────────────────────────────────────────────

function renderModal(initialState: ReportIssueState) {
  let externalSetState: React.Dispatch<React.SetStateAction<ReportIssueState>>;
  const onClose = vi.fn();

  function Wrapper() {
    const [state, setState] = useState<ReportIssueState>(initialState);
    externalSetState = setState;
    return <ReportIssueModal state={state} setState={setState} onClose={onClose} />;
  }

  const utils = render(<Wrapper />);
  return { ...utils, onClose, getState: () => undefined, setState: (s: ReportIssueState) => externalSetState(s) };
}

// ── Idle state ────────────────────────────────────────────────────────────────

describe('ReportIssueModal — idle', () => {
  it('renders nothing when state is idle', () => {
    renderModal({ kind: 'idle' });
    expect(screen.queryByTestId('report-issue-backdrop')).toBeNull();
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
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new?labels=alpha-report&body=test',
    browserOpened: true,
  };

  it('renders archive path', () => {
    renderModal(successState);
    expect(screen.getByTestId('report-issue-archive-path')).toHaveTextContent('/home/user/tuxlink-logs.tar.zst');
  });

  it('shows "Opened GitHub Issues in your browser" message', () => {
    renderModal(successState);
    expect(screen.getByText(/GitHub Issues opened in your browser/i)).toBeInTheDocument();
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
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new?labels=alpha-report&body=test',
    browserOpened: false,
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

  it('Open in browser calls window.open with the URL', () => {
    renderModal(successState);
    fireEvent.click(screen.getByTestId('report-issue-open-browser-btn'));
    expect(window.open).toHaveBeenCalledWith(successState.githubUrl, '_blank');
  });
});

// ── error state ───────────────────────────────────────────────────────────────

describe('ReportIssueModal — error', () => {
  const errorStateWithUrl: ReportIssueState = {
    kind: 'error',
    message: 'Report Issue failed: disk full',
    archivePath: '/home/user/partial.tar.zst',
    githubUrl: 'https://github.com/cameronzucker/tuxlink/issues/new?labels=alpha-report&body=test',
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
        <ReportIssueModal state={state} setState={setState} onClose={() => setState({ kind: 'idle' })} />
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
      github_url: 'https://github.com/cameronzucker/tuxlink/issues/new?labels=alpha-report&body=x',
      browser_opened: true,
      correlation_id: 'abc123',
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
