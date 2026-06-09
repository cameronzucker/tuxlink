/**
 * Tests for MessageContextMenu — read/unread affordance (tuxlink-etxt Task 12).
 */
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import type { MessageMeta } from './types';
import { MessageContextMenu } from './MessageContextMenu';

const baseMsg: MessageMeta = {
  id: 'MID1',
  subject: 'Net check-in',
  from: 'W4PHS@winlink.org',
  to: ['KK4XYZ@winlink.org'],
  date: '2026-05-19T14:05:00Z',
  unread: false,
  bodySize: 1024,
  hasAttachments: false,
};

describe('<MessageContextMenu> — Mark read/unread (tuxlink-etxt Task 12)', () => {
  it('offers Mark as read for an unread message and Mark as unread for a read one', () => {
    const onSetReadState = vi.fn();
    const unread = { ...baseMsg, unread: true };
    const { rerender } = render(
      <MessageContextMenu
        message={unread}
        folder="inbox"
        x={0}
        y={0}
        userFolders={[]}
        onSetReadState={onSetReadState}
        onMoveTo={vi.fn()}
        onArchive={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('menuitem', { name: /mark as read/i }));
    expect(onSetReadState).toHaveBeenCalledWith(true);

    rerender(
      <MessageContextMenu
        message={{ ...baseMsg, unread: false }}
        folder="inbox"
        x={0}
        y={0}
        userFolders={[]}
        onSetReadState={onSetReadState}
        onMoveTo={vi.fn()}
        onArchive={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole('menuitem', { name: /mark as unread/i })).toBeInTheDocument();
  });

  it('omits the read/unread item for folders without read-state', () => {
    render(
      <MessageContextMenu
        message={{ ...baseMsg, unread: false }}
        folder="sent"
        x={0}
        y={0}
        userFolders={[]}
        onSetReadState={vi.fn()}
        onMoveTo={vi.fn()}
        onArchive={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByRole('menuitem', { name: /mark as (read|unread)/i })).toBeNull();
  });
});
