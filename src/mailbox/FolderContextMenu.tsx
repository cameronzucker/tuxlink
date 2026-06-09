/**
 * FolderContextMenu — right-click context menu on a user folder (tuxlink-ejph;
 * styling refactored under tuxlink-i2nr; nesting actions added tuxlink-ka3z).
 *
 * Mirrors MessageContextMenu's interaction model and shares the `.tux-ctx-*`
 * styling for visual consistency across the project's inline menus.
 *
 * Nesting (tuxlink-ka3z): a top-level folder offers "New subfolder here". Any
 * folder offers a "Move to" section listing valid re-parent targets (spec D4):
 * top-level folders other than itself and its current parent, plus "Top level"
 * when the folder is currently a subfolder. A folder that itself has children
 * cannot be nested (that would exceed the 2-level cap) — the section shows a
 * disabled hint rather than silently omitting the option.
 */

import { useEffect, useRef } from 'react';
import type { UserFolder } from './types';
import './userFolders.css';

export interface FolderContextMenuProps {
  folder: UserFolder;
  /// The full user-folder list, used to compute valid re-parent targets.
  allFolders: UserFolder[];
  x: number;
  y: number;
  onRename: () => void;
  onDelete: () => void;
  /// Create a subfolder under this folder. Only shown for top-level folders.
  onNewSubfolder: () => void;
  /// Re-parent this folder. `parentSlug === undefined` promotes to top level.
  onMoveTo: (parentSlug: string | undefined) => void;
  onClose: () => void;
}

export function FolderContextMenu({
  folder,
  allFolders,
  x,
  y,
  onRename,
  onDelete,
  onNewSubfolder,
  onMoveTo,
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

  const isTopLevel = !folder.parentSlug;
  const hasChildren = allFolders.some((f) => f.parentSlug === folder.slug);
  // Valid re-parent targets (spec D4): existing top-level folders, excluding
  // this folder and its current parent. A folder with children can't be nested.
  const moveTargets = hasChildren
    ? []
    : allFolders.filter(
        (f) => !f.parentSlug && f.slug !== folder.slug && f.slug !== folder.parentSlug,
      );
  const canPromote = !isTopLevel;
  const showMoveSection = hasChildren || canPromote || moveTargets.length > 0;

  const MENU_W = 200;
  const MENU_H = 240;
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

      {isTopLevel && (
        <button
          type="button"
          role="menuitem"
          data-testid="folder-ctx-new-subfolder"
          onClick={() => {
            onNewSubfolder();
            onClose();
          }}
          className="tux-ctx-item"
        >
          New subfolder here…
        </button>
      )}

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

      {showMoveSection && (
        <>
          <div className="tux-ctx-separator" />
          <div className="tux-ctx-label">Move to</div>
          {canPromote && (
            <button
              type="button"
              role="menuitem"
              data-testid="folder-move-top"
              onClick={() => {
                onMoveTo(undefined);
                onClose();
              }}
              className="tux-ctx-item"
            >
              Top level
            </button>
          )}
          {moveTargets.map((target) => (
            <button
              type="button"
              role="menuitem"
              key={target.slug}
              data-testid={`folder-move-${target.slug}`}
              onClick={() => {
                onMoveTo(target.slug);
                onClose();
              }}
              className="tux-ctx-item"
            >
              {target.displayName}
            </button>
          ))}
          {hasChildren && (
            <div className="tux-ctx-item tux-ctx-item-disabled" data-testid="folder-move-blocked">
              Move its subfolders out first
            </div>
          )}
        </>
      )}

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
