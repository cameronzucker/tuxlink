/**
 * FolderContextMenu — right-click context menu on a user folder (tuxlink-ejph;
 * styling refactored under tuxlink-i2nr).
 *
 * Mirrors MessageContextMenu's interaction model and shares the `.tux-ctx-*`
 * styling for visual consistency across the project's inline menus.
 */

import { useEffect, useRef } from 'react';
import type { UserFolder } from './types';
import './userFolders.css';

export interface FolderContextMenuProps {
  folder: UserFolder;
  x: number;
  y: number;
  onRename: () => void;
  onDelete: () => void;
  onClose: () => void;
}

export function FolderContextMenu({
  folder,
  x,
  y,
  onRename,
  onDelete,
  onClose,
}: FolderContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  useEffect(() => {
    function onMouseDown(e: MouseEvent) {
      const node = ref.current;
      if (node && !node.contains(e.target as Node)) onClose();
    }
    document.addEventListener('mousedown', onMouseDown);
    return () => document.removeEventListener('mousedown', onMouseDown);
  }, [onClose]);

  const MENU_W = 180;
  const MENU_H = 160;
  const left = Math.min(x, window.innerWidth - MENU_W - 4);
  const top = Math.min(y, window.innerHeight - MENU_H - 4);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Folder actions"
      data-testid="folder-context-menu"
      className="tux-ctx-menu"
      style={{ position: 'fixed', left, top, minWidth: MENU_W }}
    >
      <div className="tux-ctx-label">{folder.displayName}</div>
      <button
        type="button"
        role="menuitem"
        data-testid="folder-ctx-rename"
        onClick={() => {
          onRename();
          onClose();
        }}
        className="tux-ctx-item"
      >
        Rename…
      </button>
      <div className="tux-ctx-separator" />
      <button
        type="button"
        role="menuitem"
        data-testid="folder-ctx-delete"
        onClick={() => {
          onDelete();
          onClose();
        }}
        className="tux-ctx-item tux-ctx-item-danger"
      >
        Delete folder…
      </button>
    </div>
  );
}
