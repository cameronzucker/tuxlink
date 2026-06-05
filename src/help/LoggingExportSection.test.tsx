/**
 * Tests for LoggingExportSection.
 *
 * Covers (per plan §10.1.1):
 *   - Status table renders when data is present
 *   - Export button → saveDialog → logging_export invoked with the chosen path
 *   - Export canceled (saveDialog returns null) → shows "Export canceled" feedback
 *   - Open log directory → logging_open_directory invoked
 *   - Clear history → confirm dialog → logging_clear_history invoked
 *
 * tuxlink-qjgx alpha-logging plan Task 7.4.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React from 'react';
import { LoggingExportSection } from './LoggingExportSection';

// --- Mocks ----------------------------------------------------------------
// vi.hoisted ensures the mock fns are created before the vi.mock factories
// run (factories are hoisted to top of file, before any const declarations).

const { mockInvoke, mockSaveDialog } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockSaveDialog: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: mockSaveDialog }));

// --- Helpers --------------------------------------------------------------

const MOCK_STATUS = {
  disk_usage_bytes: 1024 * 1024,       // 1.0 MB
  disk_cap_bytes: 500 * 1024 * 1024,   // 500 MB
  retained_window_seconds: 7 * 86400,  // 7d 0h
  event_rate_per_hour: 42,
  last_export: null,
  detailed_mode: 'off' as const,
  bounded_remaining_seconds: null,
  retention_days: 14,
  retention_mb_cap: 500,
  boot_id_short: 'testboot',
  degraded: null,
};

function renderExport() {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === 'logging_status') return Promise.resolve(MOCK_STATUS);
    return Promise.resolve(null);
  });

  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    React.createElement(QueryClientProvider, { client },
      React.createElement(LoggingExportSection),
    ),
  );
}

// --- Tests ----------------------------------------------------------------

beforeEach(() => {
  vi.resetAllMocks();
  vi.spyOn(window, 'confirm').mockReturnValue(true);
});

describe('LoggingExportSection — status rendering', () => {
  it('renders the Export section heading', async () => {
    renderExport();
    expect(screen.getByRole('heading', { name: /export/i })).toBeInTheDocument();
  });

  it('renders disk usage once status loads', async () => {
    renderExport();
    await waitFor(() => {
      expect(screen.getByText(/1\.0 MB \/ 500\.0 MB/)).toBeInTheDocument();
    });
  });

  it('renders retained window', async () => {
    renderExport();
    await waitFor(() => {
      expect(screen.getByText(/7d 0h/)).toBeInTheDocument();
    });
  });

  it('renders event rate', async () => {
    renderExport();
    await waitFor(() => {
      expect(screen.getByText(/~42\/hour/)).toBeInTheDocument();
    });
  });

  it('renders "(none)" for last export when null', async () => {
    renderExport();
    await waitFor(() => {
      expect(screen.getByText('(none)')).toBeInTheDocument();
    });
  });
});

describe('LoggingExportSection — Export button', () => {
  it('calls saveDialog then logging_export on Export click', async () => {
    mockSaveDialog.mockResolvedValue('/tmp/tuxlink-logs.tar.zst');
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'logging_status') return Promise.resolve(MOCK_STATUS);
      if (cmd === 'logging_export') return Promise.resolve({ archive_size_bytes: 2048, correlation_id: null });
      return Promise.resolve(null);
    });

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      React.createElement(QueryClientProvider, { client },
        React.createElement(LoggingExportSection),
      ),
    );

    const btn = screen.getByRole('button', { name: /export logs/i });
    fireEvent.click(btn);

    await waitFor(() => {
      expect(mockSaveDialog).toHaveBeenCalledOnce();
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_export', { outputPath: '/tmp/tuxlink-logs.tar.zst' });
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent(/saved.*2\.0 KB/i);
    });
  });

  it('saveDialog defaultPath contains attempt-id segment (Amendment H)', async () => {
    mockSaveDialog.mockResolvedValue('/tmp/tuxlink-logs.tar.zst');
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'logging_status') return Promise.resolve(MOCK_STATUS);
      if (cmd === 'logging_export') return Promise.resolve({ archive_size_bytes: 2048, correlation_id: null });
      return Promise.resolve(null);
    });

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      React.createElement(QueryClientProvider, { client },
        React.createElement(LoggingExportSection),
      ),
    );

    // Wait for status to load so boot_id_short is available
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith('logging_status'));

    const btn = screen.getByRole('button', { name: /export logs/i });
    fireEvent.click(btn);

    await waitFor(() => {
      expect(mockSaveDialog).toHaveBeenCalledOnce();
    });

    const callArgs = mockSaveDialog.mock.calls[0][0] as { defaultPath: string };
    expect(callArgs.defaultPath).toMatch(/tuxlink-logs-.+-(boot-testboot|[a-z0-9-]+)\.tar\.zst/);
  });

  it('shows "Export canceled." when saveDialog returns null', async () => {
    mockSaveDialog.mockResolvedValue(null);
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'logging_status') return Promise.resolve(MOCK_STATUS);
      return Promise.resolve(null);
    });

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      React.createElement(QueryClientProvider, { client },
        React.createElement(LoggingExportSection),
      ),
    );

    const btn = screen.getByRole('button', { name: /export logs/i });
    fireEvent.click(btn);

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Export canceled.');
    });
    // logging_export must NOT be called
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_export', expect.anything());
  });
});

describe('LoggingExportSection — Open log directory', () => {
  it('invokes logging_open_directory on click', async () => {
    renderExport();
    const btn = screen.getByRole('button', { name: /open log directory/i });
    fireEvent.click(btn);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_open_directory');
    });
  });
});

describe('LoggingExportSection — Clear history', () => {
  it('confirms then invokes logging_clear_history', async () => {
    renderExport();
    const btn = screen.getByRole('button', { name: /clear history/i });
    fireEvent.click(btn);

    expect(window.confirm).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('logging_clear_history');
    });
    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('History cleared.');
    });
  });

  it('does NOT invoke logging_clear_history when confirm is canceled', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    renderExport();
    const btn = screen.getByRole('button', { name: /clear history/i });
    fireEvent.click(btn);

    expect(window.confirm).toHaveBeenCalledOnce();
    expect(mockInvoke).not.toHaveBeenCalledWith('logging_clear_history');
  });
});
