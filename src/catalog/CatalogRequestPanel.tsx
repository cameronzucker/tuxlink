// CatalogRequestPanel — inline overlay for WLE catalog inquiries (tuxlink-ddiq).
//
// Opens from Message → Catalog Request. Shows the bundled WLE catalog as
// a tree of categories with multi-select checkboxes for inquiries. On Send,
// invokes `catalog_send_inquiry` which routes the request through the
// existing outgoing rails (To: INQUIRY@winlink.org, Subject: REQUEST,
// body = newline-joined filenames).
//
// Inline-overlay pattern matches SettingsPanel / ThemeDesigner (operator
// preference per feedback_inline_ui_no_window_clutter).

import { useMemo, useState } from 'react';
import { groupByCategory } from './types';
import { useCatalog, sendCatalogInquiry } from './useCatalog';
import './CatalogRequestPanel.css';

export interface CatalogRequestPanelProps {
  onClose: () => void;
}

type SendState =
  | { kind: 'idle' }
  | { kind: 'sending' }
  | { kind: 'success'; mid: string; count: number }
  | { kind: 'error'; message: string };

export function CatalogRequestPanel({ onClose }: CatalogRequestPanelProps) {
  const { entries, loading, error } = useCatalog();
  const [filter, setFilter] = useState('');
  const [selectedFilenames, setSelectedFilenames] = useState<Set<string>>(new Set());
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(new Set());
  const [sendState, setSendState] = useState<SendState>({ kind: 'idle' });

  // Build the tree once entries are loaded; recompute when filter changes.
  const tree = useMemo(() => {
    if (!entries) return null;
    const needle = filter.trim().toLowerCase();
    const filtered = needle
      ? entries.filter(
          (e) =>
            e.filename.toLowerCase().includes(needle) ||
            e.description.toLowerCase().includes(needle) ||
            e.category.toLowerCase().includes(needle),
        )
      : entries;
    return groupByCategory(filtered);
  }, [entries, filter]);

  const toggleSelected = (filename: string) => {
    setSelectedFilenames((prev) => {
      const next = new Set(prev);
      if (next.has(filename)) next.delete(filename);
      else next.add(filename);
      return next;
    });
  };

  const toggleCategory = (category: string) => {
    setExpandedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(category)) next.delete(category);
      else next.add(category);
      return next;
    });
  };

  const clearSelection = () => {
    setSelectedFilenames(new Set());
    setSendState({ kind: 'idle' });
  };

  const onSend = async () => {
    if (selectedFilenames.size === 0) return;
    setSendState({ kind: 'sending' });
    try {
      const filenames = Array.from(selectedFilenames);
      const mid = await sendCatalogInquiry(filenames);
      setSendState({ kind: 'success', mid, count: filenames.length });
      setSelectedFilenames(new Set());
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      setSendState({ kind: 'error', message });
    }
  };

  return (
    <div className="catalog-overlay" data-testid="catalog-overlay" onClick={onClose}>
      <div
        className="catalog-panel"
        data-testid="catalog-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="catalog-header">
          <div className="catalog-title">
            <h2>Catalog Request</h2>
            <p className="catalog-subtitle">
              Request bulletins, weather, gateway lists, propagation data, and other items from the
              CMS. Responses arrive in your inbox as separate messages.
            </p>
          </div>
          <button
            type="button"
            className="catalog-close"
            data-testid="catalog-close"
            onClick={onClose}
            aria-label="Close catalog request panel"
            title="Close"
          >
            ✕
          </button>
        </header>

        {loading && (
          <div className="catalog-loading" data-testid="catalog-loading">
            Loading catalog…
          </div>
        )}

        {error && (
          <div className="catalog-error" data-testid="catalog-error">
            Failed to load catalog: {error}
          </div>
        )}

        {entries && tree && (
          <>
            <div className="catalog-filter-row">
              <input
                type="search"
                className="catalog-filter"
                data-testid="catalog-filter"
                placeholder="Filter by category, filename, or description…"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
              />
              <span className="catalog-count" data-testid="catalog-count">
                {tree.totalCount} of {entries.length} shown
              </span>
            </div>

            <div className="catalog-tree" data-testid="catalog-tree">
              {tree.categories.size === 0 ? (
                <div className="catalog-empty">No entries match your filter.</div>
              ) : (
                Array.from(tree.categories.entries()).map(([category, items]) => {
                  const expanded = expandedCategories.has(category) || filter.trim().length > 0;
                  const selectedInCategory = items.filter((it) => selectedFilenames.has(it.filename))
                    .length;
                  return (
                    <section
                      key={category}
                      className="catalog-category"
                      data-testid={`catalog-category-${category}`}
                    >
                      <button
                        type="button"
                        className="catalog-category-header"
                        data-testid={`catalog-category-header-${category}`}
                        onClick={() => toggleCategory(category)}
                        aria-expanded={expanded}
                      >
                        <span className="catalog-category-chevron">{expanded ? '▾' : '▸'}</span>
                        <span className="catalog-category-name">{category}</span>
                        <span className="catalog-category-count">
                          {selectedInCategory > 0
                            ? `${selectedInCategory} / ${items.length} selected`
                            : `${items.length}`}
                        </span>
                      </button>
                      {expanded && (
                        <ul className="catalog-items">
                          {items.map((entry) => (
                            <li key={entry.filename} className="catalog-item">
                              <label className="catalog-item-label">
                                <input
                                  type="checkbox"
                                  data-testid={`catalog-item-${entry.filename}`}
                                  checked={selectedFilenames.has(entry.filename)}
                                  onChange={() => toggleSelected(entry.filename)}
                                />
                                <span className="catalog-item-filename">{entry.filename}</span>
                                <span className="catalog-item-description">{entry.description}</span>
                                <span className="catalog-item-size">
                                  {formatSize(entry.size_bytes)}
                                </span>
                              </label>
                            </li>
                          ))}
                        </ul>
                      )}
                    </section>
                  );
                })
              )}
            </div>

            <footer className="catalog-footer">
              <div className="catalog-selection-summary" data-testid="catalog-selection-summary">
                {selectedFilenames.size === 0
                  ? 'No items selected.'
                  : `${selectedFilenames.size} item${selectedFilenames.size === 1 ? '' : 's'} selected`}
              </div>
              <div className="catalog-actions">
                <button
                  type="button"
                  className="catalog-clear"
                  data-testid="catalog-clear"
                  onClick={clearSelection}
                  disabled={selectedFilenames.size === 0 && sendState.kind === 'idle'}
                >
                  Clear
                </button>
                <button
                  type="button"
                  className="catalog-send"
                  data-testid="catalog-send"
                  onClick={onSend}
                  disabled={selectedFilenames.size === 0 || sendState.kind === 'sending'}
                >
                  {sendState.kind === 'sending' ? 'Sending…' : 'Send Request'}
                </button>
              </div>

              {sendState.kind === 'success' && (
                <div className="catalog-send-status catalog-send-success" data-testid="catalog-send-success">
                  Queued {sendState.count} inquir{sendState.count === 1 ? 'y' : 'ies'} to outbox
                  (MID {sendState.mid}). Connect to the CMS to send.
                </div>
              )}
              {sendState.kind === 'error' && (
                <div className="catalog-send-status catalog-send-error" data-testid="catalog-send-error">
                  Send failed: {sendState.message}
                </div>
              )}
            </footer>
          </>
        )}
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
