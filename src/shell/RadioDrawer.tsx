import { useEffect, useRef, type ReactNode } from 'react';
import type { RadioPanelState } from '../radio/RadioPanel';
import './RadioDrawer.css';

export interface RadioDrawerProps {
  /** Drawer open/closed (only meaningful in compact mode; desktop ignores it via CSS). */
  open: boolean;
  /** Toggle handler (grip tap). */
  onToggle: () => void;
  /** Current session state — drives the grip's session-state tick. */
  sessionState: RadioPanelState;
  /** The radio panel mount block. */
  children: ReactNode;
}

/**
 * Wraps the radio-panel mount block. Desktop (>=1366px): `display: contents`
 * (CSS), so the child panel IS the 4th grid column — visually identical to the
 * pre-compact layout (the wrapper is layout- and a11y-transparent; we keep
 * display:contents rather than conditionally wrapping because a conditional
 * wrap would REMOUNT the live radio panel on a resize across the breakpoint,
 * dropping session state — Codex adrev R1 #4). Compact (<1366px): the wrapper is
 * an ABSOLUTE OVERLAY (tuxlink-813d D1) pinned to the right edge of .panes —
 * closed = only the ~16px grip strip pokes at the right edge; open = the panel
 * slides in (220ms). The reader keeps its full 1fr width in both states (it is
 * NOT pushed). The grip shows a coarse session-state tick and toggles open/closed.
 * tuxlink-h7q7.
 */
export function RadioDrawer({ open, onToggle, sessionState, children }: RadioDrawerProps) {
  const gripRef = useRef<HTMLButtonElement | null>(null);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const prevOpen = useRef(open);

  // Focus management (Claude adrev F10): on open, move focus into the panel
  // body so the keyboard user lands on the radio controls (not stranded on the
  // grip with Abort N tabs away); on close, return focus to the grip.
  useEffect(() => {
    if (open === prevOpen.current) return;
    if (open) {
      bodyRef.current?.focus();
    } else {
      gripRef.current?.focus();
    }
    prevOpen.current = open;
  }, [open]);

  return (
    <div className={`radio-drawer${open ? ' is-open' : ''}`} data-testid="radio-drawer">
      <button
        ref={gripRef}
        type="button"
        className="radio-drawer-grip"
        data-testid="radio-drawer-grip"
        data-session-state={sessionState}
        aria-expanded={open}
        aria-label={open ? 'Collapse radio panel' : 'Open radio panel'}
        onClick={onToggle}
      >
        <span className="radio-drawer-grip-dot" aria-hidden="true" />
      </button>
      <div className="radio-drawer-body" ref={bodyRef} tabIndex={-1}>
        {children}
      </div>
    </div>
  );
}
