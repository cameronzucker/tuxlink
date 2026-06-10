import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { UninstallCleanupDialog } from './UninstallCleanupDialog';

const { mockInvoke } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mockInvoke }));

function report(mode: 'keep' | 'transient' | 'full', dryRun: boolean) {
  return {
    mode,
    dry_run: dryRun,
    paths: mode === 'keep'
      ? []
      : [
          {
            path: '/home/operator/.local/state/tuxlink/logs',
            outcome: dryRun ? 'WouldRemove' : 'Removed',
          },
        ],
    keyring: mode === 'full'
      ? [{ service: 'tuxlink', account: 'W4PHS', outcome: dryRun ? 'WouldRemove' : 'Removed' }]
      : [],
    warnings: mode === 'full' ? ['Secret Service credentials cannot be enumerated service-wide.'] : [],
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  mockInvoke.mockImplementation((command: string, args: { mode: 'keep' | 'transient' | 'full' }) => {
    if (command === 'uninstall_cleanup_preview') {
      return Promise.resolve(report(args.mode, true));
    }
    if (command === 'uninstall_cleanup_execute') {
      return Promise.resolve(report(args.mode, false));
    }
    return Promise.reject(new Error(`unexpected command ${command}`));
  });
});

describe('UninstallCleanupDialog', () => {
  it('renders nothing when closed', () => {
    render(<UninstallCleanupDialog open={false} onClose={vi.fn()} />);
    expect(screen.queryByTestId('uninstall-cleanup-panel')).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('previews transient cleanup by default and explains apt removal data retention', async () => {
    render(<UninstallCleanupDialog open={true} onClose={vi.fn()} />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('uninstall_cleanup_preview', { mode: 'transient' });
    });

    expect(screen.getByText(/sudo apt remove tuxlink/i)).toBeInTheDocument();
    expect(screen.getByText('tuxlink cleanup --transient --dry-run')).toBeInTheDocument();
    expect(await screen.findByTestId('uninstall-cleanup-report')).toHaveTextContent(/would remove/i);
  });

  it('refreshes the preview when the cleanup mode changes', async () => {
    render(<UninstallCleanupDialog open={true} onClose={vi.fn()} />);
    await screen.findByTestId('uninstall-cleanup-report');

    fireEvent.click(screen.getByLabelText(/Remove all operator data/i));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('uninstall_cleanup_preview', { mode: 'full' });
    });
    expect(screen.getByText('tuxlink cleanup --all')).toBeInTheDocument();
  });

  it('requires transient confirmation before executing cleanup', async () => {
    render(<UninstallCleanupDialog open={true} onClose={vi.fn()} />);
    await screen.findByTestId('uninstall-cleanup-report');

    const execute = screen.getByTestId('uninstall-cleanup-execute');
    expect(execute).toBeDisabled();

    fireEvent.click(screen.getByLabelText(/I understand this will remove transient/i));
    expect(execute).not.toBeDisabled();

    fireEvent.click(execute);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('uninstall_cleanup_execute', { mode: 'transient' });
    });
    expect(await screen.findByTestId('uninstall-cleanup-success')).toHaveTextContent(/Cleanup finished/i);
  });

  it('requires DELETE before executing full cleanup', async () => {
    render(<UninstallCleanupDialog open={true} onClose={vi.fn()} />);
    await screen.findByTestId('uninstall-cleanup-report');

    fireEvent.click(screen.getByLabelText(/Remove all operator data/i));
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('uninstall_cleanup_preview', { mode: 'full' });
    });

    const execute = screen.getByTestId('uninstall-cleanup-execute');
    expect(execute).toBeDisabled();

    fireEvent.change(screen.getByTestId('uninstall-cleanup-delete-confirm'), {
      target: { value: 'DELETE' },
    });
    expect(execute).not.toBeDisabled();

    fireEvent.click(execute);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('uninstall_cleanup_execute', { mode: 'full' });
    });
  });
});
