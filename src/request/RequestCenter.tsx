// RequestCenter — full-viewport overlay workspace for assembling CMS requests
// (bd-tuxlink-eymu). Task C1: the overlay shell.
//
// This is the single owner of the catalog load: it calls `useCatalog()` once
// and will pass `entries` down to the home sections / browse / search consumers
// that arrive in later tasks (C2+). For C1 the content region is a placeholder;
// the shell only establishes the chrome (location chip, search, basket region)
// and the three catalog-load states (loading / empty / error).
//
// Unlike CatalogRequestPanel, the Request Center is a full-screen workspace, not
// a dismissable popover: backdrop clicks do NOT close it. ESC and the Close
// button are the only close paths.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalog } from '../catalog/useCatalog';
import { useRequestBasket, dispatchBasket, type DispatchResult } from './basket';
import { buildSections, type CardAction } from './sections';
import { CatalogBrowse } from './CatalogBrowse';
import { GribForm } from './GribForm';
import './RequestCenter.css';

export interface RequestCenterProps {
  onClose: () => void;
  initialView?: 'home' | 'browse' | 'grib';
}

// Send-all state machine (Task E1). `done` carries the DispatchResult so the
// result region renders the per-rail summary + errors directly from it.
type SendState =
  | { kind: 'idle' }
  | { kind: 'sending' }
  | { kind: 'done'; result: DispatchResult };

// Per-rail footer summary. The CMS rail collapses to ONE inquiry message
// regardless of how many filenames; saildocs is one request each. Clauses for
// absent rails are omitted; counts pluralize correctly. Present-indicative,
// formal, no first person.
function basketSummary(cmsCount: number, saildocsCount: number): string {
  const total = cmsCount + saildocsCount;
  const clauses: string[] = [`${total} ${total === 1 ? 'request' : 'requests'}`];
  if (cmsCount > 0) clauses.push('1 inquiry message to the CMS');
  if (saildocsCount > 0) {
    clauses.push(`${saildocsCount} Saildocs ${saildocsCount === 1 ? 'request' : 'requests'}`);
  }
  return clauses.join(' · ');
}

export function RequestCenter({ onClose, initialView = 'home' }: RequestCenterProps) {
  // Single catalog-load owner (adrev #3). entries === null while loading.
  const { entries, loading, error } = useCatalog();

  // Basket ownership lands here (C2): card add controls dispatch into it; the
  // full basket UI (remove, per-rail footer, Send) is Task E1.
  const basket = useRequestBasket();

  // Full-precision home grid for the location chip. null until config_read
  // resolves with a grid; stays null on no-grid or read failure → neutral chip
  // (adrev #9 — never "Near null"/"Near undefined").
  const [grid, setGrid] = useState<string | null>(null);
  const [search, setSearch] = useState('');

  // Send state machine (Task E1). `done` carries the DispatchResult so the
  // result region renders from it (per-rail summary + errors). `sending`
  // disables the Send button to prevent a double-dispatch.
  const [sendState, setSendState] = useState<SendState>({ kind: 'idle' });

  // View routing. `initialView` (the C1 nit) seeds the view; openBrowse swaps
  // to the browse pane (a placeholder until the real 3-pane CatalogBrowse in
  // Task D1) and records which category it was opened at.
  const [view, setView] = useState<'home' | 'browse' | 'grib'>(initialView);
  const [browseCategory, setBrowseCategory] = useState<string | null>(null);

  // Curated request-first home sections, recomputed when the catalog or the
  // resolved grid changes. Pure (sections.ts) — no React/Tauri inside.
  const sections = useMemo(
    () => (entries ? buildSections(entries, grid) : []),
    [entries, grid],
  );

  const runAction = (action: CardAction, label: string) => {
    if (action.kind === 'addCms') {
      basket.add({
        id: `cms:${action.filename}`,
        label,
        rail: 'cms',
        filename: action.filename,
      });
    } else {
      // openBrowse navigates; it never mutates the basket.
      setBrowseCategory(action.category);
      setView('browse');
    }
  };

  // Escape closes the overlay. Document-level so it fires regardless of which
  // element inside the workspace holds focus.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onClose]);

  useEffect(() => {
    // `mounted` guard matches the codebase house pattern (useCatalog, AppShell):
    // don't setState if config_read resolves after the overlay unmounts.
    let mounted = true;
    invoke<{ grid: string | null }>('config_read')
      .then((c) => {
        if (mounted && c?.grid) setGrid(c.grid);
      })
      .catch(() => {
        // Leave grid null → neutral chip. Never surface a read error as a location.
      });
    return () => {
      mounted = false;
    };
  }, []);

  const locationLabel = grid ? `Near ${grid}` : 'Location not set';

  // Global search (Task D2). A non-empty trimmed needle shows cross-category
  // search results, overriding the current view (home/browse/grib). Cleared →
  // the view-based content returns. The header search input stays visible in
  // all states. Reuses CatalogBrowse's search mode + the shared cms add path.
  const searchActive = search.trim().length > 0;

  const addCms = (e: { filename: string; description: string }) =>
    basket.add({
      id: `cms:${e.filename}`,
      label: e.description || e.filename,
      rail: 'cms',
      filename: e.filename,
    });

  // "Send all" (Task E1). Dispatch both rails via dispatchBasket (never throws,
  // runs Promise.allSettled internally). Capture the cms filenames BEFORE
  // dispatch so the post-dispatch clear targets the right ids even if the basket
  // changes meanwhile. adrev #4 keep/clear: clear only the succeeded rail.
  const sendAll = async () => {
    if (basket.isEmpty || sendState.kind === 'sending') return;
    const cmsFilenamesAtSend = [...basket.cmsFilenames];
    setSendState({ kind: 'sending' });
    const result = await dispatchBasket(basket.items);

    if (result.cms?.ok) {
      for (const filename of cmsFilenamesAtSend) basket.remove(`cms:${filename}`);
    }
    for (const entry of result.saildocs) {
      if (entry.ok) basket.remove(entry.item.id);
    }

    setSendState({ kind: 'done', result });
  };

  return (
    <div className="request-overlay" data-testid="request-overlay" role="dialog" aria-label="Request Center">
      <div className="request-workspace">
        <header className="request-header">
          <div className="request-header__lead">
            <h2 className="request-header__title">Request Center</h2>
            <span
              className="request-location-chip"
              data-testid="request-center-location"
              aria-label={`Origin: ${locationLabel}`}
            >
              {locationLabel}
            </span>
          </div>
          <div className="request-header__search">
            <input
              type="search"
              className="request-search"
              data-testid="request-search"
              placeholder="Search the catalog…"
              aria-label="Search the catalog"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          <button
            type="button"
            className="request-close"
            data-testid="request-close"
            onClick={onClose}
            aria-label="Close Request Center"
            title="Close"
          >
            ✕
          </button>
        </header>

        <div className="request-body">
          <div className="request-content" data-testid="request-content">
            {loading && (
              <div className="request-state request-state--loading" data-testid="request-catalog-loading">
                Loading catalog…
              </div>
            )}
            {error && (
              <div className="request-state request-state--error" data-testid="request-catalog-error">
                Failed to load catalog: {error}
              </div>
            )}
            {!loading && !error && entries && entries.length === 0 && (
              <div className="request-state request-state--empty" data-testid="request-catalog-empty">
                No catalog items available.
              </div>
            )}

            {/* Global search results (Task D2). When the header search needle
                is non-empty, CatalogBrowse renders a flat cross-category
                results list — overriding home/browse/grib. Cleared → the view
                content (below) returns. Gated by the same load-guard prefix.
                Reuses CatalogBrowse's search mode + the shared cms add path. */}
            {searchActive && !loading && !error && entries && entries.length > 0 && (
              <CatalogBrowse
                entries={entries}
                searchQuery={search}
                onAddCms={addCms}
                addedFilenames={new Set(basket.cmsFilenames)}
                onBack={() => setSearch('')}
              />
            )}

            {/* Browse pane — the 3-pane master-detail CatalogBrowse (Task D1).
                Gated by the same load-guard prefix as home so it can't render
                against entries===null. CatalogBrowse receives `entries` as a
                prop — it does NOT call useCatalog() (adrev #3: RequestCenter is
                the single catalog-load owner). */}
            {!searchActive && view === 'browse' && !loading && !error && entries && (
              <CatalogBrowse
                entries={entries}
                initialCategory={browseCategory}
                onAddCms={addCms}
                addedFilenames={new Set(basket.cmsFilenames)}
                onBack={() => setView('home')}
              />
            )}

            {/* GRIB pane — the Saildocs GRIB request form (Task D3). Unlike the
                cms cards, "Add to request" creates a `saildocs` BasketItem
                carrying the full GribRequest (NOT an immediate send — dispatch
                is Task E1's job). The id `saildocs:<json>` lets two distinct
                requests coexist while re-adding an identical request dedups to a
                no-op. Same load-guard prefix as browse/home for symmetry. */}
            {!searchActive && view === 'grib' && !loading && !error && entries && (
              <GribForm
                onAddSaildocs={(request) =>
                  basket.add({
                    id: `saildocs:${JSON.stringify(request)}`,
                    label: request.subject || 'GRIB request',
                    rail: 'saildocs',
                    request,
                  })
                }
                onBack={() => setView('home')}
              />
            )}

            {/* Request-first home sections. Home renders only when not loading,
                no error, entries present, and the home view is active. */}
            {!searchActive && view === 'home' && !loading && !error && entries && entries.length > 0 && (
              <div className="request-sections">
                {sections.map((section) => (
                  <section
                    key={section.id}
                    className="request-section"
                    data-testid={`request-section-${section.id}`}
                    aria-label={section.title}
                  >
                    <h3 className="request-section__title">{section.title}</h3>
                    <div className="request-section__cards">
                      {section.cards.map((card) => (
                        <article
                          key={card.id}
                          className="request-card"
                          data-testid={`request-card-${card.id}`}
                        >
                          <div className="request-card__body">
                            <span className="request-card__label">{card.label}</span>
                            {card.description && (
                              <span className="request-card__desc">{card.description}</span>
                            )}
                          </div>
                          <button
                            type="button"
                            className="request-card__action"
                            onClick={() => runAction(card.action, card.label)}
                            aria-label={
                              card.action.kind === 'addCms'
                                ? `Add ${card.label} to request`
                                : `Open ${card.label}`
                            }
                          >
                            {card.action.kind === 'addCms' ? 'Add' : 'Open'}
                          </button>
                        </article>
                      ))}
                    </div>
                  </section>
                ))}

                {/* Browse-everything reveal — drops into the full master-detail
                    catalog browser (Task D1). Opens with no category preselected
                    so the browser defaults to its first category. */}
                <div className="request-browse-reveal">
                  <button
                    type="button"
                    className="request-browse-reveal__button"
                    data-testid="request-browse-reveal"
                    onClick={() => {
                      setBrowseCategory(null);
                      setView('browse');
                    }}
                  >
                    Browse full catalog by category →
                  </button>
                </div>

                {/* GRIB-by-area reveal — drops into the Saildocs GRIB request
                    form (Task D3). Adds a saildocs request to the basket; the
                    send happens later from the basket rail (Task E1). */}
                <div className="request-browse-reveal">
                  <button
                    type="button"
                    className="request-browse-reveal__button"
                    data-testid="request-grib-reveal"
                    onClick={() => setView('grib')}
                  >
                    More: GRIB by area →
                  </button>
                </div>
              </div>
            )}
          </div>

          <aside className="request-basket" data-testid="request-basket" aria-label="Request basket">
            <ul className="request-basket__list">
              {basket.items.map((item) => (
                <li
                  key={item.id}
                  className="request-basket__item"
                  data-testid={`basket-item-${item.id}`}
                >
                  <span className="request-basket__item-label">{item.label}</span>
                  <button
                    type="button"
                    className="request-basket__remove"
                    data-testid={`basket-remove-${item.id}`}
                    onClick={() => basket.remove(item.id)}
                    aria-label={`Remove ${item.label}`}
                    title="Remove"
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>

            {!basket.isEmpty && (
              <p className="request-basket__summary" data-testid="request-basket-summary">
                {basketSummary(basket.cmsFilenames.length, basket.saildocsItems.length)}
              </p>
            )}

            <button
              type="button"
              className="request-basket__send"
              data-testid="request-basket-send"
              onClick={sendAll}
              disabled={basket.isEmpty || sendState.kind === 'sending'}
            >
              {sendState.kind === 'sending' ? 'Sending…' : 'Send all'}
            </button>

            {sendState.kind === 'done' &&
              (() => {
                const { result } = sendState;
                const okSaildocs = result.saildocs.filter((e) => e.ok);
                return (
                  <div
                    className="request-basket__result"
                    data-testid="request-basket-result"
                    role="status"
                  >
                    {result.cms?.ok && (
                      <p className="request-basket__result-line">
                        Queued 1 inquiry message to the CMS
                        {result.cms.mid ? ` (MID ${result.cms.mid}).` : '.'}
                      </p>
                    )}
                    {result.cms && !result.cms.ok && (
                      <p className="request-basket__result-error">
                        CMS failed: {result.cms.error}
                      </p>
                    )}
                    {okSaildocs.length > 0 && (
                      <p className="request-basket__result-line">
                        Queued {okSaildocs.length} Saildocs{' '}
                        {okSaildocs.length === 1 ? 'request' : 'requests'}.
                      </p>
                    )}
                    {result.saildocs
                      .filter((e) => !e.ok)
                      .map((e) => (
                        <p key={e.item.id} className="request-basket__result-error">
                          Saildocs failed: {e.error}
                        </p>
                      ))}
                    {(result.cms?.ok || okSaildocs.length > 0) && (
                      <p className="request-basket__result-note">
                        Responses arrive in your Inbox after the next connect.
                      </p>
                    )}
                  </div>
                );
              })()}
          </aside>
        </div>
      </div>
    </div>
  );
}
