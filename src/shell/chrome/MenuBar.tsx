import { useState, useCallback, useEffect } from 'react';
import { MENU_TREE, type MenuActionId, type MenuNode } from './menuModel';
import { menuAnchorId } from '../../onboarding/menuAnchors';
import './chrome.css';

interface MenuBarProps {
  onAction: (id: MenuActionId) => void;
  /** Per-top-menu amber count badges (task-14 brief: the Part 97 consent
   *  moment "cannot hide" — the Routines menu label carries the count of
   *  runs parked awaiting operator transmit consent). Only `routines` exists
   *  today; absent or 0 renders no badge. */
  badges?: { routines?: number };
  /** Whether the Routines surface is currently popped to its own window
   *  (tuxlink-dmwte task 8, spec §5). While true the Routines top-level label
   *  reads "Routines ↗" (the visual pathway back to the window) and the
   *  "Dock Routines back" item is shown; while false it is hidden. This is the
   *  dynamic-affordance seam MENU_TREE deliberately lacks — same MenuBar-level
   *  special-case pattern as `badges`. */
  dockPopped?: boolean;
  /** tuxlink-9se1x: whether the Routines surface is open INLINE in the main
   *  pane right now — gates the "Back to Mailbox" item (meaningless when the
   *  surface is closed or popped to its own window). */
  routinesInlineOpen?: boolean;
  /** bd tuxlink-mfssz: whether the Elmer surface is popped to its own window
   *  — gates the Tools → "Dock Elmer back" item (same dynamic-affordance seam
   *  as `dockPopped` for Routines). */
  elmerPopped?: boolean;
}

/** The static "Dock Routines back" item is hidden unless Routines is popped. */
const ROUTINES_DOCKBACK_ID = 'menu:routines:dockback';

/** tuxlink-9se1x: "Back to Mailbox" is hidden unless the surface is open inline. */
const ROUTINES_CLOSE_ID = 'menu:routines:close';

/** bd tuxlink-mfssz: "Dock Elmer back" is hidden unless Elmer is popped. */
const ELMER_DOCKBACK_ID = 'menu:tools:elmer_dockback';

function MenuItems({ items, onPick }: { items: MenuNode[]; onPick: (id: MenuActionId) => void }) {
  return (
    <>
      {items.map((node, i) => {
        if (node.separator) return <div key={`sep-${i}`} className="tux-sep" />;
        if (node.submenu) {
          return (
            <div key={node.label} className="tux-mi tux-has-sub">
              {node.label}
              <span className="tux-chev">›</span>
              <div className="tux-submenu">
                <MenuItems items={node.submenu} onPick={onPick} />
              </div>
            </div>
          );
        }
        if (node.disabled) {
          // Not-yet-wired: disabled + badged so it reads as "coming", not broken.
          return (
            <button
              key={node.id}
              type="button"
              className="tux-mi tux-disabled"
              disabled
              aria-disabled="true"
              title="Coming in a future release"
              data-tour-anchor={node.id}
            >
              {node.label}
              <span className="tux-v01" aria-hidden="true">soon</span>
            </button>
          );
        }
        return (
          <button
            key={node.id}
            type="button"
            className="tux-mi"
            onClick={() => node.id && onPick(node.id)}
            data-tour-anchor={node.id}
          >
            {node.label}
            {node.accel && <span className="tux-accel">{node.accel}</span>}
          </button>
        );
      })}
    </>
  );
}

export function MenuBar({ onAction, badges, dockPopped = false, routinesInlineOpen = false, elmerPopped = false }: MenuBarProps) {
  const [openLabel, setOpenLabel] = useState<string | null>(null);
  const routinesBadge = badges?.routines ?? 0;

  const pick = useCallback((id: MenuActionId) => {
    onAction(id);
    setOpenLabel(null);
  }, [onAction]);

  // Click-away close: a document click that doesn't bubble up from inside the
  // menubar clears the open menu (prototype pattern).
  useEffect(() => {
    if (!openLabel) return;
    function handleClickAway() {
      setOpenLabel(null);
    }
    document.addEventListener('click', handleClickAway);
    return () => document.removeEventListener('click', handleClickAway);
  }, [openLabel]);

  return (
    <div className="tux-menubar" role="menubar">
      {MENU_TREE.map((menu) => {
        const isRoutines = menu.label === 'Routines';
        // Dynamic Routines affordances (spec §5): the "↗" pathway suffix and
        // the "Dock Routines back" item appear only while the surface is
        // popped. When docked, the dockback item is filtered from the dropdown
        // (it stays in MENU_TREE / the vocabulary — just not rendered).
        // bd tuxlink-mfssz: the Tools menu gets the same treatment for "Dock
        // Elmer back" (no top-label suffix — Tools is not an Elmer-only menu).
        const items = menu.items.filter(
          (n) =>
            // dockback only while popped; Back to Mailbox only while the
            // surface is open inline (tuxlink-9se1x).
            (n.id !== ROUTINES_DOCKBACK_ID || dockPopped) &&
            (n.id !== ROUTINES_CLOSE_ID || routinesInlineOpen) &&
            (n.id !== ELMER_DOCKBACK_ID || elmerPopped),
        );
        const topLabel = isRoutines && dockPopped ? 'Routines ↗' : menu.label;
        return (
          <div
            key={menu.label}
            className={`tux-menu${openLabel === menu.label ? ' tux-open' : ''}`}
            // hover-to-switch once a menu is open (native menubar behavior)
            onMouseEnter={() => setOpenLabel((cur) => (cur ? menu.label : cur))}
          >
            <button
              onClick={(e) => {
                e.stopPropagation();
                setOpenLabel((cur) => (cur === menu.label ? null : menu.label));
              }}
              data-tour-anchor={menuAnchorId(menu.label)}
            >
              {topLabel}
              {isRoutines && routinesBadge > 0 && (
                <span className="tux-menu-badge" data-testid="menu-badge-routines">
                  {routinesBadge}
                </span>
              )}
            </button>
            {openLabel === menu.label && (
              <div className="tux-dropdown">
                <MenuItems items={items} onPick={pick} />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
