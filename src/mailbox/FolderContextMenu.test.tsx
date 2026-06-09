import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FolderContextMenu } from './FolderContextMenu';
import type { UserFolder } from './types';

const FOLDERS: UserFolder[] = [
  { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
  { slug: 'weather', displayName: 'Weather', createdAt: 'b' },
  { slug: 'ares', displayName: 'ARES', createdAt: 'c', parentSlug: 'nets' },
];

function handlers() {
  return {
    onRename: vi.fn(),
    onDelete: vi.fn(),
    onNewSubfolder: vi.fn(),
    onMoveTo: vi.fn(),
    onClose: vi.fn(),
  };
}

describe('FolderContextMenu — nesting actions (tuxlink-ka3z)', () => {
  it('shows "New subfolder here" on a top-level folder, hides it on a subfolder', () => {
    const h = handlers();
    const { rerender } = render(
      <FolderContextMenu folder={FOLDERS[0]} allFolders={FOLDERS} x={0} y={0} {...h} />,
    );
    expect(screen.getByTestId('folder-ctx-new-subfolder')).toBeInTheDocument();
    rerender(<FolderContextMenu folder={FOLDERS[2]} allFolders={FOLDERS} x={0} y={0} {...h} />);
    expect(screen.queryByTestId('folder-ctx-new-subfolder')).toBeNull();
  });

  it('lists valid parents and excludes self, current parent, and subfolders', () => {
    // Right-click 'weather' (top-level, no children): valid target = nets only;
    // no "Top level" (already top), self excluded, subfolder ares excluded.
    render(<FolderContextMenu folder={FOLDERS[1]} allFolders={FOLDERS} x={0} y={0} {...handlers()} />);
    expect(screen.getByTestId('folder-move-nets')).toBeInTheDocument();
    expect(screen.queryByTestId('folder-move-weather')).toBeNull();
    expect(screen.queryByTestId('folder-move-ares')).toBeNull();
    expect(screen.queryByTestId('folder-move-top')).toBeNull();
  });

  it('offers "Top level" for a subfolder and excludes its current parent', () => {
    render(<FolderContextMenu folder={FOLDERS[2]} allFolders={FOLDERS} x={0} y={0} {...handlers()} />);
    expect(screen.getByTestId('folder-move-top')).toBeInTheDocument();
    // current parent (nets) excluded; the other top-level (weather) offered.
    expect(screen.queryByTestId('folder-move-nets')).toBeNull();
    expect(screen.getByTestId('folder-move-weather')).toBeInTheDocument();
  });

  it('a folder with children cannot be nested — shows the blocked hint', () => {
    // 'nets' has child 'ares' → no move targets, blocked hint instead.
    render(<FolderContextMenu folder={FOLDERS[0]} allFolders={FOLDERS} x={0} y={0} {...handlers()} />);
    expect(screen.getByTestId('folder-move-blocked')).toBeInTheDocument();
    expect(screen.queryByTestId('folder-move-weather')).toBeNull();
  });

  it('clicking a move target fires onMoveTo with the target slug', () => {
    const h = handlers();
    render(<FolderContextMenu folder={FOLDERS[1]} allFolders={FOLDERS} x={0} y={0} {...h} />);
    fireEvent.click(screen.getByTestId('folder-move-nets'));
    expect(h.onMoveTo).toHaveBeenCalledWith('nets');
    expect(h.onClose).toHaveBeenCalled();
  });

  it('clicking "Top level" fires onMoveTo(undefined)', () => {
    const h = handlers();
    render(<FolderContextMenu folder={FOLDERS[2]} allFolders={FOLDERS} x={0} y={0} {...h} />);
    fireEvent.click(screen.getByTestId('folder-move-top'));
    expect(h.onMoveTo).toHaveBeenCalledWith(undefined);
  });

  it('clicking "New subfolder here" fires onNewSubfolder', () => {
    const h = handlers();
    render(<FolderContextMenu folder={FOLDERS[0]} allFolders={FOLDERS} x={0} y={0} {...h} />);
    fireEvent.click(screen.getByTestId('folder-ctx-new-subfolder'));
    expect(h.onNewSubfolder).toHaveBeenCalled();
  });
});
