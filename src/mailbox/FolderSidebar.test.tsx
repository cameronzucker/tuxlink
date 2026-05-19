import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FolderSidebar } from './FolderSidebar';

describe('<FolderSidebar>', () => {
  beforeEach(() => {
    // Drafts count reads localStorage; keep it empty/clean per test.
    globalThis.localStorage?.clear?.();
  });

  it('renders the functional folders enabled', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />);
    for (const id of ['inbox', 'outbox', 'sent', 'drafts']) {
      expect(screen.getByTestId(`folder-${id}`)).not.toBeDisabled();
    }
  });

  // Task-12 test (9): Deleted + Templates render as disabled placeholders.
  it('renders Deleted and Templates as disabled placeholders', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-deleted')).toBeDisabled();
    expect(screen.getByTestId('folder-templates')).toBeDisabled();
    expect(screen.getByTestId('folder-deleted')).toHaveAttribute('aria-disabled', 'true');
  });

  it('clicking a functional folder fires onSelectFolder with its id', () => {
    const onSelectFolder = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelectFolder} />);
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(onSelectFolder).toHaveBeenCalledWith('sent');
  });

  it('clicking a disabled folder does NOT fire onSelectFolder', () => {
    const onSelectFolder = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelectFolder} />);
    fireEvent.click(screen.getByTestId('folder-deleted'));
    fireEvent.click(screen.getByTestId('folder-templates'));
    expect(onSelectFolder).not.toHaveBeenCalled();
  });

  it('marks the selected folder with aria-current', () => {
    render(<FolderSidebar selectedFolder="outbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-outbox')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-inbox')).not.toHaveAttribute('aria-current');
  });

  it('renders a count badge for backend folders when count > 0', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        counts={{ inbox: 3, sent: 0 }}
      />,
    );
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('3');
    // Zero count → no badge.
    expect(screen.queryByTestId('folder-count-sent')).not.toBeInTheDocument();
  });
});
