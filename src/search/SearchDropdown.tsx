import { useEffect, useState } from 'react';
import './SearchDropdown.css';
import { renderQuery } from './queryRender';
import type { RecentSearch, SavedSearch } from './types';

export interface SearchDropdownProps {
  saved: SavedSearch[];
  recent: RecentSearch[];
  activeSavedId: string | null;
  /// Currently-typed query text. When non-empty AND no saved is active,
  /// a "Save this search" row appears at the top of the dropdown with
  /// an inline rename input.
  currentQueryText: string;
  onRunSaved: (s: SavedSearch) => void;
  onRunRecent: (r: RecentSearch) => void;
  /// Promote a recent → saved with the given user-supplied name.
  onPromoteRecent: (r: RecentSearch, name: string) => void;
  onUnsaveActive: () => void;
  onManage: () => void;
  onClose: () => void;
  /// Save the currently-typed query with the given name.
  onSaveCurrent?: (name: string) => void;
  /// Wipe the recent-history list. Saved searches are untouched.
  onClearRecent?: () => void;
}

export function SearchDropdown(props: SearchDropdownProps) {
  const {
    saved, recent, activeSavedId, currentQueryText, onRunSaved, onRunRecent,
    onPromoteRecent, onManage, onClose, onSaveCurrent, onClearRecent,
  } = props;
  const totalRows = saved.length + recent.length;
  const [focusIdx, setFocusIdx] = useState(0);

  // Inline-rename state — non-null = in name-edit mode for that target.
  const [namingCurrent, setNamingCurrent] = useState<string | null>(null);
  const [namingRecent, setNamingRecent] = useState<{ idx: number; value: string } | null>(null);

  const showSaveCurrent =
    !!onSaveCurrent && !activeSavedId && currentQueryText.trim().length > 0;

  const trimName = (s: string) => s.trim().slice(0, 40);

  const submitCurrent = () => {
    const name = (namingCurrent ?? '').trim();
    if (name && onSaveCurrent) onSaveCurrent(name);
    setNamingCurrent(null);
  };

  const submitRecent = (r: RecentSearch) => {
    const name = (namingRecent?.value ?? '').trim();
    if (name) onPromoteRecent(r, name);
    setNamingRecent(null);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // When inline rename is active, defer to the input's own handler.
      if (namingCurrent !== null || namingRecent !== null) return;
      if (e.key === 'ArrowDown') { e.preventDefault(); setFocusIdx((i) => Math.min(i + 1, totalRows - 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setFocusIdx((i) => Math.max(i - 1, 0)); }
      else if (e.key === 'Enter') {
        e.preventDefault();
        if (focusIdx < saved.length) onRunSaved(saved[focusIdx]);
        else onRunRecent(recent[focusIdx - saved.length]);
      } else if (e.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [focusIdx, saved, recent, totalRows, onRunSaved, onRunRecent, onClose, namingCurrent, namingRecent]);

  return (
    <div className="search-dropdown" data-testid="search-dropdown">
      {showSaveCurrent && namingCurrent === null && (
        <button
          type="button"
          className="dropdown-save-current"
          data-testid="dropdown-save-current"
          onClick={() => setNamingCurrent(trimName(currentQueryText))}
        >
          <span className="star filled" aria-hidden="true">★</span>
          <span className="label">Save this search</span>
          <span className="query-preview">{currentQueryText}</span>
        </button>
      )}
      {namingCurrent !== null && (
        <div className="dropdown-name-row" data-testid="dropdown-name-current">
          <span className="star filled" aria-hidden="true">★</span>
          <input
            type="text"
            autoFocus
            value={namingCurrent}
            data-testid="dropdown-name-input-current"
            placeholder="Name this search"
            onChange={(e) => setNamingCurrent(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') { e.preventDefault(); submitCurrent(); }
              else if (e.key === 'Escape') { e.preventDefault(); setNamingCurrent(null); }
            }}
          />
          <button type="button" className="action primary" onClick={submitCurrent}>Save</button>
          <button type="button" className="action" onClick={() => setNamingCurrent(null)}>Cancel</button>
        </div>
      )}

      <div className="dropdown-section-label" data-testid="section-label-saved">
        Saved {saved.length > 0 && <span className="muted">(pinned)</span>}
      </div>
      {saved.length === 0 && <div className="dropdown-empty">No saved searches yet — star a recent one or save your current search above.</div>}
      {saved.map((s, i) => (
        <div
          key={s.id}
          className={`dropdown-row${focusIdx === i ? ' focused' : ''}${s.id === activeSavedId ? ' active' : ''}`}
          data-testid={`dropdown-saved-row-${s.id}`}
          onClick={() => onRunSaved(s)}
        >
          <span className="star filled" aria-hidden="true">★</span>
          <div className="body">
            <span className="name">{s.name}</span>
            <span className="query">{renderQuery(s.spec)}</span>
          </div>
        </div>
      ))}

      <div className="dropdown-section-label" data-testid="section-label-recent">Recent</div>
      {recent.length === 0 && <div className="dropdown-empty">No recent searches yet.</div>}
      {recent.map((r, i) => {
        const idx = saved.length + i;
        const isNaming = namingRecent?.idx === i;
        if (isNaming) {
          return (
            <div key={`recent-${i}-naming`} className="dropdown-name-row" data-testid={`dropdown-name-recent-${i}`}>
              <span className="star filled" aria-hidden="true">★</span>
              <input
                type="text"
                autoFocus
                value={namingRecent!.value}
                placeholder="Name this search"
                onChange={(e) => setNamingRecent({ idx: i, value: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') { e.preventDefault(); submitRecent(r); }
                  else if (e.key === 'Escape') { e.preventDefault(); setNamingRecent(null); }
                }}
              />
              <button type="button" className="action primary" onClick={() => submitRecent(r)}>Save</button>
              <button type="button" className="action" onClick={() => setNamingRecent(null)}>Cancel</button>
            </div>
          );
        }
        return (
          <div
            key={`recent-${i}`}
            className={`dropdown-row unsaved${focusIdx === idx ? ' focused' : ''}`}
            data-testid={`dropdown-recent-row-${i}`}
            onClick={() => onRunRecent(r)}
          >
            <button
              type="button"
              className="star empty"
              data-testid={`dropdown-recent-star-${i}`}
              aria-label="Star to save"
              onClick={(e) => { e.stopPropagation(); setNamingRecent({ idx: i, value: trimName(renderQuery(r.spec)) }); }}
            >☆</button>
            <div className="body"><span className="name">{renderQuery(r.spec)}</span></div>
          </div>
        );
      })}

      <div className="dropdown-footer">
        <span className="hints">↑↓ navigate · ⏎ commit · Esc close</span>
        {onClearRecent && recent.length > 0 && (
          <button type="button" className="action" data-testid="dropdown-clear-recent" onClick={onClearRecent}>Clear recents</button>
        )}
        <button type="button" className="action" data-testid="dropdown-manage" onClick={onManage}>Manage… ⚙</button>
      </div>
    </div>
  );
}
