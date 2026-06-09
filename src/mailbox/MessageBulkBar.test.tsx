import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MessageBulkBar } from './MessageBulkBar';
import type { UserFolder } from './types';

const USER_FOLDERS: UserFolder[] = [
  { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-09T00:00:00Z' },
];

function renderBar(over: Partial<Parameters<typeof MessageBulkBar>[0]> = {}) {
  const props = {
    count: 3,
    currentFolder: 'inbox' as const,
    userFolders: USER_FOLDERS,
    onMarkRead: vi.fn(),
    onMarkUnread: vi.fn(),
    onArchive: vi.fn(),
    onMove: vi.fn(),
    onClear: vi.fn(),
    ...over,
  };
  render(<MessageBulkBar {...props} />);
  return props;
}

describe('MessageBulkBar', () => {
  it('renders the count and fires read/unread/clear callbacks', () => {
    const props = renderBar();
    expect(screen.getByText(/3 selected/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /mark read/i }));
    expect(props.onMarkRead).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /mark unread/i }));
    expect(props.onMarkUnread).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /clear selection/i }));
    expect(props.onClear).toHaveBeenCalled();
  });

  it('fires onArchive from the Archive button (tuxlink-l80q)', () => {
    const props = renderBar();
    fireEvent.click(screen.getByRole('button', { name: /^archive$/i }));
    expect(props.onArchive).toHaveBeenCalled();
  });

  it('disables Archive when the current folder is already Archive', () => {
    renderBar({ currentFolder: 'archive' });
    expect(screen.getByRole('button', { name: /^archive$/i })).toBeDisabled();
  });

  it('exposes a Move ▾ dropdown that fires onMove with the chosen destination', () => {
    const props = renderBar();
    // Open the reused MoveToButton dropdown, then pick Sent.
    fireEvent.pointerDown(screen.getByTestId('move-to-btn'), { button: 0 });
    fireEvent.click(screen.getByTestId('move-to-sent'));
    expect(props.onMove).toHaveBeenCalledWith('sent');
  });

  it('disables the current folder in the Move dropdown', () => {
    renderBar({ currentFolder: 'inbox' });
    fireEvent.pointerDown(screen.getByTestId('move-to-btn'), { button: 0 });
    expect(screen.getByTestId('move-to-inbox')).toHaveAttribute('aria-disabled', 'true');
  });
});
