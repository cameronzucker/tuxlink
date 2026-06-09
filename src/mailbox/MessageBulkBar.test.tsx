import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MessageBulkBar } from './MessageBulkBar';

describe('MessageBulkBar', () => {
  it('renders the count and fires read/unread/clear callbacks', () => {
    const onMarkRead = vi.fn(), onMarkUnread = vi.fn(), onClear = vi.fn();
    render(<MessageBulkBar count={3} onMarkRead={onMarkRead} onMarkUnread={onMarkUnread} onClear={onClear} />);
    expect(screen.getByText(/3 selected/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /mark read/i })); expect(onMarkRead).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /mark unread/i })); expect(onMarkUnread).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /clear/i })); expect(onClear).toHaveBeenCalled();
  });
});
