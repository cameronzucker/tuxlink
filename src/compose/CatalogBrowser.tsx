// CatalogBrowser — hierarchical + flat-search picker for the WLE
// catalog plus the operator's custom forms. Replaces FormPicker as the
// form-entry picker; drives Compose into either native form mode
// (ICS-213, Bulletin — when the React registry has a Form for the id)
// or webview-form mode (everything else, served via WebviewFormHost).
//
// The Rust side returns a FLAT list with a `folder` field per template.
// We tree-build folders client-side: folders sorted alphabetically;
// the synthetic "Custom" folder (forms with `folder: ''` from the
// operator's custom-forms directory) always appears LAST so the
// operator's own forms have a stable shelf at the bottom of the list.
//
// Search collapses the tree into a flat filtered list (case-insensitive
// substring match against `template.label` and `template.folder`) so
// the operator can locate a form by partial-name without expanding
// every folder.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 10.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §7.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './CatalogBrowser.css';

/** Mirror of the Rust `forms::wle_templates::Template` struct (no serde
 *  rename_all on the Rust side → snake_case field names; the path is
 *  serialized as a string by serde's `PathBuf` default). The `source`
 *  discriminator matches the Rust `TemplateSource` enum's
 *  unit-variant serialization (`Bundled` / `Custom`). */
export interface Template {
  id: string;
  label: string;
  /** Folder path relative to bundle/custom root. Empty string for
   *  custom forms placed at the custom-root top level — those get
   *  grouped under the synthetic "Custom" folder for display. */
  folder: string;
  source: 'Bundled' | 'Custom';
  path: string;
}

export interface CatalogBrowserProps {
  /** Fired when the operator chooses a form. Compose routes the id to
   *  native form mode if the React registry has a Form for it, else
   *  to webview-form mode via `WebviewFormHost`. */
  onPick: (formId: string) => void;
  /** Fired when the operator backs out of the picker without choosing
   *  a form. Compose returns to plain-text mode. */
  onCancel: () => void;
}

/** Folder bucket built client-side from the flat catalog. The
 *  `isCustom` flag drives the always-last sort placement. */
interface FolderBucket {
  name: string;
  templates: Template[];
  isCustom: boolean;
}

const CUSTOM_FOLDER_LABEL = 'Custom';

/** Tree-build: bucket templates by their `folder` field, with the
 *  synthetic "Custom" folder collecting every template whose `folder`
 *  is empty (operator's custom-forms root). Returned in display
 *  order: alphabetical, with Custom always last. Templates inside
 *  each folder are sorted by label for determinism. */
function buildFolderTree(catalog: Template[]): FolderBucket[] {
  const buckets = new Map<string, FolderBucket>();
  for (const t of catalog) {
    // Empty folder strings live under the synthetic Custom folder.
    // We use the source-derived placement here — a "Bundled" template
    // shouldn't normally have an empty folder (the WLE bundle always
    // nests under a category dir), but if it did, we'd still want it
    // listed somewhere. The Custom folder is the catch-all.
    const isCustom = t.folder === '' || t.source === 'Custom';
    const name = isCustom ? CUSTOM_FOLDER_LABEL : t.folder;
    let bucket = buckets.get(name);
    if (!bucket) {
      bucket = { name, templates: [], isCustom };
      buckets.set(name, bucket);
    }
    bucket.templates.push(t);
  }
  // Stable order inside each folder.
  for (const bucket of buckets.values()) {
    bucket.templates.sort((a, b) => a.label.localeCompare(b.label));
  }
  const ordered = Array.from(buckets.values());
  // Alphabetical, with Custom pinned to the end.
  ordered.sort((a, b) => {
    if (a.isCustom && !b.isCustom) return 1;
    if (!a.isCustom && b.isCustom) return -1;
    return a.name.localeCompare(b.name);
  });
  return ordered;
}

/** Case-insensitive substring match against label + folder. Used in
 *  search mode to flatten the tree into matching templates. */
function matchesQuery(t: Template, query: string): boolean {
  const q = query.toLowerCase();
  return (
    t.label.toLowerCase().includes(q) ||
    t.folder.toLowerCase().includes(q)
  );
}

export function CatalogBrowser({ onPick, onCancel }: CatalogBrowserProps) {
  const [catalog, setCatalog] = useState<Template[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<Template[]>('forms_list_catalog')
      .then((result) => {
        if (cancelled) return;
        setCatalog(result ?? []);
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const folders = useMemo(() => buildFolderTree(catalog), [catalog]);

  // Search-mode flat results: every template that matches the query.
  // We keep the per-template folder context so the rendered row can
  // show "Folder / Label" disambiguation when the same label appears
  // in multiple folders.
  const searchResults = useMemo(() => {
    if (!searchQuery.trim()) return null;
    return catalog
      .filter((t) => matchesQuery(t, searchQuery))
      .sort((a, b) => a.label.localeCompare(b.label));
  }, [catalog, searchQuery]);

  const toggleFolder = (name: string) => {
    setExpandedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  };

  return (
    <div
      className="catalog-browser"
      role="dialog"
      aria-modal="true"
      aria-label="Pick a form"
      data-testid="catalog-browser"
    >
      <div className="catalog-browser__card">
        <h3 className="catalog-browser__title">Pick a form to author</h3>

        <input
          type="text"
          className="catalog-browser__search"
          placeholder="Search forms…"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          aria-label="Search forms"
          data-testid="catalog-browser-search"
        />

        <div className="catalog-browser__results" data-testid="catalog-browser-results">
          {loading && (
            <div className="catalog-browser__status">Loading catalog…</div>
          )}
          {error && (
            <div className="catalog-browser__error" role="alert">
              Form catalog failed to load: {error}
            </div>
          )}
          {!loading && !error && searchResults !== null && (
            <SearchResultsList
              results={searchResults}
              onPick={onPick}
            />
          )}
          {!loading && !error && searchResults === null && (
            <FolderTree
              folders={folders}
              expanded={expandedFolders}
              onToggle={toggleFolder}
              onPick={onPick}
            />
          )}
          {!loading && !error && catalog.length === 0 && (
            <div className="catalog-browser__empty">
              No forms found. The WLE snapshot may be missing from this build.
            </div>
          )}
        </div>

        <div className="catalog-browser__actions">
          <button
            type="button"
            className="catalog-browser__btn"
            onClick={onCancel}
            data-testid="catalog-browser-cancel"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Sub-views: folder tree (default) + flat search results.
// ---------------------------------------------------------------------

interface FolderTreeProps {
  folders: FolderBucket[];
  expanded: Set<string>;
  onToggle: (name: string) => void;
  onPick: (formId: string) => void;
}

function FolderTree({ folders, expanded, onToggle, onPick }: FolderTreeProps) {
  if (folders.length === 0) return null;
  return (
    <ul className="catalog-browser__folders" role="tree">
      {folders.map((folder) => {
        const isOpen = expanded.has(folder.name);
        return (
          <li
            key={folder.name}
            className="catalog-browser__folder"
            role="treeitem"
            aria-expanded={isOpen}
          >
            <button
              type="button"
              className="catalog-browser__folder-row"
              onClick={() => onToggle(folder.name)}
              aria-expanded={isOpen}
            >
              <span className="catalog-browser__folder-chevron" aria-hidden="true">
                {isOpen ? '▾' : '▸'}
              </span>
              <span
                className="catalog-browser__folder-name"
                data-testid="catalog-folder-name"
              >
                {folder.name}
              </span>
              <span className="catalog-browser__folder-count">
                {folder.templates.length}
              </span>
            </button>
            {isOpen && (
              <ul className="catalog-browser__templates" role="group">
                {folder.templates.map((t) => (
                  <li key={t.id} className="catalog-browser__template-row">
                    <button
                      type="button"
                      className="catalog-browser__template-btn"
                      onClick={() => onPick(t.id)}
                      data-testid={`catalog-template-${t.id}`}
                    >
                      {t.label}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </li>
        );
      })}
    </ul>
  );
}

interface SearchResultsListProps {
  results: Template[];
  onPick: (formId: string) => void;
}

function SearchResultsList({ results, onPick }: SearchResultsListProps) {
  if (results.length === 0) {
    return (
      <div className="catalog-browser__empty">No forms match that search.</div>
    );
  }
  return (
    <ul className="catalog-browser__search-results" role="listbox">
      {results.map((t) => (
        <li key={t.id} role="option" aria-selected="false">
          <button
            type="button"
            className="catalog-browser__template-btn"
            onClick={() => onPick(t.id)}
            data-testid={`catalog-template-${t.id}`}
          >
            <span className="catalog-browser__result-label">{t.label}</span>
            {t.folder && (
              <span className="catalog-browser__result-folder">{t.folder}</span>
            )}
          </button>
        </li>
      ))}
    </ul>
  );
}
