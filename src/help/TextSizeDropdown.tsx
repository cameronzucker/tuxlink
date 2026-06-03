import { useEffect, useRef, useState } from 'react';
import { FONT_PRESETS, type FontPreset } from './useFontSize';
import './TextSizeDropdown.css';

interface TextSizeDropdownProps {
  value: FontPreset;
  onChange: (value: FontPreset) => void;
}

export function TextSizeDropdown({ value, onChange }: TextSizeDropdownProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    const onClickAway = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onClickAway);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onClickAway);
    };
  }, [open]);

  const handleSelect = (p: FontPreset) => {
    onChange(p);
    setOpen(false);
  };

  return (
    <div className="tux-help-textsize" ref={rootRef}>
      <button
        type="button"
        className="tux-help-textsize-button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="lab">Text size:</span>
        <span className="val">{value}</span>
        <span className="chev">▼</span>
      </button>
      {open && (
        <div className="tux-help-textsize-menu" role="menu">
          {FONT_PRESETS.map((p) => (
            <div
              key={p}
              role="menuitem"
              aria-checked={p === value}
              className={`tux-help-textsize-item${p === value ? ' active' : ''}`}
              onClick={() => handleSelect(p)}
              tabIndex={0}
            >
              <span>{p}</span>
              {p === value && <span className="check">✓</span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
