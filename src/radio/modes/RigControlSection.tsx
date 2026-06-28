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
import { getRigProfile } from './rigProfiles';

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
  /** Render mode. 'expander' (default) wraps the rows in a <details> expander.
   *  'bare' renders only the field rows as a fragment — used by ARDOP so it can
   *  embed the rows inside its own merged "Radio & audio" group (Task 7). */
  variant?: 'expander' | 'bare';
  /** Called after the hamlib model changes; lets the parent panel pre-fill
   *  PTT-method (Task 7). pttOverridden=true when the operator has already
   *  hand-edited ptt_method so the parent should leave it alone. */
  onRadioSelected?: (modelId: number | null, pttOverridden: boolean) => void;
}

/** Collapsible "Rig control" expander — hamlib model, CAT serial/baud,
 *  data mode, close-serial sequencing, and live VFO poll. Loads from
 *  config_get_rig on mount; persists via config_set_rig on change/blur.
 *  Collapsed by default; collapse state is preserved in localStorage.
 *  Use variant="bare" to render only the field rows (no expander chrome). */
export function RigControlSection({ storageKeyPrefix, variant = 'expander', onRadioSelected }: RigControlSectionProps) {
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

  /** Read-modify-write Config.rig against the BACKEND (not the possibly-stale
   *  local rigConfig) so the shared rig_field_overrides — also written by the
   *  ARDOP panel's PTT handler — is never clobbered by a stale copy. `compute`
   *  receives the fresh backend config and returns the patch to merge. */
  const rmwRig = (compute: (fresh: RigConfig) => Partial<RigConfig>) => {
    void invoke<RigConfig>('config_get_rig')
      .then((fresh) => {
        const base = fresh ?? rigConfig ?? DEFAULT_RIG_CONFIG;
        const next = { ...base, ...compute(base) };
        setRigConfig(next);
        return invoke('config_set_rig', { value: next });
      })
      .catch(() => {
        // backend unreadable (pre-wizard): best-effort local merge
        const base = rigConfig ?? DEFAULT_RIG_CONFIG;
        const next = { ...base, ...compute(base) };
        setRigConfig(next);
        void invoke('config_set_rig', { value: next }).catch(() => {});
      });
  };

  /** Merge a patch into the backend config and update local state.
   *  Uses a read-modify-write against the backend so the shared
   *  rig_field_overrides (also written by the ARDOP PTT handler) is never
   *  clobbered by a stale local copy. */
  const persistRig = (patch: Partial<RigConfig>) => {
    rmwRig(() => patch);
  };

  /** Persist a patch AND add `key` to the override set (idempotent). Used when
   *  the operator hand-edits a profile-managed field so a later radio change
   *  won't clobber it. Reads the fresh override set from the backend so a
   *  concurrent ARDOP-panel PTT override is never lost. */
  const persistRigWithOverride = (key: string, patch: Partial<RigConfig>) => {
    rmwRig((fresh) => ({
      ...patch,
      rig_field_overrides: fresh.rig_field_overrides.includes(key)
        ? fresh.rig_field_overrides
        : [...fresh.rig_field_overrides, key],
    }));
  };

  /** On radio selection: set the model, then apply the radio's documented
   *  profile to each shared field the operator has NOT overridden.
   *  Reads the fresh override set from the backend so a concurrent ARDOP-panel
   *  PTT override is never lost (fixes the ptt_method-drop regression). */
  const onModelSelected = (modelId: number | null) => {
    rmwRig((fresh) => {
      const overrides = new Set(fresh.rig_field_overrides);
      const patch: Partial<RigConfig> = { rig_hamlib_model: modelId };
      const profile = getRigProfile(modelId);
      if (profile) {
        if (profile.data_mode !== undefined && !overrides.has('data_mode')) {
          patch.data_mode = profile.data_mode;
        }
        if (profile.cat_baud !== undefined && !overrides.has('cat_baud')) {
          patch.cat_baud = profile.cat_baud;
          // keep the controlled baud input in sync if the profile changed it
          setCatBaudInput(String(profile.cat_baud));
        }
        if (profile.close_serial_sequencing !== undefined && !overrides.has('close_serial')) {
          patch.close_serial_sequencing = profile.close_serial_sequencing;
        }
      }
      // notify ARDOP to pre-fill ptt_method (Task 7); no-op when prop absent
      if (onRadioSelected) onRadioSelected(modelId, overrides.has('ptt_method'));
      return patch;
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
    persistRigWithOverride('cat_baud', { cat_baud: n });
  };

  const rows = (
    <>
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
              onModelSelected(v === '' ? null : Number(v));
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
              onModelSelected(Number.isInteger(n) && n > 0 ? n : null);
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
          onChange={(e) => persistRigWithOverride('data_mode', { data_mode: e.target.value })}
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
            persistRigWithOverride('close_serial', {
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
    </>
  );

  if (variant === 'bare') {
    return rows;
  }
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
      {rows}
    </details>
  );
}
