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

describe('<MessageContextMenu> — selection mode (tuxlink-l80q)', () => {
  it('renders an N-messages header, an "acting on N" footer, and both read items', () => {
    const onSetReadState = vi.fn();
    render(
      <MessageContextMenu
        message={baseMsg}
        folder="inbox"
        x={0}
        y={0}
        userFolders={[]}
        selectionCount={3}
        onSetReadState={onSetReadState}
        onMoveTo={vi.fn()}
        onArchive={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId('ctx-selection-header')).toHaveTextContent('3 messages');
    expect(screen.getByTestId('ctx-msg-id')).toHaveTextContent(/acting on 3 selected messages/i);
    // The single-message subject footer must NOT appear in selection mode.
    expect(screen.queryByText(baseMsg.subject)).toBeNull();

    fireEvent.click(screen.getByRole('menuitem', { name: /mark as read/i }));
    expect(onSetReadState).toHaveBeenCalledWith(true);
    fireEvent.click(screen.getByRole('menuitem', { name: /mark as unread/i }));
    expect(onSetReadState).toHaveBeenCalledWith(false);
  });

  it('fires bulk move + archive through the same onMoveTo/onArchive callbacks', () => {
    const onMoveTo = vi.fn();
    const onArchive = vi.fn();
    render(
      <MessageContextMenu
        message={baseMsg}
        folder="inbox"
        x={0}
        y={0}
        userFolders={[]}
        selectionCount={2}
        onSetReadState={vi.fn()}
        onMoveTo={onMoveTo}
        onArchive={onArchive}
        onClose={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('ctx-move-sent'));
    expect(onMoveTo).toHaveBeenCalledWith('sent');
    fireEvent.click(screen.getByTestId('ctx-archive'));
    expect(onArchive).toHaveBeenCalled();
  });

  it('single-target mode (no selectionCount) keeps the subject footer and toggle', () => {
    render(
      <MessageContextMenu
        message={{ ...baseMsg, unread: true }}
        folder="inbox"
        x={0}
        y={0}
        userFolders={[]}
        onSetReadState={vi.fn()}
        onMoveTo={vi.fn()}
        onArchive={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByTestId('ctx-selection-header')).toBeNull();
    expect(screen.getByTestId('ctx-msg-id')).toHaveTextContent(baseMsg.subject);
    // Single toggle, not two separate items.
    expect(screen.getByRole('menuitem', { name: /mark as read/i })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /mark as unread/i })).toBeNull();
  });
});
