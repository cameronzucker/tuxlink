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

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { normalizeCatalogId } from '../forms';
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

/** Result of `forms_check_for_update` (Phase 3 — tuxlink-xipa). camelCase
 *  matches the Rust `FormsRefreshStatus` `#[serde(rename_all = ...)]`. */
interface FormsRefreshStatus {
  /** `null` when no runtime snapshot has ever been installed (catalog
   *  served from the build-time bundle). */
  currentVersion: string | null;
  remoteVersion: string;
  archiveUrl: string;
  updateAvailable: boolean;
}

/** Result of `forms_refresh`. camelCase per the Rust serde rename. */
interface InstallReport {
  installedVersion: string;
  formCount: number;
  prevVersion: string | null;
}

/** Refresh sub-flow state. Mounted inline inside the CatalogBrowser
 *  dialog (rather than a nested modal) so Escape unambiguously backs
 *  out one level at a time: refreshing → idle → close picker. */
type RefreshStep =
  | { kind: 'idle' }
  | { kind: 'checking' }
  | { kind: 'up-to-date'; status: FormsRefreshStatus }
  | { kind: 'confirming'; status: FormsRefreshStatus }
  | { kind: 'refreshing'; status: FormsRefreshStatus }
  | { kind: 'done'; report: InstallReport }
  | { kind: 'error'; message: string };

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
  const [refreshStep, setRefreshStep] = useState<RefreshStep>({ kind: 'idle' });
  // Ref + auto-focus for the search input. Important #7 from the P1 Task 10
  // code review: without an initial-focus target, assistive-tech users land
  // in an aria-modal="true" dialog with no announced focus position. Search
  // is the right default focus because it's the primary entry point for
  // browsing the 250-entry catalog (typeahead beats expand-every-folder).
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    searchInputRef.current?.focus();
  }, []);

  // Escape unwinds one level: refresh sub-flow → picker close. Important #6
  // from the P1 Task 10 code review: FormPicker had Escape→onCancel;
  // CatalogBrowser was missing it. The refresh sub-flow takes precedence
  // so the operator can back out of the refresh confirmation/error without
  // dismissing the whole picker.
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        if (refreshStep.kind !== 'idle' && refreshStep.kind !== 'refreshing') {
          setRefreshStep({ kind: 'idle' });
        } else if (refreshStep.kind === 'idle') {
          onCancel();
        }
        // refreshing: ignore Escape — the install is in flight and
        // canceling mid-rename could leave the runtime root in a half-
        // state. The install rollback covers genuine swap failures; the
        // operator waits for completion (typically 5–10s).
      }
    };
    document.addEventListener('keydown', handleKey);
    return () => document.removeEventListener('keydown', handleKey);
  }, [onCancel, refreshStep.kind]);

  // Catalog fetch — extracted into a callable so the post-refresh path
  // can re-run it. Returns a Promise so the refresh flow can chain.
  const fetchCatalog = useCallback(async (): Promise<void> => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Template[]>('forms_list_catalog');
      setCatalog(result ?? []);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

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

  // Refresh flow handlers. `kickOffCheck` runs the read-only check then
  // routes to confirming/up-to-date/error. `confirmRefresh` runs the
  // install on operator confirmation. `dismissRefresh` returns the
  // dialog to idle (back-button semantics + the post-success "OK").
  const kickOffCheck = useCallback(async () => {
    setRefreshStep({ kind: 'checking' });
    try {
      const status = await invoke<FormsRefreshStatus>('forms_check_for_update');
      setRefreshStep(
        status.updateAvailable
          ? { kind: 'confirming', status }
          : { kind: 'up-to-date', status },
      );
    } catch (e: unknown) {
      setRefreshStep({
        kind: 'error',
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }, []);

  const confirmRefresh = useCallback(async () => {
    if (refreshStep.kind !== 'confirming') return;
    const inFlight = refreshStep.status;
    setRefreshStep({ kind: 'refreshing', status: inFlight });
    try {
      const report = await invoke<InstallReport>('forms_refresh');
      setRefreshStep({ kind: 'done', report });
      // Re-fetch the catalog so the new entries appear without the
      // operator having to close + reopen the picker.
      await fetchCatalog();
    } catch (e: unknown) {
      setRefreshStep({
        kind: 'error',
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }, [refreshStep, fetchCatalog]);

  const dismissRefresh = useCallback(() => {
    setRefreshStep({ kind: 'idle' });
  }, []);

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

        {refreshStep.kind !== 'idle' ? (
          <RefreshPane
            step={refreshStep}
            onConfirm={confirmRefresh}
            onDismiss={dismissRefresh}
            data-testid="catalog-refresh-pane"
          />
        ) : (
          <>
            <input
              ref={searchInputRef}
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
          </>
        )}

        <div className="catalog-browser__actions">
          {refreshStep.kind === 'idle' && (
            <button
              type="button"
              className="catalog-browser__btn catalog-browser__btn--secondary"
              onClick={kickOffCheck}
              data-testid="catalog-browser-refresh"
              title="Pull the latest WLE Standard Forms snapshot from winlink.org via getpat.io"
            >
              Refresh forms…
            </button>
          )}
          <button
            type="button"
            className="catalog-browser__btn"
            onClick={onCancel}
            data-testid="catalog-browser-cancel"
            disabled={refreshStep.kind === 'refreshing'}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------
// Sub-view: refresh flow (Phase 3 — tuxlink-xipa).
// ---------------------------------------------------------------------

interface RefreshPaneProps {
  step: Exclude<RefreshStep, { kind: 'idle' }>;
  onConfirm: () => void;
  onDismiss: () => void;
  'data-testid'?: string;
}

/** Renders one of the non-idle refresh states. Visually replaces the
 *  search + results area while the refresh sub-flow is active; the
 *  parent's footer keeps the Cancel button so Escape + Cancel still work
 *  for closing the whole picker (refreshing state aside — install is
 *  uninterruptible to avoid a half-renamed runtime root). */
function RefreshPane({ step, onConfirm, onDismiss, ...rest }: RefreshPaneProps) {
  const testid = rest['data-testid'];
  return (
    <div
      className="catalog-browser__refresh"
      role="region"
      aria-label="Refresh WLE Standard Forms"
      data-testid={testid}
    >
      {step.kind === 'checking' && (
        <div className="catalog-browser__refresh-status">
          Checking winlink.org for an updated forms snapshot…
        </div>
      )}

      {step.kind === 'up-to-date' && (
        <>
          <div className="catalog-browser__refresh-status" data-testid="catalog-refresh-up-to-date">
            Forms are up to date.
            <div className="catalog-browser__refresh-meta">
              Installed: {step.status.currentVersion ?? '(bundled)'}
              {' · '}Available: {step.status.remoteVersion}
            </div>
          </div>
          <div className="catalog-browser__refresh-actions">
            <button
              type="button"
              className="catalog-browser__btn"
              onClick={onDismiss}
              data-testid="catalog-refresh-dismiss"
            >
              OK
            </button>
          </div>
        </>
      )}

      {step.kind === 'confirming' && (
        <>
          <div className="catalog-browser__refresh-status" data-testid="catalog-refresh-confirm-prompt">
            An updated forms snapshot is available.
            <div className="catalog-browser__refresh-meta">
              Installed: {step.status.currentVersion ?? '(bundled)'}
              {' → '}Available: {step.status.remoteVersion}
            </div>
            <div className="catalog-browser__refresh-detail">
              Download + install will swap the catalog atomically. The prior
              snapshot is kept on disk for one cycle as a manual rollback.
            </div>
          </div>
          <div className="catalog-browser__refresh-actions">
            <button
              type="button"
              className="catalog-browser__btn catalog-browser__btn--primary"
              onClick={onConfirm}
              data-testid="catalog-refresh-confirm"
            >
              Refresh now
            </button>
            <button
              type="button"
              className="catalog-browser__btn"
              onClick={onDismiss}
              data-testid="catalog-refresh-back"
            >
              Not now
            </button>
          </div>
        </>
      )}

      {step.kind === 'refreshing' && (
        <div className="catalog-browser__refresh-status" data-testid="catalog-refresh-installing">
          Installing {step.status.remoteVersion}…
          <div className="catalog-browser__refresh-detail">
            Downloading the archive, extracting templates, and swapping into
            the runtime snapshot. This typically takes 5–10 seconds.
          </div>
        </div>
      )}

      {step.kind === 'done' && (
        <>
          <div className="catalog-browser__refresh-status" data-testid="catalog-refresh-done">
            Refreshed to {step.report.installedVersion} ({step.report.formCount}
            {' '}templates).
            {step.report.prevVersion && (
              <div className="catalog-browser__refresh-meta">
                Previous snapshot ({step.report.prevVersion}) retained as a
                manual rollback for one cycle.
              </div>
            )}
          </div>
          <div className="catalog-browser__refresh-actions">
            <button
              type="button"
              className="catalog-browser__btn"
              onClick={onDismiss}
              data-testid="catalog-refresh-dismiss"
            >
              OK
            </button>
          </div>
        </>
      )}

      {step.kind === 'error' && (
        <>
          <div
            className="catalog-browser__refresh-error"
            role="alert"
            data-testid="catalog-refresh-error"
          >
            Refresh failed: {step.message}
          </div>
          <div className="catalog-browser__refresh-actions">
            <button
              type="button"
              className="catalog-browser__btn"
              onClick={onDismiss}
              data-testid="catalog-refresh-dismiss"
            >
              OK
            </button>
          </div>
        </>
      )}
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
  // Important #5 from the P1 Task 10 code review: the WAI-ARIA tree
  // pattern requires full keyboard nav (Up/Down/Right/Left/Home/End/
  // typeahead), and implementing that for a 250-entry, expand/collapse
  // tree is a significant complexity addition relative to the value for
  // this audience. Per operator memory `feedback_userbase_old_internet_
  // navigation`, the sidebar-ToC + reading-pane pattern (native button
  // semantics + tab/enter/space) is the right default. The buttons here
  // already participate in tab order and respond to Enter/Space; no tree
  // role needed.
  if (folders.length === 0) return null;
  return (
    <ul className="catalog-browser__folders">
      {folders.map((folder) => {
        const isOpen = expanded.has(folder.name);
        return (
          <li key={folder.name} className="catalog-browser__folder">
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
              <ul className="catalog-browser__templates">
                {folder.templates.map((t) => (
                  <li key={t.id} className="catalog-browser__template-row">
                    <button
                      type="button"
                      className="catalog-browser__template-btn"
                      onClick={() => onPick(normalizeCatalogId(t.id))}
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
  // Same Important #5 rationale as FolderTree: dropped the listbox/option
  // ARIA roles because we don't implement listbox keyboard nav. Buttons
  // get tab order + Enter/Space for free.
  if (results.length === 0) {
    return (
      <div className="catalog-browser__empty">No forms match that search.</div>
    );
  }
  return (
    <ul className="catalog-browser__search-results">
      {results.map((t) => (
        <li key={t.id}>
          <button
            type="button"
            className="catalog-browser__template-btn"
            onClick={() => onPick(normalizeCatalogId(t.id))}
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
