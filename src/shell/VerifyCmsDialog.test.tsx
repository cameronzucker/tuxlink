import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { VerifyCmsDialog } from './VerifyCmsDialog';

// The dialog auto-runs verify_cms_connection on open; drive it through the
// mocked Tauri invoke.
const invokeMock = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

describe('VerifyCmsDialog', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('renders nothing when closed', () => {
    invokeMock.mockResolvedValue(undefined);
    render(<VerifyCmsDialog open={false} onClose={vi.fn()} />);
    expect(screen.queryByTestId('verify-cms-panel')).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('probes on open and shows the connecting state', () => {
    // Never resolves → stays in the probing substate.
    invokeMock.mockReturnValue(new Promise<void>(() => {}));
    render(<VerifyCmsDialog open={true} onClose={vi.fn()} />);
    expect(screen.getByTestId('verify-cms-probing')).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith('verify_cms_connection');
  });

  it('shows the verified state when the probe resolves', async () => {
    invokeMock.mockResolvedValue(undefined);
    render(<VerifyCmsDialog open={true} onClose={vi.fn()} />);
    expect(await screen.findByTestId('verify-cms-ok')).toBeInTheDocument();
    // The action button reads "Done" on success.
    expect(screen.getByTestId('verify-cms-done')).toHaveTextContent('Done');
  });

  it('shows the error state with the detail when the probe rejects', async () => {
    invokeMock.mockRejectedValue({ kind: 'Other', detail: 'no route to host' });
    render(<VerifyCmsDialog open={true} onClose={vi.fn()} />);
    expect(await screen.findByTestId('verify-cms-error')).toBeInTheDocument();
    expect(screen.getByTestId('verify-cms-error-detail')).toHaveTextContent('no route to host');
  });

  it('treats a Busy rejection as "already in progress"', async () => {
    invokeMock.mockRejectedValue({ kind: 'Busy' });
    render(<VerifyCmsDialog open={true} onClose={vi.fn()} />);
    expect(await screen.findByTestId('verify-cms-error-detail')).toHaveTextContent(
      /already in progress/i,
    );
  });

  it('retries the probe from the error state', async () => {
    invokeMock.mockRejectedValueOnce({ kind: 'Other', detail: 'first fail' });
    invokeMock.mockResolvedValueOnce(undefined);
    render(<VerifyCmsDialog open={true} onClose={vi.fn()} />);
    fireEvent.click(await screen.findByTestId('verify-cms-retry'));
    expect(await screen.findByTestId('verify-cms-ok')).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(2);
  });

  it('closes via the close button', async () => {
    invokeMock.mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<VerifyCmsDialog open={true} onClose={onClose} />);
    await waitFor(() => expect(screen.getByTestId('verify-cms-ok')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('verify-cms-done'));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it('closes on Escape and on backdrop click', async () => {
    invokeMock.mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(<VerifyCmsDialog open={true} onClose={onClose} />);
    await screen.findByTestId('verify-cms-ok');
    fireEvent.keyDown(document, { key: 'Escape' });
    fireEvent.click(screen.getByTestId('verify-cms-backdrop'));
    expect(onClose).toHaveBeenCalled();
  });
});
