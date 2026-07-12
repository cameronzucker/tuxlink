// BandSubsetPopover.tsx — Task C10, plan tuxlink-b026z.4 §Frontend Phase C,
// spec §Strip "Band-subset popover" (Flow 1: "limit FT-8 decode to a subset
// of bands or only one band").
//
// Read contract (spec, load-bearing): this popover renders from
// `snapshot.sweepConfig` — CONFIG TRUTH — never from the live `sweep` status.
// A saved one-band subset must reopen as saved while stopped, and the Sweep
// radio must stay CHECKED (config) even during `fallback-hold` (a runtime
// condition surfaced separately, via the inline warning below). B1's hook
// re-hydrates `sweepConfig` on a coalesced `ft8-listening:change` after a set
// command persists — the popover does not keep any of its own copy of the
// band list or the mode; every render derives straight from the `sweepConfig`
// prop, so a config change from ANY source (this popover, a future settings
// surface, a stale multi-window app) reflects here on the next render.
//
// Props-driven (not a direct `useFt8Listener()` caller): mirrors the app's
// established "hook once at the shell, props down" shape (AppShell calls
// `useFt8Listener()`; StationRail / LiveDecodesTab / DashboardRibbon all take
// slices as props) so this component unit-tests without mounting the full
// `Ft8ListenerProvider`. The eventual strip host (LiveBandStrip, Task C7, not
// yet built) threads `useFt8Listener().snapshot` fields straight through.
//
// Band-name validation (brief: "reject 60m/VHF client-side too"): the chip
// set is generated FROM `FT8_BANDS` below (transcribed from the backend's
// `tuxlink_capture::bands::BANDS` table) — there is no free-text band entry
// anywhere in this UI, so an invalid band can never be constructed to send.
// The backend remains the authority (`ft8_set_sweep_bands_inner` re-validates
// every entry before persisting); this is a client-side belt, not the buckle.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '../controls/Button';
import type { Ft8CmdError, SweepConfigDto } from './ft8Types';
import './BandSubsetPopover.css';

/**
 * Band label → dial Hz, transcribed from
 * `src-tauri/tuxlink-capture/src/bands.rs` `BANDS` (low→high; display order
 * is part of that table's contract). Labels only — this component never
 * needs the dial Hz, just the valid-band set and its canonical order.
 */
const FT8_BANDS: readonly string[] = [
  '160m', '80m', '40m', '30m', '20m', '17m', '15m', '12m', '10m',
];

/** The default reason shown on the Sweep row before any probe result exists
 *  (brief: "without CAT confirmation, the sweep-enable control is DISABLED
 *  with a reason"). */
const DEFAULT_SWEEP_REASON = 'sweep needs CAT to QSY between bands';

type CatProbeState =
  | { status: 'skipped' } // already sweep-enabled (config truth) — no gate needed
  | { status: 'checking' }
  | { status: 'ok' }
  | { status: 'failed'; error: Ft8CmdError };

function asFt8CmdError(err: unknown): Ft8CmdError {
  if (err && typeof err === 'object' && 'kind' in err) {
    const candidate = err as { kind: unknown; detail?: unknown };
    if (typeof candidate.kind === 'string') {
      return {
        kind: candidate.kind,
        detail: typeof candidate.detail === 'string' ? candidate.detail : '',
      };
    }
  }
  return { kind: 'internal-error', detail: typeof err === 'string' ? err : 'CAT probe failed' };
}

/** Per-kind disable reason (brief: "A modem-busy/rig-not-configured probe
 *  result disables with the matching reason"; unknown/other kinds — incl.
 *  `probe-timeout` — fall back to the spec's literal generic copy).
 *
 *  The modem-busy copy INTERPOLATES the active blocking session's mode
 *  (spec §502 pins "radio busy with <mode> session — disconnect first").
 *  The mode cannot come from the `Ft8CmdError` — the A4 error carries no mode
 *  identifier and the Ft8CmdError contract has the UI branch on `kind`, never
 *  parse `detail` — so it is passed in from the app's active-modem state via
 *  `blockingSessionMode`, degrading to "another" when unknown/unavailable. */
function catProbeReason(error: Ft8CmdError | null, blockingSessionMode?: string): string {
  if (!error) return DEFAULT_SWEEP_REASON;
  switch (error.kind) {
    case 'modem-busy':
      return `radio busy with ${blockingSessionMode ?? 'another'} session — disconnect first`;
    case 'rig-not-configured':
      return 'no rig configured — set up CAT first';
    default:
      return 'radio not responding — check CAT';
  }
}

export interface BandSubsetPopoverProps {
  /** `useFt8Listener().snapshot.sweepConfig` — CONFIG TRUTH. Renders verbatim;
   *  this component keeps no local copy of bands/enabled. */
  sweepConfig: SweepConfigDto;
  /** `useFt8Listener().snapshot.band` — the strip's held single band, shown
   *  on the "Hold one band — <band>" row label. */
  heldBand: string;
  /** `useFt8Listener().snapshot.service.axis === 'listening'` — gates the
   *  persist-only caption (spec: edits persist even while stopped, but only
   *  take effect at next start). */
  isListening: boolean;
  /** `useFt8Listener().snapshot.sweep.mode === 'fallback-hold'` — the
   *  runtime "radio not responding" condition, surfaced inline. Independent
   *  of `sweepConfig.enabled`, which stays true (config truth) throughout. */
  fallbackHold: boolean;
  /** The active blocking modem session's mode (e.g. "VARA", "ARDOP"), from
   *  the app's active-modem state — the parent (C7 LiveBandStrip / D1) wires
   *  this from `useActiveModemMode` / the connected-mode the ribbon shows.
   *  Interpolated into the `modem-busy` sweep-gate reason (spec §502's
   *  "<mode>"); when absent/unknown the copy degrades to "another session".
   *  Optional because the A4 `ft8_cat_probe` error cannot carry the mode
   *  (the Ft8CmdError contract branches on `kind`, never parses `detail`). */
  blockingSessionMode?: string;
}

export function BandSubsetPopover({
  sweepConfig,
  heldBand,
  isListening,
  fallbackHold,
  blockingSessionMode,
}: BandSubsetPopoverProps) {
  const { enabled: sweepEnabled, bands, dwellSlots } = sweepConfig;
  const [catProbe, setCatProbe] = useState<CatProbeState>(
    sweepEnabled ? { status: 'skipped' } : { status: 'checking' },
  );
  const [bandsError, setBandsError] = useState<string | null>(null);

  // A "fresh" ft8_cat_probe (spec: "gated on a fresh ft8_cat_probe result
  // when toggled") gates the TRANSITION into sweep mode. Already-enabled
  // sweep (config truth) is never re-gated by this effect — re-probing a
  // live sweep session would contend the same rig lock the sweep itself
  // uses, and `fallback-hold` already surfaces a live radio problem via its
  // own inline warning below, not by yanking this control's enabled state.
  useEffect(() => {
    if (sweepEnabled) {
      setCatProbe({ status: 'skipped' });
      return;
    }
    let cancelled = false;
    setCatProbe({ status: 'checking' });
    invoke('ft8_cat_probe')
      .then(() => {
        if (!cancelled) setCatProbe({ status: 'ok' });
      })
      .catch((err: unknown) => {
        if (!cancelled) setCatProbe({ status: 'failed', error: asFt8CmdError(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [sweepEnabled]);

  const sweepAllowed = sweepEnabled || catProbe.status === 'ok';
  const sweepFailure = catProbe.status === 'failed' ? catProbe.error : null;
  const sweepCaption = sweepAllowed
    ? `${dwellSlots} slot${dwellSlots === 1 ? '' : 's'}/band`
    : catProbeReason(sweepFailure, blockingSessionMode);

  const toggleBand = (band: string) => {
    if (!sweepEnabled) {
      // Hold-one mode: the chips are a SINGLE-SELECT held-band picker
      // (`ft8_set_band` — "the strip's existing band selection" the spec's
      // §Popover parenthetical names). This is also the no-CAT operator's band
      // assertion: the approved states mock's popover card reads "no CAT,
      // operator hasn't clicked a band" — clicking one IS the assertion that
      // flips provenance off `unconfirmed`. The original C10 brief made these
      // chips inert, which contradicted the mock and left `ft8_set_band` with
      // zero UI callers — a held band the operator could never change.
      if (band === heldBand) return; // already holding it — nothing to save
      setBandsError(null);
      invoke('ft8_set_band', { band }).catch((err: unknown) => {
        const e = asFt8CmdError(err);
        setBandsError(e.detail || 'could not change the held band');
      });
      return;
    }
    const has = bands.includes(band);
    const next = has ? bands.filter((b) => b !== band) : [...bands, band];
    if (next.length === 0) return; // never submit an empty subset (backend rejects it — client mirrors)
    setBandsError(null);
    invoke('ft8_set_sweep_bands', { bands: next }).catch((err: unknown) => {
      const e = asFt8CmdError(err);
      setBandsError(e.detail || 'could not save band selection');
    });
  };

  const selectHoldOne = () => {
    if (!sweepEnabled) return; // already hold-one
    invoke('ft8_set_sweep', { enabled: false }).catch(() => {});
  };

  const selectSweep = () => {
    if (sweepEnabled || !sweepAllowed) return; // already sweep, or gate not cleared
    invoke('ft8_set_sweep', { enabled: true }).catch(() => {});
  };

  return (
    <div className="band-subset-popover" data-testid="band-subset-popover">
      <div className="band-subset-popover__heading">Listen on</div>
      <div
        className="band-subset-popover__bands"
        role="group"
        aria-label={sweepEnabled ? 'Sweep bands' : 'Held band'}
      >
        {FT8_BANDS.map((band) => {
          // Selection tracks the MODE: the sweep subset in sweep mode, the one
          // held band in hold-one (where a click retunes via ft8_set_band).
          const selected = sweepEnabled ? bands.includes(band) : band === heldBand;
          return (
            <Button
              key={band}
              tone={selected ? 'primary' : 'neutral'}
              emphasis={selected ? 'soft' : 'outline'}
              size="sm"
              className="band-subset-popover__chip"
              data-testid={`band-subset-chip-${band}`}
              aria-pressed={selected}
              onClick={() => toggleBand(band)}
            >
              {selected && <span aria-hidden="true">✓ </span>}
              {band}
            </Button>
          );
        })}
      </div>
      {bandsError && (
        <div className="band-subset-popover__error" data-testid="band-subset-bands-error">
          {bandsError}
        </div>
      )}

      <fieldset className="band-subset-popover__mode">
        <legend className="band-subset-popover__mode-legend">Mode</legend>
        <label className="band-subset-popover__mode-row">
          <input
            type="radio"
            name="band-subset-mode"
            checked={!sweepEnabled}
            onChange={selectHoldOne}
            data-testid="band-subset-mode-hold"
          />
          <span>Hold one band — {heldBand}</span>
          <small>default</small>
        </label>
        <label className="band-subset-popover__mode-row">
          <input
            type="radio"
            name="band-subset-mode"
            checked={sweepEnabled}
            disabled={!sweepAllowed}
            onChange={selectSweep}
            data-testid="band-subset-mode-sweep"
          />
          <span>Sweep selected bands</span>
          <small data-testid="band-subset-sweep-caption">{sweepCaption}</small>
        </label>
      </fieldset>

      {fallbackHold && (
        <div className="band-subset-popover__fallback" data-testid="band-subset-fallback-warning">
          ⚠ Sweep paused — radio not responding
        </div>
      )}

      {!isListening && (
        <div className="band-subset-popover__caption" data-testid="band-subset-persist-caption">
          saved — applies at next start (will tune your radio)
        </div>
      )}

      <div className="band-subset-popover__foot">
        Sweep round-robins the radio through each selected band&rsquo;s FT-8 dial. Openness
        dots and the heat layer reflect only what the current dwell sampled.
      </div>
    </div>
  );
}
