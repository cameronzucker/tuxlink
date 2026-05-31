import React, { useRef, useEffect } from 'react';
import './SearchBar.css';
import { EMPTY_SPEC, type QuerySpec, type SavedSearch } from './types';

export interface SearchBarProps {
  spec: QuerySpec;
  activeSaved: SavedSearch | null;
  onSpecChange: (spec: QuerySpec) => void;
  onUnsave: () => void;
  onToggleDropdown: () => void;
  dropdownOpen: boolean;
  /// Called on Enter — explicit "commit this query to recent history".
  /// Run-as-you-type queries do NOT auto-record (avoids per-keystroke history).
  onCommit?: () => void;
}

export function SearchBar({ spec, activeSaved, onSpecChange, onUnsave, onToggleDropdown, dropdownOpen, onCommit }: SearchBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        inputRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const handleKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      onSpecChange(EMPTY_SPEC);
      if (dropdownOpen) onToggleDropdown();
    } else if (e.key === 'Enter') {
      onCommit?.();
      if (dropdownOpen) onToggleDropdown();
    }
  };

  if (activeSaved) {
    return (
      <div className="search-bar focused" data-testid="search-bar">
        <MagnifierIcon />
        <button
          type="button"
          className="saved-star"
          data-testid="searchbar-saved-star"
          aria-label={`Unsave ${activeSaved.name}`}
          onClick={onUnsave}
        >★</button>
        <span className="saved-name" data-testid="searchbar-saved-name">{activeSaved.name}</span>
        <button
          type="button"
          className="chev"
          data-testid="searchbar-chevron"
          onClick={onToggleDropdown}
          aria-label="Open search dropdown"
        >▾</button>
      </div>
    );
  }

  return (
    <div className="search-bar" data-testid="search-bar">
      <MagnifierIcon />
      <input
        ref={inputRef}
        data-testid="searchbar-input"
        type="text"
        placeholder="Search messages…"
        value={spec.free_text ?? ''}
        onChange={(e) => onSpecChange({ ...spec, free_text: e.target.value || null })}
        onFocus={() => { if (!dropdownOpen) onToggleDropdown(); }}
        onKeyDown={handleKey}
      />
      <button
        type="button"
        className="chev"
        data-testid="searchbar-chevron"
        onClick={onToggleDropdown}
        aria-label="Open search dropdown"
      >▾</button>
      <span className="shortcut">Ctrl+F</span>
    </div>
  );
}

function MagnifierIcon() {
  return (
    <svg
      className="magnifier"
      data-testid="searchbar-magnifier"
      viewBox="0 0 24 24"
      width="16"
      height="16"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </svg>
  );
}
