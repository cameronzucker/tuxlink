import { useState, useCallback, useEffect } from 'react';
import { MENU_TREE, type MenuActionId, type MenuNode } from './menuModel';
import './chrome.css';

interface MenuBarProps {
  onAction: (id: MenuActionId) => void;
}

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
        return (
          <button key={node.id} className="tux-mi" onClick={() => node.id && onPick(node.id)}>
            {node.label}
            {node.accel && <span className="tux-accel">{node.accel}</span>}
          </button>
        );
      })}
    </>
  );
}

export function MenuBar({ onAction }: MenuBarProps) {
  const [openLabel, setOpenLabel] = useState<string | null>(null);

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
      {MENU_TREE.map((menu) => (
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
          >
            {menu.label}
          </button>
          {openLabel === menu.label && (
            <div className="tux-dropdown">
              <MenuItems items={menu.items} onPick={pick} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
