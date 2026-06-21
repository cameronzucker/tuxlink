/**
 * ConfirmPurgeDialog tests (tuxlink-wl7n Task 14).
 *
 * Covers: plural/singular copy, Confirm fires onConfirm, Cancel/Escape/×/backdrop
 * fire onCancel, onConfirm rejection shows the error and keeps the dialog open.
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ConfirmPurgeDialog } from './ConfirmPurgeDialog';

describe('ConfirmPurgeDialog', () => {
  it('renders the singular body copy for count=1', () => {
    render(
      <ConfirmPurgeDialog
        open
        count={1}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('purge-dialog-body')).toHaveTextContent(
      'Permanently delete 1 message? This cannot be undone.',
    );
  });

  it('renders the plural body copy for count=3', () => {
    render(
      <ConfirmPurgeDialog
        open
        count={3}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('purge-dialog-body')).toHaveTextContent(
      'Permanently delete 3 messages? This cannot be undone.',
    );
  });

  it('calls onConfirm when the Delete permanently button is clicked', async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={onConfirm} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByTestId('purge-dialog-confirm'));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledOnce());
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('calls onCancel when the Cancel button is clicked', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByTestId('purge-dialog-cancel'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('calls onCancel when the × close button is clicked', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByTestId('purge-dialog-close'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('calls onCancel on Escape keydown', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('calls onCancel when clicking the backdrop', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    fireEvent.click(screen.getByTestId('purge-dialog-backdrop'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('does NOT call onCancel when clicking inside the dialog (only the backdrop)', () => {
    const onCancel = vi.fn();
    render(
      <ConfirmPurgeDialog open count={2} onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    // Click inside the dialog element itself — should not propagate to backdrop handler
    fireEvent.click(screen.getByTestId('purge-dialog'));
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('shows an error line and keeps the dialog open when onConfirm rejects', async () => {
    const onConfirm = vi.fn().mockRejectedValue(new Error('Purge failed'));
    render(
      <ConfirmPurgeDialog open count={1} onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    fireEvent.click(screen.getByTestId('purge-dialog-confirm'));
    await waitFor(() =>
      expect(screen.getByTestId('purge-dialog-error')).toBeInTheDocument(),
    );
    // Dialog body still visible — not closed
    expect(screen.getByTestId('purge-dialog-body')).toBeInTheDocument();
  });

  it('renders nothing when open=false', () => {
    render(
      <ConfirmPurgeDialog open={false} count={2} onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(screen.queryByTestId('purge-dialog-backdrop')).toBeNull();
  });
});
