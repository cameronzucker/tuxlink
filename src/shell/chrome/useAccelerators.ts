import { useEffect } from 'react';
import { ACCELERATORS, type MenuActionId } from './menuModel';

interface KeyState {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/** Pure matcher: a key event → the bound action id, or null. CmdOrCtrl = Ctrl|Meta. */
export function matchAccelerator(e: KeyState): MenuActionId | null {
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  for (const a of ACCELERATORS) {
    if (a.ctrl === ctrl && a.shift === e.shiftKey && a.key.toLowerCase() === key) {
      return a.id;
    }
  }
  return null;
}

/**
 * Install the main-window keyboard accelerators (tuxlink-ng3). On a matching
 * combo, prevents the browser default and calls `onAction(id)`. Lives on the
 * main window only; the compose window keeps its own Ctrl+S / Ctrl+Enter.
 */
export function useAccelerators(onAction: (id: MenuActionId) => void): void {
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const id = matchAccelerator(e);
      if (id) {
        e.preventDefault();
        onAction(id);
      }
    }
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onAction]);
}
