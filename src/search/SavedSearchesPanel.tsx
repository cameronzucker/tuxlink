/**
 * SavedSearchesPanel — modal for managing saved searches + index maintenance
 * (tuxlink-1hu Task 18). Opened from SearchDropdown's "Manage saved searches"
 * action in AppShell.
 *
 * NOT a tabbed Settings host (the existing SettingsPanel is a standalone modal
 * too — same pattern). The two modals are independent; they don't share state.
 *
 * v0.1 scope: list / unsave / create + rename via inline form + rebuild index.
 * Drag-reorder is deferred (visual handle only, no interaction).
 */

import React, { useState } from 'react';
import './SavedSearchesPanel.css';
import { EMPTY_SPEC } from './types';
import { renderQuery } from './queryRender';
import { useSavedSearches } from './useSavedSearches';

export interface SavedSearchesPanelProps {
  onClose: () => void;
}

export function SavedSearchesPanel({ onClose }: SavedSearchesPanelProps) {
  const saved = useSavedSearches();
  const [newOpen, setNewOpen] = useState(false);
  const [newName, setNewName] = useState('');
  const [newText, setNewText] = useState('');
  const [rebuildStats, setRebuildStats] = useState<{ messagesIndexed: number; elapsedMs: number } | null>(null);
  const [rebuildError, setRebuildError] = useState<string | null>(null);

  // Esc to close — matches SettingsPanel behaviour.
  React.useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  const submitNew = async () => {
    if (!newName.trim()) return;
    await saved.save(newName.trim(), { ...EMPTY_SPEC, free_text: newText.trim() || null });
    setNewName('');
    setNewText('');
    setNewOpen(false);
  };

  const handleRebuild = async () => {
    setRebuildStats(null);
    setRebuildError(null);
    try {
      const stats = await saved.rebuildIndex();
      setRebuildStats(stats);
    } catch {
      setRebuildError('Rebuild failed — check the session log for details.');
    }
  };

  return (
    <div
      className="tux-ssp-backdrop"
      data-testid="ssp-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-ssp"
        role="dialog"
        aria-modal="true"
        aria-label="Saved Searches"
        onClick={(e) => e.stopPropagation()}
      >
        {/* ── Header ── */}
        <div className="tux-ssp-header">
          <h2 className="tux-ssp-title">Saved Searches</h2>
          <button
            type="button"
            className="tux-ssp-close"
            aria-label="Close"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        {/* ── Body ── */}
        <div className="tux-ssp-body">
          {/* Saved list */}
          <ul className="tux-ssp-list" aria-label="Saved searches list">
            {saved.saved.length === 0 && (
              <li className="tux-ssp-empty">No saved searches yet.</li>
            )}
            {saved.saved
              .slice()
              .sort((a, b) => a.order - b.order)
              .map((s) => (
                <li key={s.id} className="tux-ssp-row">
                  {/* Drag handle — visual only in v0.1 */}
                  <span className="tux-ssp-drag" aria-hidden="true">⠿</span>
                  <span className="tux-ssp-name">{s.name}</span>
                  <span className="tux-ssp-query">{renderQuery(s.spec)}</span>
                  <button
                    type="button"
                    className="tux-ssp-unsave"
                    aria-label={`Unsave ${s.name}`}
                    onClick={() => void saved.unsave(s.id)}
                  >
                    🗑
                  </button>
                </li>
              ))}
          </ul>

          {/* + New saved search expander */}
          {!newOpen && (
            <button
              type="button"
              className="tux-ssp-new-btn"
              data-testid="new-saved-search"
              onClick={() => setNewOpen(true)}
            >
              + New saved search
            </button>
          )}
          {newOpen && (
            <div className="tux-ssp-new-form">
              <input
                className="tux-ssp-input"
                data-testid="new-saved-name-input"
                placeholder="Name (required)"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                autoFocus
                onKeyDown={(e) => { if (e.key === 'Enter') void submitNew(); }}
              />
              <input
                className="tux-ssp-input"
                placeholder="Free-text filter (optional)"
                value={newText}
                onChange={(e) => setNewText(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') void submitNew(); }}
              />
              <div className="tux-ssp-new-actions">
                <button
                  type="button"
                  className="tux-ssp-save-btn"
                  onClick={() => void submitNew()}
                  disabled={!newName.trim()}
                >
                  Save
                </button>
                <button
                  type="button"
                  className="tux-ssp-cancel-btn"
                  onClick={() => { setNewOpen(false); setNewName(''); setNewText(''); }}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          {/* ── Maintenance ── */}
          <div className="tux-ssp-maintenance">
            <h3 className="tux-ssp-section-label">Maintenance</h3>
            <button
              type="button"
              className="tux-ssp-rebuild-btn"
              data-testid="rebuild-index-btn"
              onClick={() => void handleRebuild()}
            >
              Rebuild search index
            </button>
            {rebuildStats && (
              <div className="tux-ssp-rebuild-banner" role="status">
                Indexed {rebuildStats.messagesIndexed} messages in {rebuildStats.elapsedMs} ms.
              </div>
            )}
            {rebuildError && (
              <div className="tux-ssp-rebuild-error" role="alert">
                {rebuildError}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
