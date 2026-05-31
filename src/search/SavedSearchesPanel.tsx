/**
 * SavedSearchesPanel — modal for managing existing saved searches + index
 * maintenance (tuxlink-1hu Task 18). Opened from SearchDropdown's
 * "Manage… ⚙" action.
 *
 * Saved searches are CREATED from the SearchDropdown's "Save this search"
 * row (when a query is typed) or by ☆-promoting a recent entry. This
 * panel is pure management: list, unsave, rebuild index.
 */

import React, { useState } from 'react';
import './SavedSearchesPanel.css';
import { renderQuery } from './queryRender';
import { useSavedSearches } from './useSavedSearches';

export interface SavedSearchesPanelProps {
  onClose: () => void;
}

export function SavedSearchesPanel({ onClose }: SavedSearchesPanelProps) {
  const saved = useSavedSearches();
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
          <p className="tux-ssp-hint">
            To create a saved search: type a query in the search bar (Gmail-style
            operators like <code>from:KX5DD</code> work), then click <strong>★ Save
            this search</strong> in the dropdown.
          </p>

          <ul className="tux-ssp-list" aria-label="Saved searches list">
            {saved.saved.length === 0 && (
              <li className="tux-ssp-empty">No saved searches yet.</li>
            )}
            {saved.saved
              .slice()
              .sort((a, b) => a.order - b.order)
              .map((s) => (
                <li key={s.id} className="tux-ssp-row">
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

          {/* ── Maintenance ── */}
          <div className="tux-ssp-maintenance">
            <h3 className="tux-ssp-section-label">Maintenance</h3>
            <p className="tux-ssp-maintenance-hint">
              The search index is built incrementally on every new message.
              Rebuild from disk if results look stale, after a major upgrade,
              or to index messages stored before find-messages shipped.
            </p>
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
