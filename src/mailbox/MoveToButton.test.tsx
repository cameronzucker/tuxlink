import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MoveToButton } from './MoveToButton';
import type { UserFolder } from './types';

const folders: UserFolder[] = [
  { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-02T12:00:00Z' },
  { slug: 'disaster-prep', displayName: 'Disaster Prep', createdAt: '2026-06-02T13:00:00Z' },
];

function open() {
  // Radix DropdownMenu opens on pointerDown (button 0), not click.
  fireEvent.pointerDown(screen.getByTestId('move-to-btn'), { button: 0 });
}

describe('<MoveToButton>', () => {
  it('renders the Move trigger button', () => {
    render(<MoveToButton currentFolder="inbox" userFolders={[]} onMove={vi.fn()} />);
    expect(screen.getByTestId('move-to-btn')).toBeInTheDocument();
  });

  it('opens with system destinations (Inbox / Sent / Archive)', () => {
    render(<MoveToButton currentFolder="inbox" userFolders={[]} onMove={vi.fn()} />);
    open();
    expect(screen.getByTestId('move-to-inbox')).toBeInTheDocument();
    expect(screen.getByTestId('move-to-sent')).toBeInTheDocument();
    expect(screen.getByTestId('move-to-archive')).toBeInTheDocument();
  });

  it('disables the row corresponding to the current folder', () => {
    render(<MoveToButton currentFolder="inbox" userFolders={[]} onMove={vi.fn()} />);
    open();
    expect(screen.getByTestId('move-to-inbox')).toHaveAttribute('aria-disabled', 'true');
    expect(screen.getByTestId('move-to-archive')).not.toHaveAttribute('aria-disabled', 'true');
  });

  it('renders user folders in a Folders section when supplied', () => {
    render(<MoveToButton currentFolder="inbox" userFolders={folders} onMove={vi.fn()} />);
    open();
    expect(screen.getByTestId('move-to-ares-drills')).toBeInTheDocument();
    expect(screen.getByTestId('move-to-disaster-prep')).toBeInTheDocument();
    expect(screen.getByText('Folders')).toBeInTheDocument();
  });

  it('omits the user-folders section when none exist', () => {
    render(<MoveToButton currentFolder="inbox" userFolders={[]} onMove={vi.fn()} />);
    open();
    expect(screen.queryByText('Folders')).toBeNull();
  });

  it('fires onMove with the chosen folder', () => {
    const onMove = vi.fn();
    render(<MoveToButton currentFolder="inbox" userFolders={folders} onMove={onMove} />);
    open();
    fireEvent.click(screen.getByTestId('move-to-ares-drills'));
    expect(onMove).toHaveBeenCalledWith('ares-drills');
  });

  it('does NOT fire onMove for the disabled current-folder row', () => {
    const onMove = vi.fn();
    render(<MoveToButton currentFolder="archive" userFolders={[]} onMove={onMove} />);
    open();
    fireEvent.click(screen.getByTestId('move-to-archive'));
    expect(onMove).not.toHaveBeenCalled();
  });
});
