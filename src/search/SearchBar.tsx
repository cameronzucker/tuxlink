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
}

export function SearchBar({ spec, activeSaved, onSpecChange, onUnsave, onToggleDropdown, dropdownOpen }: SearchBarProps) {
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

  const handleEsc = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      onSpecChange(EMPTY_SPEC);
      if (dropdownOpen) onToggleDropdown();
    }
  };

  if (activeSaved) {
    return (
      <div className="search-bar focused" data-testid="search-bar">
        <span className="magnifier" aria-hidden="true">🔍</span>
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
      <span className="magnifier" aria-hidden="true">🔍</span>
      <input
        ref={inputRef}
        data-testid="searchbar-input"
        type="text"
        placeholder="Search messages…"
        value={spec.free_text ?? ''}
        onChange={(e) => onSpecChange({ ...spec, free_text: e.target.value || null })}
        onFocus={() => { if (!dropdownOpen) onToggleDropdown(); }}
        onKeyDown={handleEsc}
      />
      <button
        type="button"
        className="chev"
        data-testid="searchbar-chevron"
        onClick={onToggleDropdown}
        aria-label="Open search dropdown"
      >▾</button>
      <span className="shortcut">⌘F</span>
    </div>
  );
}
