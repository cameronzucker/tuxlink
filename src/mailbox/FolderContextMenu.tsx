/**
 * FolderContextMenu — right-click context menu on a user folder (tuxlink-ejph,
 * spec §6).
 *
 * Renders as a positioned overlay at the right-click coordinates. Currently
 * offers Rename + Delete; future entries (Mark all read, Empty folder) drop
 * in here once their backend commands land.
 *
 * Mirrors the MessageContextMenu interaction model: Esc / outside-click
 * close, action click closes-after-fire.
 */

import { useEffect, useRef } from 'react';
import type { UserFolder } from './types';

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
      style={{
        position: 'fixed',
        left,
        top,
        minWidth: MENU_W,
        background: 'var(--elevated, #1e2832)',
        border: '1px solid var(--border-strong, #2c3744)',
        borderRadius: 6,
        padding: '4px 0',
        fontSize: 12,
        color: 'var(--text, #e4ebf2)',
        boxShadow: '0 6px 20px rgba(0, 0, 0, 0.6)',
        zIndex: 200,
      }}
    >
      <div
        style={{
          padding: '6px 14px 4px',
          fontSize: 10,
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          color: 'var(--text-faint, #5d6975)',
        }}
      >
        {folder.displayName}
      </div>
      <button
        type="button"
        role="menuitem"
        data-testid="folder-ctx-rename"
        onClick={() => {
          onRename();
          onClose();
        }}
        style={itemStyle}
      >
        Rename…
      </button>
      <div style={separatorStyle} />
      <button
        type="button"
        role="menuitem"
        data-testid="folder-ctx-delete"
        onClick={() => {
          onDelete();
          onClose();
        }}
        style={{ ...itemStyle, color: 'var(--error, #ee6b6b)' }}
      >
        Delete folder…
      </button>
    </div>
  );
}

const itemStyle: React.CSSProperties = {
  display: 'block',
  width: '100%',
  textAlign: 'left',
  padding: '6px 14px',
  background: 'transparent',
  border: 'none',
  color: 'inherit',
  fontSize: 12,
  fontFamily: 'inherit',
  cursor: 'pointer',
};

const separatorStyle: React.CSSProperties = {
  height: 1,
  background: 'var(--border, #1f2832)',
  margin: '4px 0',
};
