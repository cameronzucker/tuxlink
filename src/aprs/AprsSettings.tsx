// src/aprs/AprsSettings.tsx
//
// APRS station identity settings (Task 14). A small inline form, rendered as a
// section inside the Settings panel, that reads the live APRS config
// (aprs_config_get) and persists edits via aprs_config_set.
//
// Fields:
//   - Source SSID — the station's APRS SSID (0-15), a number input.
//   - Path        — the digipeater path (e.g. "WIDE1-1,WIDE2-1"), a text input.
//   - To-call     — the destination/software tocall (e.g. "APZTUX"). Shown
//                   read-only: it identifies the Tuxlink software and is not an
//                   operator-tunable field.
//
// The JS arg key for the setter MUST be `dto` (the Tauri command signature is
// `aprs_config_set(dto: AprsConfigDto)`), so the call site passes
// `{ dto: { sourceSsid, tocall, path } }`.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AprsConfigDto } from './aprsTypes';

export function AprsSettings() {
  const [sourceSsid, setSourceSsid] = useState<number>(0);
  const [tocall, setTocall] = useState<string>('');
  const [path, setPath] = useState<string>('');
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load the live config once on mount.
  useEffect(() => {
    let mounted = true;
    invoke<AprsConfigDto>('aprs_config_get')
      .then((cfg) => {
        if (!mounted) return;
        setSourceSsid(cfg.sourceSsid);
        setTocall(cfg.tocall);
        setPath(cfg.path);
        setLoaded(true);
      })
      .catch(() => {
        if (mounted) setError('Could not load APRS settings.');
      });
    return () => {
      mounted = false;
    };
  }, []);

  const onSave = async () => {
    setError(null);
    try {
      // The setter's wire arg key MUST be `dto`. tocall passes through unchanged
      // (read-only in the UI) so the persisted record keeps the software tocall.
      await invoke('aprs_config_set', {
        dto: { sourceSsid, tocall, path } satisfies AprsConfigDto,
      });
    } catch {
      setError('Could not save APRS settings.');
    }
  };

  return (
    <div className="aprs-settings" data-testid="aprs-settings">
      {error && (
        <div className="tux-settings-error" role="alert">
          {error}
        </div>
      )}

      <label className="tux-settings-field">
        <span className="tux-settings-field-label">Source SSID</span>
        <input
          type="number"
          min={0}
          max={15}
          value={loaded ? sourceSsid : ''}
          onChange={(e) => setSourceSsid(Number(e.target.value))}
        />
      </label>

      <label className="tux-settings-field">
        <span className="tux-settings-field-label">Path</span>
        <input
          type="text"
          value={path}
          spellCheck={false}
          autoCapitalize="characters"
          autoCorrect="off"
          onChange={(e) => setPath(e.target.value)}
        />
      </label>

      <div className="tux-settings-field">
        <span className="tux-settings-field-label">To-call</span>
        <span className="tux-settings-readonly" data-testid="aprs-tocall">
          {tocall}
        </span>
      </div>

      <button type="button" className="tux-settings-save" onClick={onSave}>
        Save
      </button>
    </div>
  );
}
