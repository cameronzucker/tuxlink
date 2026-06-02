import { useEffect } from 'react';
import { ACCELERATORS, type MenuActionId } from './menuModel';

interface KeyState {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
}

/** Pure matcher: a key event → the bound action id, or null. CmdOrCtrl = Ctrl|Meta.
 *
 * `inTextInput` (default false) is true when a text input / textarea /
 * contenteditable element is currently focused. Accelerators with
 * `suppressInTextInput: true` (e.g. plain `A` for Archive) are skipped in
 * that case so typing the letter doesn't trigger the action. (tuxlink-ca5x) */
export function matchAccelerator(e: KeyState, inTextInput: boolean = false): MenuActionId | null {
  const ctrl = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  for (const a of ACCELERATORS) {
    if (a.ctrl === ctrl && a.shift === e.shiftKey && a.key.toLowerCase() === key) {
      if (a.suppressInTextInput && inTextInput) continue;
      return a.id;
    }
  }
  return null;
}

/** True when the focused element accepts plain-text typing — INPUT (text-ish),
 *  TEXTAREA, or [contenteditable]. Used to gate plain-letter accelerators
 *  (tuxlink-ca5x). Conservative: most INPUT types are treated as text; a few
 *  non-text variants (checkbox, radio, button, submit, range) opt out. */
export function isTextInputFocused(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || !el.tagName) return false;
  // `isContentEditable` is the modern check, but jsdom doesn't implement the
  // computed property; the attribute fallback handles both jsdom + browsers
  // that haven't set the bit yet.
  if (el.isContentEditable) return true;
  const ce = el.getAttribute && el.getAttribute('contenteditable');
  if (ce !== null && ce !== undefined && ce !== 'false' && ce !== 'inherit') return true;
  const tag = el.tagName;
  if (tag === 'TEXTAREA') return true;
  if (tag === 'INPUT') {
    const type = ((el as HTMLInputElement).type || 'text').toLowerCase();
    const nonText = new Set([
      'button', 'submit', 'reset', 'checkbox', 'radio',
      'file', 'range', 'color', 'image', 'hidden',
    ]);
    return !nonText.has(type);
  }
  return false;
}

/**
 * Install the main-window keyboard accelerators (tuxlink-ng3). On a matching
 * combo, prevents the browser default and calls `onAction(id)`. Lives on the
 * main window only; the compose window keeps its own Ctrl+S / Ctrl+Enter.
 */
export function useAccelerators(onAction: (id: MenuActionId) => void): void {
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const id = matchAccelerator(e, isTextInputFocused(e.target));
      if (id) {
        e.preventDefault();
        onAction(id);
      }
    }
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onAction]);
}
