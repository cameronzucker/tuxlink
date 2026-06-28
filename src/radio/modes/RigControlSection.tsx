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

import { useCallback, useEffect, useMemo, useState } from 'react';
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
  data_mode: string;
  rig_field_overrides: string[];
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
  data_mode: 'PKTUSB',
  rig_field_overrides: [],
};

/** Mirror of the backend RigModelDto from rig_list_models. */
interface RigModel {
  id: number;
  manufacturer: string;
  model: string;
}

interface RigControlSectionProps {
  /** Prefix for the localStorage collapse-state key, e.g. "ardop" or "vara". */
  storageKeyPrefix: string;
}

/** Collapsible "Rig control" expander — hamlib model, CAT serial/baud,
 *  data mode, close-serial sequencing, and live VFO poll. Loads from
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

  const [models, setModels] = useState<RigModel[]>([]);
  const [serialPorts, setSerialPorts] = useState<{ path: string; kind: string; label: string }[]>([]);

  const loadModels = useCallback(() => {
    void invoke<RigModel[]>('rig_list_models')
      .then((list) => setModels(list ?? []))
      .catch(() => setModels([]));
  }, []);
  const loadSerialPorts = useCallback(() => {
    void invoke<{ path: string; kind: string; label: string }[]>('packet_list_serial_devices')
      .then((list) => setSerialPorts(list ?? []))
      .catch(() => setSerialPorts([]));
  }, []);

  // Group + sort models by manufacturer for the picker (A–Z; no curated pins).
  const groupedModels = useMemo(() => {
    const byMfg = new Map<string, RigModel[]>();
    for (const m of models) {
      const arr = byMfg.get(m.manufacturer) ?? [];
      arr.push(m);
      byMfg.set(m.manufacturer, arr);
    }
    return [...byMfg.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([mfg, list]) => ({
        mfg,
        list: list.slice().sort((a, b) => a.model.localeCompare(b.model)),
      }));
  }, [models]);

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

  useEffect(() => {
    loadModels();
    loadSerialPorts();
  }, [loadModels, loadSerialPorts]);

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

      {/* Radio model — sourced from the installed hamlib via rig_list_models
          (rigctl -l), grouped by manufacturer, A–Z. No curated pins. Empty
          model list degrades to a manual hamlib-model-# entry. */}
      <label className="radio-panel-input-row">
        <span>Radio</span>
        {models.length > 0 ? (
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
            {groupedModels.map((g) => (
              <optgroup key={g.mfg} label={g.mfg}>
                {g.list.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.manufacturer} {m.model} ({m.id})
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        ) : (
          <input
            type="text"
            inputMode="numeric"
            className="radio-panel-input"
            data-testid="rig-model-manual"
            value={rigConfig?.rig_hamlib_model ?? ''}
            placeholder="hamlib model # (rigctl unavailable)"
            spellCheck={false}
            onChange={(e) => {
              const n = Number(e.target.value.trim());
              persistRig({ rig_hamlib_model: Number.isInteger(n) && n > 0 ? n : null });
            }}
          />
        )}
        <button
          type="button"
          className="radio-panel-btn-sm"
          data-testid="rig-model-refresh"
          onClick={loadModels}
          aria-label="Refresh radio model list"
        >
          ↻
        </button>
      </label>

      {/* CAT port — detected serial ports (reuses packet_list_serial_devices,
          the AX.25/PTT enumeration). Manual row covers an unlisted device. */}
      <label className="radio-panel-input-row">
        <span>CAT port</span>
        <select
          className="radio-panel-input"
          data-testid="rig-cat-port"
          value={serialPorts.some((d) => d.path === catSerialInput) ? catSerialInput : ''}
          onChange={(e) => {
            const next = e.target.value;
            setCatSerialInput(next);
            persistRig({ cat_serial_path: next === '' ? null : next });
          }}
        >
          <option value="">Choose serial port…</option>
          {serialPorts.map((d) => (
            <option key={d.path} value={d.path}>
              {d.path} — {d.label}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="radio-panel-btn-sm"
          data-testid="rig-cat-port-refresh"
          onClick={loadSerialPorts}
          aria-label="Refresh CAT serial port list"
        >
          ↻
        </button>
      </label>
      <label className="radio-panel-input-row">
        <span>Manual</span>
        <input
          type="text"
          className="radio-panel-input"
          data-testid="rig-cat-port-manual"
          value={catSerialInput}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder="/dev/ttyUSB0 (unlisted)"
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

      {/* Data mode — the token rigctld sets on tune (Mode::rigctl_str). */}
      <label className="radio-panel-input-row">
        <span>Mode</span>
        <select
          className="radio-panel-input"
          data-testid="rig-data-mode"
          value={rigConfig?.data_mode ?? 'PKTUSB'}
          onChange={(e) => persistRig({ data_mode: e.target.value })}
        >
          <option value="PKTUSB">PKTUSB</option>
          <option value="PKTLSB">PKTLSB</option>
          <option value="USB-D">USB-D</option>
          <option value="LSB-D">LSB-D</option>
          <option value="USB">USB</option>
          <option value="LSB">LSB</option>
        </select>
      </label>

      {/* Close-serial sequencing — required for radios (e.g. FT-710) that
          reset their codec when the CAT serial port is held open during audio.
          Enabling this forces live_vfo_poll off (poll holds the port open). */}
      <label className="radio-panel-input-row">
        <span>Close serial during audio</span>
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
    </details>
  );
}
