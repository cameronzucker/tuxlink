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

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useCatalog } from '../catalog/useCatalog';
import './RequestCenter.css';

export interface RequestCenterProps {
  onClose: () => void;
  initialView?: 'home' | 'browse' | 'grib';
}

export function RequestCenter({ onClose }: RequestCenterProps) {
  // Single catalog-load owner (adrev #3). entries === null while loading.
  const { entries, loading, error } = useCatalog();

  // Full-precision home grid for the location chip. null until config_read
  // resolves with a grid; stays null on no-grid or read failure → neutral chip
  // (adrev #9 — never "Near null"/"Near undefined").
  const [grid, setGrid] = useState<string | null>(null);
  const [search, setSearch] = useState('');

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
    invoke<{ grid: string | null }>('config_read')
      .then((c) => {
        if (c?.grid) setGrid(c.grid);
      })
      .catch(() => {
        // Leave grid null → neutral chip. Never surface a read error as a location.
      });
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
            {/* Sections / browse / grib land in later tasks (C2+). */}
          </div>

          <aside className="request-basket" data-testid="request-basket" aria-label="Request basket">
            {/* Basket contents land in a later task. */}
          </aside>
        </div>
      </div>
    </div>
  );
}
