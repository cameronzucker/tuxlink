/**
 * SettingsPanel — inline (in-webview) Settings overlay for the GPS privacy
 * controls (tuxlink-39b). NOT a separate OS window (operator pet-peeve: no
 * window clutter; Compose is the lone window exception).
 *
 * Closes the gap found in the post-merge smoke of #113: gps_state +
 * position_precision were ENFORCED but unreachable — the Tools→Settings menu
 * items were dead no-op stubs and no backend setter existed. This panel reads
 * the live config (config_read) and writes via config_set_privacy.
 *
 * Opened by AppShell from the three Tools→Settings GPS/privacy menu items
 * (see dispatchMenuAction `openSettings`).
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { GpsState, PositionPrecision } from './useStatus';
import './SettingsPanel.css';

const GPS_STATE_OPTIONS: { value: GpsState; label: string; help: string }[] = [
  {
    value: 'BroadcastAtPrecision',
    label: 'Broadcast at precision',
    help: 'GPS position is read and broadcast on air at the precision below (default).',
  },
  {
    value: 'LocalUiOnly',
    label: 'Local display only',
    help: 'GPS is read but never broadcast — the configured grid is sent on air instead.',
  },
  {
    value: 'Off',
    label: 'Off',
    help: 'GPS is not read at all; the configured grid is used.',
  },
];

const PRECISION_OPTIONS: { value: PositionPrecision; label: string; help: string }[] = [
  {
    value: 'FourCharGrid',
    label: '4-char grid (~1°)',
    help: 'Coarser location; recommended for privacy (default).',
  },
  {
    value: 'SixCharGrid',
    label: '6-char grid (~5 km)',
    help: 'Finer location; opt-in.',
  },
];

interface PrivacyView {
  gps_state: GpsState;
  position_precision: PositionPrecision;
}

export interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
}

export function SettingsPanel({ open, onClose }: SettingsPanelProps) {
  const [gpsState, setGpsState] = useState<GpsState | null>(null);
  const [precision, setPrecision] = useState<PositionPrecision | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Load the current values each time the panel opens (live config, not cached).
  useEffect(() => {
    if (!open) return;
    let mounted = true;
    setError(null);
    invoke<PrivacyView>('config_read')
      .then((c) => {
        if (!mounted) return;
        setGpsState(c.gps_state);
        setPrecision(c.position_precision);
      })
      .catch(() => {
        if (mounted) setError('Could not load settings.');
      });
    return () => {
      mounted = false;
    };
  }, [open]);

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

  // Persist both fields together (config_set_privacy takes the full privacy
  // state). Optimistically reflect the choice; surface a failure inline.
  async function persist(next: { gpsState: GpsState; precision: PositionPrecision }) {
    setGpsState(next.gpsState);
    setPrecision(next.precision);
    setError(null);
    try {
      await invoke('config_set_privacy', {
        gpsState: next.gpsState,
        positionPrecision: next.precision,
      });
    } catch {
      setError('Could not save settings.');
    }
  }

  return (
    <div className="tux-settings-backdrop" data-testid="settings-backdrop" onClick={onClose}>
      <div
        className="tux-settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        data-testid="settings-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-settings-header">
          <h2 className="tux-settings-title">Settings · GPS &amp; Privacy</h2>
          <button
            type="button"
            className="tux-settings-close"
            data-testid="settings-close"
            aria-label="Close settings"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        {error && (
          <div className="tux-settings-error" role="alert">
            {error}
          </div>
        )}

        <fieldset className="tux-settings-group">
          <legend>GPS state</legend>
          {GPS_STATE_OPTIONS.map((o) => (
            <label key={o.value} className="tux-settings-opt">
              <input
                type="radio"
                name="gps-state"
                value={o.value}
                checked={gpsState === o.value}
                onChange={() => precision && persist({ gpsState: o.value, precision })}
              />
              <span className="tux-settings-opt-text">
                <span className="tux-settings-opt-label">{o.label}</span>
                <span className="tux-settings-opt-help">{o.help}</span>
              </span>
            </label>
          ))}
        </fieldset>

        <fieldset className="tux-settings-group">
          <legend>Broadcast precision</legend>
          {PRECISION_OPTIONS.map((o) => (
            <label key={o.value} className="tux-settings-opt">
              <input
                type="radio"
                name="precision"
                value={o.value}
                checked={precision === o.value}
                onChange={() => gpsState && persist({ gpsState, precision: o.value })}
              />
              <span className="tux-settings-opt-text">
                <span className="tux-settings-opt-label">{o.label}</span>
                <span className="tux-settings-opt-help">{o.help}</span>
              </span>
            </label>
          ))}
        </fieldset>
      </div>
    </div>
  );
}
