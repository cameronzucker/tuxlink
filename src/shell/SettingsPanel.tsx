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

interface SettingsView {
  gps_state: GpsState;
  position_precision: PositionPrecision;
}

/**
 * Frontend mirror of Rust's `config::ArdopUiConfig`. The Rust struct lacks
 * `#[serde(rename_all = "camelCase")]`, so the wire format is snake_case
 * (verified against `src-tauri/src/config.rs` and the Phase 2 modem-commands
 * tests). Keys here must match the Rust field names exactly.
 */
interface ArdopUiConfig {
  binary: string;
  capture_device: string;
  playback_device: string;
  ptt_serial_path: string | null;
  cmd_port: number;
}

const ARDOP_DEFAULT: ArdopUiConfig = {
  binary: 'ardopcf',
  capture_device: '',
  playback_device: '',
  ptt_serial_path: null,
  cmd_port: 8515,
};

export interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
}

export function SettingsPanel({ open, onClose }: SettingsPanelProps) {
  const [gpsState, setGpsState] = useState<GpsState | null>(null);
  const [precision, setPrecision] = useState<PositionPrecision | null>(null);
  const [ardop, setArdop] = useState<ArdopUiConfig>(ARDOP_DEFAULT);
  const [error, setError] = useState<string | null>(null);

  // Load the current values each time the panel opens (live config, not cached).
  useEffect(() => {
    if (!open) return;
    let mounted = true;
    setError(null);
    invoke<SettingsView>('config_read')
      .then((c) => {
        if (!mounted) return;
        setGpsState(c.gps_state);
        setPrecision(c.position_precision);
      })
      .catch(() => {
        if (mounted) setError('Could not load settings.');
      });
    // ARDOP HF section (tuxlink-4ek). config_get_ardop returns the persisted
    // ArdopUiConfig or the struct default (Task 2.2), so the call is safe even
    // pre-wizard. Swallow errors deliberately — the rest of Settings should
    // remain usable if the ARDOP slice fails to read.
    invoke<ArdopUiConfig>('config_get_ardop')
      .then((v) => {
        if (mounted) setArdop(v);
      })
      .catch(() => {
        /* keep ARDOP_DEFAULT */
      });
    return () => {
      mounted = false;
    };
  }, [open]);

  // Persist the ARDOP section. Called on blur of any ARDOP field. Errors are
  // swallowed (network or pre-wizard config-write failures); the in-memory
  // state remains the source of truth until the next open.
  function persistArdop(next: ArdopUiConfig) {
    setArdop(next);
    void invoke('config_set_ardop', { value: next }).catch(() => {
      /* surface via inline error if/when we add per-section error UI */
    });
  }

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
          <h2 className="tux-settings-title">Settings</h2>
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

        {/* ARDOP HF — tuxlink-4ek Phase 5. Persisted via config_set_ardop (Task 2.2);
            Task 3.3 modem_ardop_connect refuses if capture/playback devices are empty. */}
        <fieldset className="tux-settings-group">
          <legend>ARDOP HF</legend>
          <label className="tux-settings-field">
            <span className="tux-settings-field-label">ardopcf binary</span>
            <input
              type="text"
              className="tux-settings-host-input"
              value={ardop.binary}
              onChange={(e) => setArdop({ ...ardop, binary: e.target.value })}
              onBlur={() => persistArdop(ardop)}
            />
          </label>
          <label className="tux-settings-field">
            <span className="tux-settings-field-label">Capture device (ALSA)</span>
            <input
              type="text"
              className="tux-settings-host-input"
              placeholder="plughw:1,0"
              value={ardop.capture_device}
              onChange={(e) => setArdop({ ...ardop, capture_device: e.target.value })}
              onBlur={() => persistArdop(ardop)}
            />
          </label>
          <label className="tux-settings-field">
            <span className="tux-settings-field-label">Playback device (ALSA)</span>
            <input
              type="text"
              className="tux-settings-host-input"
              placeholder="plughw:1,0"
              value={ardop.playback_device}
              onChange={(e) => setArdop({ ...ardop, playback_device: e.target.value })}
              onBlur={() => persistArdop(ardop)}
            />
          </label>
          <label className="tux-settings-field">
            <span className="tux-settings-field-label">PTT serial path (optional — leave blank for VOX)</span>
            <input
              type="text"
              className="tux-settings-host-input"
              placeholder="/dev/ttyUSB0"
              value={ardop.ptt_serial_path ?? ''}
              onChange={(e) =>
                setArdop({
                  ...ardop,
                  ptt_serial_path: e.target.value === '' ? null : e.target.value,
                })
              }
              onBlur={() => persistArdop(ardop)}
            />
          </label>
          <label className="tux-settings-field">
            <span className="tux-settings-field-label">Cmd port</span>
            <input
              type="number"
              className="tux-settings-host-input"
              value={ardop.cmd_port}
              onChange={(e) =>
                setArdop({
                  ...ardop,
                  cmd_port: parseInt(e.target.value, 10) || 8515,
                })
              }
              onBlur={() => persistArdop(ardop)}
            />
          </label>
        </fieldset>
      </div>
    </div>
  );
}
