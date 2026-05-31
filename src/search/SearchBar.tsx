import React, { useRef, useEffect } from 'react';
import './SearchBar.css';
import type { SavedSearch } from './types';

export interface SearchBarProps {
  /// Raw user-typed string (Gmail-style operators inline: `from:KX5DD damage`).
  /// Parsing into the structured QuerySpec happens in useSearch.
  value: string;
  /// When set, the bar shows the saved-search NAME + ★ instead of the raw
  /// input. Click the ★ to detach (unsave).
  activeSaved: SavedSearch | null;
  onValueChange: (next: string) => void;
  onUnsave: () => void;
  onToggleDropdown: () => void;
  dropdownOpen: boolean;
  /// Called on Enter — explicit "commit this query to recent history".
  /// Run-as-you-type queries do NOT auto-record (avoids per-keystroke history).
  onCommit?: () => void;
  /// Optional inline meta text shown between input and chevron when a search
  /// is active. e.g. `"3 matches · 47 ms"`. Replaces the deleted chip strip.
  metaText?: string | null;
}

export function SearchBar({
  value, activeSaved, onValueChange, onUnsave, onToggleDropdown, dropdownOpen,
  onCommit, metaText,
}: SearchBarProps) {
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
      onValueChange('');
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
        {metaText && <span className="meta" data-testid="searchbar-meta">{metaText}</span>}
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
        placeholder="Search messages… (try from:KX5DD damage)"
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
        onFocus={() => { if (!dropdownOpen) onToggleDropdown(); }}
        onKeyDown={handleKey}
      />
      {metaText && <span className="meta" data-testid="searchbar-meta">{metaText}</span>}
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
