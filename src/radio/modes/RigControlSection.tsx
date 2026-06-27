// src/radio/modes/RigControlSection.tsx
//
// Shared "Rig control" expander for ARDOP and VARA panels. Reads/writes the
// radio-level Config.rig via the Tauri commands config_get_rig /
// config_set_rig introduced in Task A1.
//
// Both panels render this component — one physical radio, both modes share
// the same hamlib/CAT config. The storageKeyPrefix param distinguishes the
// localStorage collapse-state key so the two panels can be independently
// expanded/collapsed.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** TypeScript mirror of Rust's `RigUiConfig`. Field names are snake_case to
 *  match the serde-serialised wire format from the Tauri backend. */
export interface RigConfig {
  rig_hamlib_model: number | null;
  rigctld_host: string;
  rigctld_port: number;
  rigctld_binary: string;
  close_serial_sequencing: boolean;
  live_vfo_poll: boolean;
  qsy_on_fail: boolean;
  cat_serial_path: string | null;
  cat_baud: number;
}

const DEFAULT_RIG_CONFIG: RigConfig = {
  rig_hamlib_model: null,
  rigctld_host: '127.0.0.1',
  rigctld_port: 4534,
  rigctld_binary: 'rigctld',
  close_serial_sequencing: false,
  live_vfo_poll: false,
  qsy_on_fail: false,
  cat_serial_path: null,
  cat_baud: 38400,
};

interface RigControlSectionProps {
  /** Prefix for the localStorage collapse-state key, e.g. "ardop" or "vara". */
  storageKeyPrefix: string;
}

/** Collapsible "Rig control" expander — hamlib model, CAT serial/baud,
 *  close-serial sequencing, live VFO poll, and QSY-on-fail. Loads from
 *  config_get_rig on mount; persists via config_set_rig on change/blur.
 *  Collapsed by default; collapse state is preserved in localStorage. */
export function RigControlSection({ storageKeyPrefix }: RigControlSectionProps) {
  const lsKey = `tuxlink.${storageKeyPrefix}.rigCfgOpen`;

  const [rigCfgOpen, setRigCfgOpen] = useState<boolean>(() => {
    try {
      return localStorage.getItem(lsKey) === '1';
    } catch {
      return false;
    }
  });

  const [rigConfig, setRigConfig] = useState<RigConfig | null>(null);
  const [catSerialInput, setCatSerialInput] = useState<string>('');
  const [catBaudInput, setCatBaudInput] = useState<string>('38400');

  // Load rig config from backend on mount.
  useEffect(() => {
    let cancelled = false;
    invoke<RigConfig>('config_get_rig')
      .then((c) => {
        if (cancelled || !c) return;
        setRigConfig(c);
        setCatSerialInput(c.cat_serial_path ?? '');
        setCatBaudInput(String(c.cat_baud ?? 38400));
      })
      .catch(() => {
        /* config absent on first-run / pre-wizard — keep defaults */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** Merge a patch into the local state and persist the result to the backend.
   *  Mirrors the persistArdop pattern used by the ARDOP panel. */
  const persistRig = (patch: Partial<RigConfig>) => {
    const base = rigConfig ?? DEFAULT_RIG_CONFIG;
    const next = { ...base, ...patch };
    setRigConfig(next);
    void invoke('config_set_rig', { value: next }).catch(() => {
      /* persist errors surface via the session log from the backend */
    });
  };

  const commitCatSerial = () => {
    const trimmed = catSerialInput.trim();
    persistRig({ cat_serial_path: trimmed === '' ? null : trimmed });
  };

  const commitCatBaud = () => {
    const n = Number(catBaudInput.trim());
    if (!Number.isInteger(n) || n <= 0) {
      setCatBaudInput(String(rigConfig?.cat_baud ?? 38400));
      return;
    }
    persistRig({ cat_baud: n });
  };

  return (
    <details
      className="expander"
      open={rigCfgOpen}
      onToggle={(e) => {
        const open = e.currentTarget.open;
        setRigCfgOpen(open);
        try {
          localStorage.setItem(lsKey, open ? '1' : '0');
        } catch {
          /* localStorage unavailable — in-memory toggle still works */
        }
      }}
      data-testid="rig-control-expander"
    >
      <summary className="expander-summary" data-testid="rig-control-expander-summary">
        Rig control
      </summary>

      {/* Hamlib rig model — null means no rigctld integration. The FT-710
          is the proven model; additional entries can be appended here as
          more rigs are validated. */}
      <label className="radio-panel-input-row">
        <span>Rig model</span>
        <select
          className="radio-panel-input"
          data-testid="rig-model"
          value={rigConfig?.rig_hamlib_model ?? ''}
          onChange={(e) => {
            const v = e.target.value;
            persistRig({ rig_hamlib_model: v === '' ? null : Number(v) });
          }}
        >
          <option value="">None / unset</option>
          <option value="1049">Yaesu FT-710 (1049)</option>
        </select>
      </label>

      {/* CAT serial port — the /dev/ttyUSB* path the CAT cable connects to.
          Used by both ARDOP (close-serial bridge) and VARA (managed rigctld). */}
      <label className="radio-panel-input-row">
        <span>CAT port</span>
        <input
          type="text"
          className="radio-panel-input"
          data-testid="rig-cat-port"
          value={catSerialInput}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder="/dev/ttyUSB0 (CAT/Enhanced port)"
          onChange={(e) => setCatSerialInput(e.target.value)}
          onBlur={commitCatSerial}
        />
      </label>

      {/* CAT baud rate — 38400 matches the FT-710's Enhanced port default. */}
      <label className="radio-panel-input-row">
        <span>CAT baud</span>
        <input
          type="text"
          inputMode="numeric"
          className="radio-panel-input"
          data-testid="rig-cat-baud"
          value={catBaudInput}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder="38400"
          onChange={(e) => setCatBaudInput(e.target.value)}
          onBlur={commitCatBaud}
        />
      </label>

      {/* rigctld backend label — informational, not user-editable. */}
      <div className="radio-panel-input-row">
        <span>CAT backend</span>
        <span className="radio-panel-input" style={{ opacity: 0.7, userSelect: 'none' }}>
          Managed rigctld
        </span>
      </div>

      {/* Close-serial sequencing — required for radios (e.g. FT-710) that
          reset their codec when the CAT serial port is held open during audio.
          Enabling this forces live_vfo_poll off (poll holds the port open). */}
      <label className="radio-panel-input-row">
        <span>Close-serial sequencing</span>
        <input
          type="checkbox"
          data-testid="rig-close-serial"
          checked={rigConfig?.close_serial_sequencing ?? false}
          onChange={(e) => {
            const checked = e.target.checked;
            persistRig({
              close_serial_sequencing: checked,
              ...(checked ? { live_vfo_poll: false } : {}),
            });
          }}
        />
      </label>

      {/* Live VFO poll — disabled when close-serial sequencing is on (polling
          holds the serial port open, incompatible with sequenced close). */}
      <label className="radio-panel-input-row">
        <span>Live VFO poll</span>
        <input
          type="checkbox"
          data-testid="rig-live-vfo"
          checked={rigConfig?.live_vfo_poll ?? false}
          disabled={rigConfig?.close_serial_sequencing ?? false}
          onChange={(e) => {
            persistRig({ live_vfo_poll: e.target.checked });
          }}
        />
      </label>

      {/* QSY on fail — when enabled, tuxlink walks the candidate frequency
          list and retunes the rig on each failed connect attempt. */}
      <label className="radio-panel-input-row">
        <span>QSY on fail</span>
        <input
          type="checkbox"
          data-testid="rig-qsy-on-fail"
          checked={rigConfig?.qsy_on_fail ?? false}
          onChange={(e) => {
            persistRig({ qsy_on_fail: e.target.checked });
          }}
        />
      </label>
    </details>
  );
}
