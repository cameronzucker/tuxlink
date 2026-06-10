/**
 * MapTileSettingsPanel — inline (in-webview) overlay that hosts the LAN
 * map-tile source configuration (tuxlink-a1cc / dyop; design §8.7). NOT a
 * separate OS window (operator pet-peeve: no window clutter; Compose is the
 * lone window exception).
 *
 * This is the one reachable home for the dyop tile backend: without a way to
 * configure a source, the gatekeeper never gets one, so the map zoom ceiling
 * never rises (design §8.6) and the tile layer never engages on any map. The
 * overlay reuses the shared `tux-settings-*` dialog chrome (backdrop, panel,
 * header, close button — including the compact-mode touch-target rule) and
 * wraps the standalone <MapTileSourceSettings/> section.
 *
 * Opened by AppShell from Tools → Settings → Map tiles… (see dispatchMenuAction
 * `openMapTileSettings`).
 */

import { useEffect } from 'react';
import { MapTileSourceSettings } from './MapTileSourceSettings';
// The shared `.tux-settings-*` overlay chrome (backdrop position:fixed, centered
// panel, header/close) lives in SettingsPanel.css. This panel is lazy-loaded into
// its OWN chunk, so it MUST import that chrome itself — relying on the GPS
// SettingsPanel chunk being loaded first left the backdrop unpositioned and the
// overlay rendered inline, shoving the app under the bottom bar (tuxlink-jgom).
import '../shell/SettingsPanel.css';
import './MapTileSettingsPanel.css';

export interface MapTileSettingsPanelProps {
  open: boolean;
  onClose: () => void;
}

export function MapTileSettingsPanel({ open, onClose }: MapTileSettingsPanelProps) {
  // Esc closes (matches the click-away/Esc affordances elsewhere in the chrome).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      className="tux-settings-backdrop"
      data-testid="map-tile-settings-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-settings-panel tux-map-tile-settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Map tiles"
        data-testid="map-tile-settings-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-settings-header">
          <h2 className="tux-settings-title">Map tiles</h2>
          <button
            type="button"
            className="tux-settings-close"
            data-testid="map-tile-settings-close"
            aria-label="Close map tile settings"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        <MapTileSourceSettings />
      </div>
    </div>
  );
}
