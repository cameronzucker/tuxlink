// ServiceCodesField — operator control for the station-listing service codes
// that filter which gateways the catalog returns (tuxlink-6j14).
//
// A service code is a sysop-assigned TAG on a gateway registration; the listing
// endpoint returns only gateways tagged with the code(s) you request. It is a
// DIRECTORY FILTER, not a connection credential — it never travels over the air.
// PUBLIC and EMCOMM are the only publicly-blessed codes; group codes (MARS /
// SHARES) are member-issued FOUO secrets the operator pastes themselves. The
// value is persisted in the OS keyring (never plaintext config, never hardcoded)
// by the `catalog_set_service_codes` command; `catalog_get_service_codes` reads
// it back (default PUBLIC).

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** The codes Winlink makes public; safe to offer as one-tap presets. Group
 *  codes are intentionally absent — they are member-issued secrets, never shipped. */
const PUBLIC_PRESETS = ['PUBLIC', 'EMCOMM'] as const;

export interface ServiceCodesFieldProps {
  /** Fired after a successful save so the finder can refetch with the new filter
   *  (the Rust cache keys on service codes, so the next fetch is a fresh set). */
  onApplied?: () => void;
}

export function ServiceCodesField({ onApplied }: ServiceCodesFieldProps) {
  const [saved, setSaved] = useState('PUBLIC');
  const [draft, setDraft] = useState('PUBLIC');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const codes = await invoke<string>('catalog_get_service_codes');
        if (active && typeof codes === 'string' && codes.length > 0) {
          setSaved(codes);
          setDraft(codes);
        }
      } catch {
        // Keep the PUBLIC default on read failure (e.g. no keyring daemon).
      }
    })();
    return () => {
      active = false;
    };
  }, []);

  const unchanged = draft.trim() === saved.trim();

  async function apply() {
    setBusy(true);
    setError(null);
    try {
      await invoke('catalog_set_service_codes', { codes: draft });
      // Read back the normalized value the backend actually stored.
      const codes = await invoke<string>('catalog_get_service_codes');
      setSaved(codes);
      setDraft(codes);
      onApplied?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  function addPreset(code: string) {
    const parts = draft.split(/\s+/).filter(Boolean);
    if (!parts.includes(code)) {
      setDraft([...parts, code].join(' '));
    }
  }

  return (
    <div className="station-finder__svc" data-testid="service-codes">
      <label className="station-finder__svc-lab" htmlFor="service-codes-input">
        Listing service codes
      </label>
      <input
        id="service-codes-input"
        className="station-finder__svc-input"
        data-testid="service-codes-input"
        value={draft}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        aria-label="Station-listing service codes"
        title="Most stations use PUBLIC. MARS/SHARES members: enter the code your group provides."
        onChange={(e) => setDraft(e.target.value)}
      />
      {PUBLIC_PRESETS.map((code) => (
        <button
          key={code}
          type="button"
          className="station-finder__svc-preset"
          data-testid={`service-codes-preset-${code}`}
          onClick={() => addPreset(code)}
        >
          {code}
        </button>
      ))}
      <button
        type="button"
        className="station-finder__svc-apply"
        data-testid="service-codes-apply"
        disabled={busy || unchanged}
        onClick={apply}
      >
        {busy ? 'Saving…' : 'Apply'}
      </button>
      {error && (
        <span className="station-finder__svc-err" role="alert" data-testid="service-codes-error">
          {error}
        </span>
      )}
    </div>
  );
}
