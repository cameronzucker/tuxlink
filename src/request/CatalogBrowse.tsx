// CatalogBrowse — master-detail catalog browser for the Request Center
// (bd-tuxlink-eymu, Task D1). Replaces the home-view "browse" placeholder.
//
// Two columns inside the `request-content` region (the shared basket aside is
// RequestCenter's own pane — the "3rd column" of the 3-pane layout):
//   - LEFT  (catalog-browse-nav): one row per category present in `entries`,
//            with its item count. Selecting a row sets the active category.
//   - CENTER (catalog-browse-items): the active category's items, each with an
//            Add control. Add immediately puts ONE cms BasketItem in the shared
//            basket (no checkboxes, no Send — Send lives in the basket rail).
//
// Data-flow constraint (adrev #3 — single catalog-load owner): CatalogBrowse
// does NOT call useCatalog(). RequestCenter is the sole catalog-load owner and
// passes `entries` down as a prop.

import { useMemo, useState } from 'react';
import { groupByCategory, type CatalogEntry } from '../catalog/types';
import './CatalogBrowse.css';

export interface CatalogBrowseProps {
  /// Catalog entries from RequestCenter's single useCatalog. Never null here —
  /// RequestCenter gates this component behind a loaded, non-error catalog.
  entries: CatalogEntry[];
  /// Deep-link target from an openBrowse card (e.g. 'WX_EASTPAC', 'WL2K_RMS').
  /// Pre-selected on mount when present in `entries`; otherwise the first
  /// category is selected. null → first category (or empty state).
  initialCategory?: string | null;
  /// Adds a cms BasketItem (keyed by the entry's filename) to the shared basket.
  onAddCms: (entry: CatalogEntry) => void;
  /// Filenames already in the basket — rows for these show an "Added"
  /// affordance instead of an Add control.
  addedFilenames?: Set<string>;
  /// Global search needle (Task D2). When non-empty (trimmed), CatalogBrowse
  /// renders a FLAT cross-category results list of all `entries` matching the
  /// needle in filename, description, OR category (case-insensitive) — hiding
  /// the master-detail nav. Empty / absent → the D1 master-detail behaviour.
  searchQuery?: string;
  /// Returns to the request-first home view.
  onBack: () => void;
}

export function CatalogBrowse({
  entries,
  initialCategory,
  onAddCms,
  addedFilenames,
  searchQuery,
  onBack,
}: CatalogBrowseProps) {
  // Global search mode takes precedence over master-detail when the needle is
  // a non-empty trimmed string.
  const searchNeedle = (searchQuery ?? '').trim().toLowerCase();
  const isSearching = searchNeedle.length > 0;

  // Flat cross-category matches: filename || description || category, all
  // case-insensitive. Source order preserved (entries is already grouped).
  const searchResults = useMemo(() => {
    if (!isSearching) return [];
    return entries.filter(
      (e) =>
        e.filename.toLowerCase().includes(searchNeedle) ||
        e.description.toLowerCase().includes(searchNeedle) ||
        e.category.toLowerCase().includes(searchNeedle),
    );
  }, [entries, isSearching, searchNeedle]);

  // Shared item-row render — identical markup + Add/✓Added affordance for both
  // the master-detail center pane and the flat search-results list, so the
  // `cms:<filename>` add path and the Added state behave the same everywhere.
  const renderItemRow = (entry: CatalogEntry) => {
    const added = addedFilenames?.has(entry.filename) ?? false;
    return (
      <li
        key={entry.filename}
        className="catalog-browse__item"
        data-testid={`catalog-browse-item-${entry.filename}`}
      >
        <div className="catalog-browse__item-body">
          <span className="catalog-browse__item-filename">{entry.filename}</span>
          {entry.description && (
            <span className="catalog-browse__item-desc">{entry.description}</span>
          )}
          {formatSize(entry.size_bytes) && (
            <span className="catalog-browse__item-size">{formatSize(entry.size_bytes)}</span>
          )}
        </div>
        {added ? (
          <span
            className="catalog-browse__added"
            aria-label={`${entry.filename} already in request`}
          >
            ✓ Added
          </span>
        ) : (
          <button
            type="button"
            className="catalog-browse__add"
            onClick={() => onAddCms(entry)}
            aria-label={`Add ${entry.filename} to request`}
          >
            Add
          </button>
        )}
      </li>
    );
  };

  // Group once; insertion-ordered (groupByCategory preserves source order).
  const tree = useMemo(() => groupByCategory(entries), [entries]);
  const categoryNames = useMemo(() => Array.from(tree.categories.keys()), [tree]);

  // Resolve the initial active category. A deep-link target (`initialCategory`)
  // is authoritative even when its entries are not present in this catalog
  // load: the nav highlights it and the center shows a neutral "no items"
  // state, rather than silently snapping to an unrelated first category. This
  // also keeps the deep-link reflected in `data-category` on the browse root
  // (the openBrowse cards rely on that). When no deep-link is provided, default
  // to the first category (null only when there are no categories at all).
  const resolvedInitial = initialCategory ?? categoryNames[0] ?? null;

  // NOTE: `initialCategory` is read ONCE on mount. This is safe because the
  // parent remounts CatalogBrowse on each browse entry (openBrowse cards render
  // only in the home view, so the prop cannot change mid-mount). If a future
  // task adds a live category-change path (breadcrumbs, an in-browse "jump to
  // category", a second openBrowse while already in browse), add a useEffect to
  // sync `initialCategory → activeCategory` — otherwise the change is ignored.
  const [activeCategory, setActiveCategory] = useState<string | null>(resolvedInitial);
  const [filter, setFilter] = useState('');

  // The active category's items, narrowed by the in-pane text filter.
  const activeItems = useMemo(() => {
    if (!activeCategory) return [];
    const items = tree.categories.get(activeCategory) ?? [];
    const needle = filter.trim().toLowerCase();
    if (!needle) return items;
    return items.filter(
      (e) =>
        e.filename.toLowerCase().includes(needle) ||
        e.description.toLowerCase().includes(needle),
    );
  }, [tree, activeCategory, filter]);

  const isEmpty = categoryNames.length === 0;

  if (isSearching) {
    return (
      <div className="catalog-browse" data-testid="request-browse" data-category="">
        <div className="catalog-browse__bar">
          <button
            type="button"
            className="catalog-browse__back"
            data-testid="catalog-browse-back"
            onClick={onBack}
          >
            ← Clear search
          </button>
          <span className="catalog-browse__search-count" data-testid="catalog-search-count">
            {searchResults.length === 1
              ? '1 match'
              : `${searchResults.length} matches`}
          </span>
        </div>

        <div className="catalog-browse__items" data-testid="catalog-search-results">
          {searchResults.length === 0 ? (
            <div className="catalog-browse__noitems">
              No items match “{searchQuery?.trim()}”.
            </div>
          ) : (
            <ul className="catalog-browse__list">{searchResults.map(renderItemRow)}</ul>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      className="catalog-browse"
      data-testid="request-browse"
      data-category={activeCategory ?? ''}
    >
      <div className="catalog-browse__bar">
        <button
          type="button"
          className="catalog-browse__back"
          data-testid="catalog-browse-back"
          onClick={onBack}
        >
          ← Back
        </button>
        <input
          type="search"
          className="catalog-browse__filter"
          data-testid="catalog-browse-filter"
          placeholder="Filter items in this category…"
          aria-label="Filter items in this category"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>

      {isEmpty ? (
        <div className="catalog-browse__empty" data-testid="catalog-browse-empty">
          No catalog categories available.
        </div>
      ) : (
        <div className="catalog-browse__panes">
          <nav
            className="catalog-browse__nav"
            data-testid="catalog-browse-nav"
            aria-label="Catalog categories"
          >
            {categoryNames.map((name) => {
              const count = tree.categories.get(name)?.length ?? 0;
              const active = name === activeCategory;
              return (
                <button
                  key={name}
                  type="button"
                  className={
                    'catalog-browse__cat' + (active ? ' catalog-browse__cat--active' : '')
                  }
                  data-testid={`catalog-browse-cat-${name}`}
                  aria-current={active ? 'true' : undefined}
                  onClick={() => {
                    setActiveCategory(name);
                    setFilter('');
                  }}
                >
                  <span className="catalog-browse__cat-name">{name}</span>
                  <span className="catalog-browse__cat-count">{count}</span>
                </button>
              );
            })}
          </nav>

          <div className="catalog-browse__items" data-testid="catalog-browse-items">
            {activeItems.length === 0 ? (
              <div className="catalog-browse__noitems">No items match your filter.</div>
            ) : (
              <ul className="catalog-browse__list">{activeItems.map(renderItemRow)}</ul>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
