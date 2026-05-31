import { useEffect, useState } from 'react';
import './SearchDropdown.css';
import { renderQuery } from './queryRender';
import type { RecentSearch, SavedSearch } from './types';

export interface SearchDropdownProps {
  saved: SavedSearch[];
  recent: RecentSearch[];
  activeSavedId: string | null;
  onRunSaved: (s: SavedSearch) => void;
  onRunRecent: (r: RecentSearch) => void;
  onPromoteRecent: (r: RecentSearch) => void;
  onUnsaveActive: () => void;
  onManage: () => void;
  onClose: () => void;
}

export function SearchDropdown(props: SearchDropdownProps) {
  const { saved, recent, activeSavedId, onRunSaved, onRunRecent, onPromoteRecent, onManage, onClose } = props;
  const totalRows = saved.length + recent.length;
  const [focusIdx, setFocusIdx] = useState(0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
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
  }, [focusIdx, saved, recent, totalRows, onRunSaved, onRunRecent, onClose]);

  return (
    <div className="search-dropdown" data-testid="search-dropdown">
      <div className="dropdown-section-label" data-testid="section-label-saved">
        Saved {saved.length > 0 && <span className="muted">(pinned)</span>}
      </div>
      {saved.length === 0 && <div className="dropdown-empty">No saved searches yet — star a recent one to save it.</div>}
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
              onClick={(e) => { e.stopPropagation(); onPromoteRecent(r); }}
            >☆</button>
            <div className="body"><span className="name">{renderQuery(r.spec)}</span></div>
          </div>
        );
      })}

      <div className="dropdown-footer">
        <span className="hints">↑↓ navigate · ⏎ run · Esc close</span>
        <button type="button" className="action" data-testid="dropdown-manage" onClick={onManage}>Manage… ⚙</button>
      </div>
    </div>
  );
}
