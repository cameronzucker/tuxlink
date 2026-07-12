// src/radio/modes/RigControlSection.tsx
//
// Shared "Rig control" expander for ARDOP, VARA, and (Task C9b) the FT-8
// setup surface. Reads/writes the radio-level Config.rig via the Tauri
// commands config_get_rig / config_set_rig introduced in Task A1.
//
// All three sites render this component — one physical radio, every mode
// shares the same hamlib/CAT config. The storageKeyPrefix param distinguishes
// the localStorage collapse-state key so each render site can be
// independently expanded/collapsed.
//
// Task C9b: the FT-8 setup surface's "Test CAT" action must not read stale
// `Config.rig` when the operator has just typed into a field but not yet
// blurred it (blur is when this component normally flushes serial/baud/
// rigctld-binary to the backend). `commitNow()` — exposed via
// `useImperativeHandle` on a forwarded ref — flushes those three
// blur-deferred fields immediately, so a caller can `await ref.current
// ?.commitNow()` right before probing. The ref is OPTIONAL: ArdopRadioPanel
// and VaraRadioPanel render this component without a ref today and keep
// working unchanged — forwardRef accepts (and ignores) a missing ref.

import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button, Select, Field } from '../../controls';
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

/** Imperative handle (Task C9b) — exposed via a forwarded ref. Optional for
 *  every consumer; ArdopRadioPanel/VaraRadioPanel render without a ref and
 *  are unaffected. */
export interface RigControlSectionHandle {
  /** Flushes the three blur-deferred fields (CAT serial manual entry, CAT
   *  baud, rigctld binary) to the backend immediately, regardless of focus
   *  state. Resolves once every flush's config_set_rig round-trip (or its
   *  no-op skip, e.g. an invalid baud that reverts instead of persisting)
   *  has settled. Safe to call even when nothing changed since the last
   *  blur — the writes are idempotent read-modify-writes. */
  commitNow: () => Promise<void>;
}

/** Collapsible "Rig control" expander — hamlib model, CAT serial/baud,
 *  data mode, close-serial sequencing, and live VFO poll. Loads from
 *  config_get_rig on mount; persists via config_set_rig on change/blur.
 *  Collapsed by default; collapse state is preserved in localStorage.
 *  Use variant="bare" to render only the field rows (no expander chrome). */
export const RigControlSection = forwardRef<RigControlSectionHandle, RigControlSectionProps>(
  function RigControlSection({ storageKeyPrefix, variant = 'expander', onRadioSelected }, ref) {
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
  const [rigctldBinaryInput, setRigctldBinaryInput] = useState<string>('rigctld');

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
        setRigctldBinaryInput(c.rigctld_binary ?? 'rigctld');
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
   *  receives the fresh backend config and returns the patch to merge.
   *
   *  Returns a Promise<void> that settles once the write (or its best-effort
   *  fallback) has been attempted — Task C9b's `commitNow()` awaits this so a
   *  caller can be sure a just-typed field has actually reached the backend
   *  before it reads Config.rig for something else (e.g. a CAT probe). */
  const rmwRig = (compute: (fresh: RigConfig) => Partial<RigConfig>): Promise<void> => {
    return invoke<RigConfig>('config_get_rig')
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
        return invoke('config_set_rig', { value: next }).catch(() => {});
      })
      .then(() => undefined);
  };

  /** Merge a patch into the backend config and update local state.
   *  Uses a read-modify-write against the backend so the shared
   *  rig_field_overrides (also written by the ARDOP PTT handler) is never
   *  clobbered by a stale local copy. */
  const persistRig = (patch: Partial<RigConfig>): Promise<void> => {
    return rmwRig(() => patch);
  };

  /** Persist a patch AND add `key` to the override set (idempotent). Used when
   *  the operator hand-edits a profile-managed field so a later radio change
   *  won't clobber it. Reads the fresh override set from the backend so a
   *  concurrent ARDOP-panel PTT override is never lost. */
  const persistRigWithOverride = (key: string, patch: Partial<RigConfig>): Promise<void> => {
    return rmwRig((fresh) => ({
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

  /** commitNow (Task C9b) awaits this directly, so it MUST return the
   *  persist Promise rather than fire-and-forget it — otherwise a caller
   *  awaiting commitNow() would resolve before the write actually landed. */
  const commitCatSerial = (): Promise<void> => {
    const trimmed = catSerialInput.trim();
    return persistRig({ cat_serial_path: trimmed === '' ? null : trimmed });
  };

  const commitCatBaud = (): Promise<void> => {
    const n = Number(catBaudInput.trim());
    if (!Number.isInteger(n) || n <= 0) {
      setCatBaudInput(String(rigConfig?.cat_baud ?? 38400));
      // Invalid input reverts instead of persisting — nothing to await.
      return Promise.resolve();
    }
    return persistRigWithOverride('cat_baud', { cat_baud: n });
  };

  /** Blank clears back to the bundled sentinel "rigctld" rather than an empty
   *  string, since rigctld_binary is a plain (non-nullable) string field and
   *  "rigctld" is what config_get_rig / the Rust backend treats as "use the
   *  bundled copy" (bd-tuxlink-a9ip3). Not radio-profile-driven (unlike
   *  data_mode/cat_baud/close_serial), so a plain persistRig — no override-set
   *  bookkeeping needed. */
  const commitRigctldBinary = (): Promise<void> => {
    const trimmed = rigctldBinaryInput.trim();
    const next = trimmed === '' ? 'rigctld' : trimmed;
    setRigctldBinaryInput(next);
    return persistRig({ rigctld_binary: next });
  };

  /** Task C9b: flush the three blur-deferred fields (CAT serial manual
   *  entry, CAT baud, rigctld binary) immediately, regardless of DOM focus.
   *  Exposed via the forwarded ref so a caller — the FT-8 setup surface's
   *  "Test CAT" button — can `await ref.current?.commitNow()` right before
   *  probing, so a just-typed-but-unblurred field doesn't cause a false
   *  "radio not responding". No dependency array: each render closes over
   *  the latest input state, and re-registering the handle every render is
   *  cheap (a single object with one function). */
  useImperativeHandle(ref, () => ({
    commitNow: async () => {
      await Promise.all([commitCatSerial(), commitCatBaud(), commitRigctldBinary()]);
    },
  }));

  const rows = (
    <>
      {/* Radio model — sourced from the installed hamlib via rig_list_models
          (rigctl -l), grouped by manufacturer, A–Z. No curated pins. Empty
          model list degrades to a manual hamlib-model-# entry. */}
      <label className="radio-panel-input-row">
        <span>Radio</span>
        {models.length > 0 ? (
          <Select
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
          </Select>
        ) : (
          <Field
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
        <Button
          tone="neutral" emphasis="outline" size="xs"
          data-testid="rig-model-refresh"
          onClick={loadModels}
          aria-label="Refresh radio model list"
        >
          ↻
        </Button>
      </label>

      {/* CAT port — detected serial ports (reuses packet_list_serial_devices,
          the AX.25/PTT enumeration). Manual row covers an unlisted device. */}
      <label className="radio-panel-input-row">
        <span>CAT port</span>
        <Select
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
        </Select>
        <Button
          tone="neutral" emphasis="outline" size="xs"
          data-testid="rig-cat-port-refresh"
          onClick={loadSerialPorts}
          aria-label="Refresh CAT serial port list"
        >
          ↻
        </Button>
      </label>
      <label className="radio-panel-input-row">
        <span>Manual</span>
        <Field
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
        <Field
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

      {/* rigctld binary — default "rigctld" is the bundled-sidecar sentinel
          (bd-tuxlink-a9ip3): Tuxlink resolves it to the copy it ships, no
          system hamlib install required. Overriding it to reach a different
          hamlib on the operator's system requires an absolute path — a bare
          name like "rigctld" would just re-resolve to the bundled copy. */}
      <label className="radio-panel-input-row">
        <span>rigctld binary</span>
        <Field
          type="text"
          className="radio-panel-input"
          data-testid="rig-rigctld-binary"
          value={rigctldBinaryInput}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          placeholder="bundled (default) — set an absolute path to use your own hamlib"
          onChange={(e) => setRigctldBinaryInput(e.target.value)}
          onBlur={commitRigctldBinary}
        />
      </label>
      <p className="radio-panel-radio-help" data-testid="rig-rigctld-binary-hint">
        Leave as <strong>rigctld</strong> to use Tuxlink&apos;s bundled copy. To use a
        different hamlib, enter its absolute path (e.g. <strong>/usr/bin/rigctld</strong>).
      </p>

      {/* Data mode — the token rigctld sets on tune (Mode::rigctl_str). */}
      <label className="radio-panel-input-row">
        <span>Mode</span>
        <Select
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
        </Select>
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

      {/* QSY on fail — control removed for tuxlink-qevsf/tuxlink-31c63
          (SAFETY/Part 97): auto-QSY transmitted on candidate frequencies the
          operator never saw or selected (a control-operator violation). The
          connect path clamps the candidate list to the operator-chosen channel,
          so this control would be inert. The `qsy_on_fail` field stays in
          RigConfig/DEFAULT_RIG_CONFIG to avoid churning the config schema; it is
          simply not rendered. Restored by the Channel-Selection redesign (Find a
          Station = operator-driven channel picker). */}
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
  },
);
