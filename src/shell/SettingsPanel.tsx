/**
 * SettingsPanel — inline (in-webview) Settings overlay (tuxlink-39b, redesigned
 * tuxlink-b95x). NOT a separate OS window, and NO nested windows: one roomy
 * surface sized like Find a Station (width min(1180px,96vw), max-height 92vh)
 * with a section nav on the left and an INLINE content pane on the right. Each
 * section renders in the pane; nothing pops a second window (the operator's WLE
 * pet-peeve: no window clutter; Compose is the lone window exception).
 *
 * Opened by AppShell from the Tools→Settings menu items (dispatchMenuAction
 * `openSettings`). GPS state + precision persist via config_set_privacy; the
 * Location & GPS pane renders the shared GpsSourcePicker inline.
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { GpsState, PositionPrecision } from './useStatus';
import { LocationSettingsPane } from '../location/LocationSettingsPane';
import { AprsSettings } from '../aprs/AprsSettings';
import { FormSequenceSettings } from '../forms/FormSequenceSettings';
import { OfflineMapsSettings } from '../map/OfflineMapsSettings';
import { IdentitiesSettings } from './IdentitiesSettings';
import { WinlinkAccountSettings } from './WinlinkAccountSettings';
import { MailboxSettings } from './MailboxSettings';
import { WindowSettings } from './WindowSettings';
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
  review_inbound_before_download: boolean;
}

export type SectionId =
  | 'identities'
  | 'account'
  | 'location'
  | 'gpsstate'
  | 'aprs'
  | 'forms'
  | 'maps'
  | 'mailbox'
  | 'window';

const NAV: { group: string; items: { id: SectionId; label: string }[] }[] = [
  {
    group: 'Profile',
    items: [
      { id: 'identities', label: 'Identities' },
      // tuxlink-vfb3: CMS account credential management (password change + the
      // keyring-only re-enter recovery).
      { id: 'account', label: 'Winlink Account' },
      { id: 'location', label: 'Location & GPS' },
      { id: 'gpsstate', label: 'GPS state & privacy' },
    ],
  },
  {
    group: 'On air',
    items: [
      { id: 'aprs', label: 'APRS tactical chat' },
      { id: 'forms', label: 'Form sequence numbers' },
    ],
  },
  {
    group: 'App',
    items: [
      { id: 'maps', label: 'Offline maps' },
      { id: 'mailbox', label: 'Mailbox' },
      { id: 'window', label: 'Window' },
    ],
  },
];

export interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  /** Optional initial section (defaults to Location & GPS). */
  initialSection?: SectionId;
}

export function SettingsPanel({ open, onClose, initialSection = 'location' }: SettingsPanelProps) {
  const [active, setActive] = useState<SectionId>(initialSection);
  const [gpsState, setGpsState] = useState<GpsState | null>(null);
  const [precision, setPrecision] = useState<PositionPrecision | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Load gps_state/precision each time the panel opens (live config, not cached).
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
    return () => {
      mounted = false;
    };
  }, [open]);

  // Esc closes.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

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

  const sectionTitle = NAV.flatMap((g) => g.items).find((i) => i.id === active)?.label ?? '';

  return (
    <div className="tux-settings-overlay" data-testid="settings-backdrop" onClick={onClose}>
      <div
        className="tux-settings-window"
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

        <div className="tux-settings-cols">
          <nav className="tux-settings-nav" aria-label="Settings sections">
            {NAV.map((g) => (
              <div key={g.group} className="tux-settings-navgroup">
                <div className="tux-settings-navgroup-label">{g.group}</div>
                {g.items.map((i) => (
                  <button
                    key={i.id}
                    type="button"
                    className={`tux-settings-navitem${active === i.id ? ' is-active' : ''}`}
                    data-testid={`settings-nav-${i.id}`}
                    aria-current={active === i.id ? 'page' : undefined}
                    onClick={() => setActive(i.id)}
                  >
                    {i.label}
                  </button>
                ))}
              </div>
            ))}
          </nav>

          <section
            className={`tux-settings-pane tux-settings-pane--${active}`}
            data-testid={`settings-pane-${active}`}
            aria-label={sectionTitle}
          >
            {error && (
              <div className="tux-settings-error" role="alert">
                {error}
              </div>
            )}

            {active === 'identities' && <IdentitiesSettings />}

            {active === 'account' && <WinlinkAccountSettings />}

            {active === 'location' && <LocationSettingsPane />}

            {active === 'gpsstate' && (
              <div className="tux-settings-formblock">
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
            )}

            {active === 'aprs' && <AprsSettings />}

            {active === 'forms' && <FormSequenceSettings />}

            {active === 'maps' && <OfflineMapsSettings />}

            {active === 'mailbox' && <MailboxSettings />}

            {active === 'window' && <WindowSettings />}
          </section>
        </div>
      </div>
    </div>
  );
}
