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
import { useRequestBasket } from './basket';
import { buildSections, type CardAction } from './sections';
import './RequestCenter.css';

export interface RequestCenterProps {
  onClose: () => void;
  initialView?: 'home' | 'browse' | 'grib';
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

            {/* Browse pane — minimal placeholder; the real 3-pane CatalogBrowse
                replaces this in Task D1. */}
            {view === 'browse' && (
              <div data-testid="request-browse" data-category={browseCategory ?? ''} />
            )}

            {/* Request-first home sections. Home renders only when not loading,
                no error, entries present, and the home view is active. */}
            {view === 'home' && !loading && !error && entries && entries.length > 0 && (
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
                          data-testid={`request-card-${card.label}`}
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
              </div>
            )}
          </div>

          <aside className="request-basket" data-testid="request-basket" aria-label="Request basket">
            {/* Labelled item list only; the full basket UI (remove, footer,
                Send) is Task E1. */}
            <ul className="request-basket__list">
              {basket.items.map((item) => (
                <li key={item.id} data-testid={`basket-item-${item.id}`}>
                  {item.label}
                </li>
              ))}
            </ul>
          </aside>
        </div>
      </div>
    </div>
  );
}
