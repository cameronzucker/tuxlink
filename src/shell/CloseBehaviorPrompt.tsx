/**
 * CloseBehaviorPrompt — one-time "Tuxlink keeps running when you close the
 * window" explainer modal (tuxlink-5rvp / #882).
 *
 * The main window's close (× / taskbar / Alt-F4) is intercepted backend-side and
 * minimizes to tray instead of quitting, so closing mid-transfer does not kill
 * the process. On the FIRST window close, lib.rs's CloseRequested handler emits
 * `show-close-prompt` (instead of minimizing) so this modal can explain that
 * behavior and let the operator keep it or opt out.
 *
 * Both choices route through the `resolve_close_prompt` backend command, which
 * persists the choice (`close_to_tray` + marks `close_prompt_seen` so the prompt
 * never reappears) AND performs the close the operator asked for (minimize, or
 * `app.exit(0)` to quit). The Settings toggle (set_close_to_tray) is the
 * change-it-later path.
 *
 * Inline overlay per feedback_inline_ui_no_window_clutter — not a separate OS
 * window. Mirrors VerifyCmsDialog's backdrop/panel chrome.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './CloseBehaviorPrompt.css';

export function CloseBehaviorPrompt() {
  const [open, setOpen] = useState(false);
  const primaryRef = useRef<HTMLButtonElement | null>(null);
  // Guards against double-resolution (e.g. Escape firing while a button click is
  // already in flight): once we've sent the operator's answer, ignore further
  // dismiss paths until the modal re-opens.
  const resolved = useRef(false);

  // Open the modal when the backend signals the first window close.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let mounted = true;
    void listen('show-close-prompt', () => {
      resolved.current = false;
      setOpen(true);
    }).then((fn) => {
      if (mounted) unlisten = fn;
      else fn();
    });
    return () => {
      mounted = false;
      if (unlisten) unlisten();
    };
  }, []);

  // Send the operator's answer to the backend, which persists the choice and
  // performs the minimize/exit. `quitOnClose` is the JS arg key the Tauri
  // command expects (camelCase → Rust `quit_on_close`).
  const resolve = useCallback(async (quitOnClose: boolean) => {
    if (resolved.current) return;
    resolved.current = true;
    setOpen(false);
    try {
      await invoke<void>('resolve_close_prompt', { quitOnClose });
    } catch {
      // If the persist/close call fails the window simply stays open (the
      // backend already prevent_close()'d it); nothing destructive happens.
      // Re-arm so a subsequent close attempt can retry.
      resolved.current = false;
    }
  }, []);

  // Autofocus the primary ("Keep running") button when the modal opens.
  useEffect(() => {
    if (open) primaryRef.current?.focus();
  }, [open]);

  // Escape defaults to the SAFE outcome: keep running (minimize). Closing the
  // modal without an explicit choice must not strand the user — it marks the
  // prompt seen and minimizes, exactly like clicking "Keep running on close".
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') void resolve(false);
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, resolve]);

  if (!open) return null;

  return (
    <div
      className="tux-closeprompt-backdrop"
      data-testid="close-prompt-backdrop"
      // Overlay click defaults to the safe "keep running" outcome.
      onClick={() => void resolve(false)}
    >
      <div
        className="tux-closeprompt-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="tux-closeprompt-title"
        data-testid="close-prompt-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-closeprompt-body">
          <h2 className="tux-closeprompt-title" id="tux-closeprompt-title">
            Tuxlink keeps running when you close the window
          </h2>
          <p className="tux-closeprompt-text">
            Closing the window minimizes Tuxlink instead of quitting, so an active
            transfer or connection is not interrupted. Tuxlink stays available in
            the system tray and window list.
          </p>
          <p className="tux-closeprompt-text">
            Quit any time from File → Quit or Ctrl+Q.
          </p>
        </div>

        <div className="tux-closeprompt-actions">
          <button
            type="button"
            ref={primaryRef}
            className="tux-closeprompt-button tux-closeprompt-button-primary"
            data-testid="close-prompt-keep"
            onClick={() => void resolve(false)}
          >
            Keep running on close
          </button>
          <button
            type="button"
            className="tux-closeprompt-button"
            data-testid="close-prompt-quit"
            onClick={() => void resolve(true)}
          >
            Quit on close
          </button>
        </div>

        <p className="tux-closeprompt-footnote">
          You can change this later in Settings.
        </p>
      </div>
    </div>
  );
}
