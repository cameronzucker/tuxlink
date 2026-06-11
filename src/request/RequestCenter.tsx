// RequestCenter — full-viewport request-assembly workspace for the Winlink
// catalog (bd-tuxlink-eymu).
//
// It is the single owner of the catalog load: it calls `useCatalog()` once and
// passes `entries` down to every consumer (home sections, master-detail browse,
// global search, GRIB form) — none of those re-fetch the catalog. The content
// region routes between four views (home / browse / search / grib) over a shared
// request basket; basket items dispatch per rail on "Send all" — cms items
// collapse into one catalog inquiry to the CMS, while each Saildocs item sends
// its own GRIB request.
//
// The workspace is a full-screen overlay, not a dismissable popover: backdrop
// clicks do NOT close it. ESC and the Close button are the only close paths.

import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalog } from '../catalog/useCatalog';
import { useRequestBasket, dispatchBasket, type DispatchResult } from './basket';
import { buildSections, type CardAction } from './sections';
import { gridToLatLon, latLonToUsState } from './geo';
import { usStateName } from './usStateName';
import { Icon, type IconName } from './icons';
import { CatalogBrowse } from './CatalogBrowse';
import { GribForm } from './GribForm';
import './RequestCenter.css';

// Card id → glyph for the feat cards (hero) and chips (national grids).
// Unmapped ids fall back to `info`.
const CARD_ICONS: Record<string, IconName> = {
  'loc-zone-forecast': 'weather',
  'loc-radar': 'radar',
  'loc-marine': 'wave',
  'prop-forecast': 'prop',
  'prop-solar': 'sun',
  'prop-aurora': 'aurora',
  'nearby-gateways': 'tower',
  'nearby-winlink-info': 'info',
};
const cardIcon = (id: string): IconName => CARD_ICONS[id] ?? 'info';

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

  // Resolve the chip's state suffix the same way buildSections derives geo:
  // grid → lat/lon → USPS → full name. null when the grid is unset/unresolvable
  // or maps to no U.S. state (the chip then shows no suffix). Keyed on grid.
  const stateName = useMemo(() => {
    if (!grid) return null;
    const latLon = gridToLatLon(grid);
    if (!latLon) return null;
    const stateCode = latLonToUsState(latLon.lat, latLon.lon);
    return stateCode ? usStateName(stateCode) : null;
  }, [grid]);

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
          <div className="request-header__lead hd-title">
            <span className="request-header__glyph g" aria-hidden="true">
              <Icon name="inbox" size={19} />
            </span>
            <h2 className="request-header__title">Request Center</h2>
            <span
              className="request-location-chip loc"
              data-testid="request-center-location"
              aria-label={`Origin: ${locationLabel}`}
            >
              <Icon name="pin" size={14} className="pin" />
              <span>{locationLabel}</span>
              {stateName && <span className="sub"> · {stateName}</span>}
            </span>
          </div>
          <div className="request-header__search">
            <Icon name="search" size={15} className="request-search__icon" />
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
            <Icon name="close" size={16} />
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
              <div className="request-sections content-inner">
                {/* HERO — the location ('location' kind) section. An amber-edged
                    panel; the primary card (zone forecast) renders as the
                    full-width .zone hero card; the rest (radar, marine) render
                    in a .locgrid of .feat cards. Each card shows its .meta
                    mono line (catalog target / filename). */}
                {sections
                  .filter((section) => section.kind === 'location')
                  .map((section) => {
                    const primaryCard = section.cards.find((c) => c.primary) ?? null;
                    const supportingCards = section.cards.filter((c) => !c.primary);
                    return (
                      <section
                        key={section.id}
                        className="request-hero hero"
                        data-testid={`request-section-${section.id}`}
                        aria-label={section.title}
                      >
                        <div className="hero-h">
                          <span className="pin" aria-hidden="true">
                            <Icon name="pin" size={17} />
                          </span>
                          <h4>For your location</h4>
                          {grid && (
                            <span className="where">
                              {grid}
                              {stateName ? ` · ${stateName}` : ''}
                            </span>
                          )}
                        </div>

                        {/* Primary zone card — the most-local product */}
                        {primaryCard && (
                          <article
                            className="zone"
                            data-testid={`request-card-${primaryCard.id}`}
                          >
                            <span className="zi" aria-hidden="true">
                              <Icon name={cardIcon(primaryCard.id)} size={23} />
                            </span>
                            <div className="request-card__body">
                              <div className="zn request-card__label">{primaryCard.label}</div>
                              {primaryCard.description && (
                                <div className="zd request-card__desc">{primaryCard.description}</div>
                              )}
                              {primaryCard.meta && (
                                <div className="zmeta">{primaryCard.meta}</div>
                              )}
                            </div>
                            <span className="za">
                              <button
                                type="button"
                                className="badd"
                                onClick={() => runAction(primaryCard.action, primaryCard.label)}
                                aria-label={`Add ${primaryCard.label} to request`}
                              >
                                <Icon name="plus" size={15} />
                                Add request
                              </button>
                            </span>
                          </article>
                        )}

                        {/* Supporting cards — radar + marine in a 2-column locgrid */}
                        {supportingCards.length > 0 && (
                          <div className="locgrid">
                            {supportingCards.map((card) => {
                              const isAdd = card.action.kind === 'addCms';
                              return (
                                <article
                                  key={card.id}
                                  className="feat"
                                  data-testid={`request-card-${card.id}`}
                                >
                                  <span className="fi" aria-hidden="true">
                                    <Icon name={cardIcon(card.id)} size={20} />
                                  </span>
                                  <div className="request-card__body">
                                    <div className="fn request-card__label">{card.label}</div>
                                    {card.description && (
                                      <div className="fd request-card__desc">{card.description}</div>
                                    )}
                                    {card.meta && (
                                      <div className="fmeta">{card.meta}</div>
                                    )}
                                  </div>
                                  <span className="fa">
                                    {isAdd ? (
                                      <button
                                        type="button"
                                        className="iadd"
                                        onClick={() => runAction(card.action, card.label)}
                                        aria-label={`Add ${card.label} to request`}
                                      >
                                        <Icon name="plus" size={15} />
                                      </button>
                                    ) : (
                                      <button
                                        type="button"
                                        className="iadd"
                                        onClick={() => runAction(card.action, card.label)}
                                        aria-label={`Open ${card.label}`}
                                      >
                                        <Icon name="arrow" size={15} />
                                      </button>
                                    )}
                                  </span>
                                </article>
                              );
                            })}
                          </div>
                        )}

                      </section>
                    );
                  })}

                {/* CHIP GRIDS — the 'national' sections. Each card is a compact
                    chip (icon tile + title + one-line description + the catalog
                    filename/category in mono + an add/→ control). */}
                {sections
                  .filter((section) => section.kind === 'national')
                  .map((section) => (
                    <section
                      key={section.id}
                      className="request-section"
                      data-testid={`request-section-${section.id}`}
                      aria-label={section.title}
                    >
                      <div className="request-subt subt">
                        <span className="bar" aria-hidden="true" />
                        {section.title}
                      </div>
                      <div className="request-chips chips">
                        {section.cards.map((card) => {
                          const isAdd = card.action.kind === 'addCms';
                          const sub =
                            card.action.kind === 'addCms'
                              ? card.action.filename
                              : card.action.category;
                          return (
                            <article
                              key={card.id}
                              className="chip"
                              data-testid={`request-card-${card.id}`}
                            >
                              <span className="ci" aria-hidden="true">
                                <Icon name={cardIcon(card.id)} size={20} />
                              </span>
                              <div className="cb">
                                <div className="ct request-card__label">{card.label}</div>
                                {card.description && (
                                  <div className="cd request-card__desc">{card.description}</div>
                                )}
                                <span className="cs">{sub}</span>
                              </div>
                              <button
                                type="button"
                                className="cadd"
                                onClick={() => runAction(card.action, card.label)}
                                aria-label={
                                  isAdd
                                    ? `Add ${card.label} to request`
                                    : `Open ${card.label}`
                                }
                              >
                                <Icon name={isAdd ? 'plus' : 'arrow'} size={16} />
                              </button>
                            </article>
                          );
                        })}
                      </div>
                    </section>
                  ))}

                {/* Reveals — drop into the full master-detail catalog browser
                    (Task D1) and the Saildocs GRIB request form (Task D3). */}
                <div className="request-reveals reveals">
                  <button
                    type="button"
                    className="request-rev rev request-browse-reveal__button"
                    data-testid="request-browse-reveal"
                    onClick={() => {
                      setBrowseCategory(null);
                      setView('browse');
                    }}
                  >
                    <Icon name="list" size={16} className="lead" />
                    Browse full catalog
                    <span className="m">by category</span>
                  </button>
                  <button
                    type="button"
                    className="request-rev rev request-browse-reveal__button"
                    data-testid="request-grib-reveal"
                    onClick={() => setView('grib')}
                  >
                    <Icon name="map" size={16} className="lead" />
                    GRIB
                    <span className="m">by area</span>
                  </button>
                </div>
              </div>
            )}
          </div>

          <aside className="request-basket" data-testid="request-basket" aria-label="Request basket">
            {/* basket header */}
            <div className="bk-head">
              <Icon name="basket" size={18} />
              <h4>
                Request basket
                {!basket.isEmpty && (
                  <span className="ct">{basket.items.length}</span>
                )}
              </h4>
            </div>

            {/* scrollable list region */}
            <div className="bk-list">
              {/* empty placeholder — only when no items and not in done state */}
              {basket.isEmpty && sendState.kind !== 'done' && (
                <div className="bk-empty">
                  <div className="ring">
                    <Icon name="basket" size={24} />
                  </div>
                  <p>Your basket is empty. Add requests from the cards or browse.</p>
                </div>
              )}

              {/* item list — visible whenever items exist */}
              {!basket.isEmpty && (
                <ul className="request-basket__list">
                  {basket.items.map((item) => (
                    <li
                      key={item.id}
                      className="request-basket__item bk-item"
                      data-testid={`basket-item-${item.id}`}
                    >
                      <span className={`ic ${item.rail === 'saildocs' ? 'sail' : 'cms'}`}>
                        <Icon name={item.rail === 'saildocs' ? 'map' : 'check'} size={15} />
                      </span>
                      <span className="bd">
                        <span className="lb">{item.label}</span>
                        <span className="rl">
                          {item.rail === 'saildocs' ? 'Saildocs' : 'CMS inquiry'}
                        </span>
                      </span>
                      <button
                        type="button"
                        className="request-basket__remove rm"
                        data-testid={`basket-remove-${item.id}`}
                        onClick={() => basket.remove(item.id)}
                        aria-label={`Remove ${item.label}`}
                        title="Remove"
                      >
                        <Icon name="trash" size={14} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}

              {/* result block — visible when send completed */}
              {sendState.kind === 'done' &&
                (() => {
                  const { result } = sendState;
                  const okSaildocs = result.saildocs.filter((e) => e.ok);
                  return (
                    <div
                      className="request-basket__result result"
                      data-testid="request-basket-result"
                      role="status"
                    >
                      {result.cms?.ok && (
                        <p className="request-basket__result-line ln">
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
                        <p className="request-basket__result-line ln">
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
            </div>

            {/* footer: summary + send + arrival note */}
            <div className="bk-foot">
              {!basket.isEmpty && (
                <p className="request-basket__summary bk-sum" data-testid="request-basket-summary">
                  {basketSummary(basket.cmsFilenames.length, basket.saildocsItems.length)}
                </p>
              )}

              <button
                type="button"
                className="request-basket__send send"
                data-testid="request-basket-send"
                onClick={sendAll}
                disabled={basket.isEmpty || sendState.kind === 'sending'}
              >
                {sendState.kind === 'sending' ? (
                  'Sending…'
                ) : (
                  <>
                    Send all
                    <Icon name="arrow" size={15} />
                  </>
                )}
              </button>

              {sendState.kind !== 'done' && (
                <p className="snote">
                  Responses arrive in your Inbox after the next connect.
                </p>
              )}
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
