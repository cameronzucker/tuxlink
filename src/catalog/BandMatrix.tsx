// BandMatrix — the Station tab's band-by-band scan (design §Rail, tuxlink-b026z.4
// Task C3). Unifies the pre-matrix "path forecast" bars + "channels grouped by
// mode" list into ONE row per finder HF band (`bandPlan.HF_BANDS`, which already
// includes 60m) plus a trailing VHF/UHF row:
//
//   band label · FT-8 openness dot (B3's honest-recency bandActivity) ·
//   VOACAP path-reliability bar+% · this station's dial chips for that band
//
// Two lenses share a row but NEVER a color scale or a merged number: the
// openness dot answers "is anyone actually decoding this band right now"
// (recent FT-8 activity); the VOACAP bar answers "does the propagation model
// predict this path reaches the station" (a forecast). Keep them visually and
// numerically distinct.
//
// Dial-chip click semantics are moved here VERBATIM from the pre-matrix
// StationRail (not reimplemented): `rankedDialsFor` + `channelToDial` resolve
// the station's other same-mode channels, and the CLICKED channel is always
// forced to `candidates[0]` (tuxlink-8fkkk — the backend's QSY-on-fail walk
// dials candidates[0] first, and a non-empty list overrides the form target).
// The ☆ save star is a SIBLING of the Use-chip button — never nested inside
// it — preserving the `save-${mode}-${khz}` testids and `aria-pressed`.

import { useState } from 'react';
import { HF_BANDS, bandForKhz, bandLabel, type Band } from './bandPlan';
import { relToTier, tierColorVar, bestBandNow } from './reachability';
import { channelToDial, channelReliability } from './channelGrouping';
import { rankedDialsFor } from './ranking';
import { emitGatewayPrefill } from '../favorites/prefillEvent';
import type { Station, Channel } from './stationModel';
import type { PathPrediction } from './propagationApi';
import type { PredictionStatus } from './useStationPrediction';
import type { FavoriteDial } from '../favorites/types';
import type { BandDot } from '../ft8ui/ft8Types';

/** Rows = the finder's selectable HF allocation (already includes 60m — a
 *  channelized US allocation that is still VOACAP-modeled) plus one trailing
 *  VHF/UHF row (spec §Rail: "finder HF bands + VHF"). */
const ROWS: Band[] = [...HF_BANDS, 'vhf-uhf'];

/** Bands the FT-8 engine structurally cannot sample (openness invariant,
 *  spec §Openness) — mirrors `StationFinderControls`' `NEVER_SAMPLEABLE` set.
 *  Kept local (not imported) since this task's writable scope is
 *  BandMatrix-only and neither module exports the set. */
const NEVER_SAMPLEABLE: Set<Band> = new Set(['60m', 'vhf-uhf']);

const NO_DATA_DOT: BandDot = { tier: 'no-data', opacity: 0, sampledAgoMs: null, dwellSlots: 0 };

const MODE_LABEL: Record<string, string> = {
  'vara-hf': 'VARA HF',
  'ardop-hf': 'ARDOP HF',
  packet: 'Packet',
  pactor: 'Pactor',
  'robust-packet': 'Robust Packet',
};

const mhz = (khz: number): string => (khz / 1000).toFixed(3);

/** Best 2 chips shown inline; a row with a 3rd+ channel collapses the rest
 *  behind a `+N` overflow chip that expands the row in place (spec §Rail). */
const VISIBLE_CHIP_CAP = 2;

function dotTitle(dot: BandDot): string {
  if (dot.tier === 'no-data' || dot.sampledAgoMs === null) {
    return 'not sampled in the last 10 min';
  }
  const ageMin = Math.round(dot.sampledAgoMs / 60_000);
  return `sampled ${ageMin}m ago · dwell ${dot.dwellSlots} slots`;
}

export interface BandMatrixProps {
  station: Station;
  prediction: PathPrediction | null;
  predictionStatus: PredictionStatus;
  utcHour: number;
  /**
   * Live per-band FT-8 openness dots — `useFt8Listener().bandActivity`, itself
   * B3's `deriveBandActivity` output. Optional so BandMatrix stays a pure
   * presentational component pre-D1-wiring: omitting it renders every
   * eligible row with a hollow no-data dot, identical to an empty map (same
   * precedent as `StationFinderControls`' chip dots).
   */
  bandActivity?: Map<string, BandDot>;
  /**
   * Handle a dial-chip click — identical contract to `StationRailProps.onUse`
   * (arm-on-demand: AppShell opens the matching modem then prefills it).
   * Omitted falls back to a bare `emitGatewayPrefill`, matching the
   * pre-matrix StationRail behavior for tests/standalone harnesses.
   */
  onUse?: (dial: FavoriteDial, candidates?: FavoriteDial[]) => void;
  /** Save / unsave a channel as a starred favorite; omitting hides the ☆. */
  onSaveFavorite?: (dial: FavoriteDial) => void;
  /** Whether a channel's dial is already a STARRED favorite (drives ★ fill). */
  isSaved?: (dial: FavoriteDial) => boolean;
}

function DialChip({
  station,
  channel,
  onUse,
  onSaveFavorite,
  isSaved,
}: {
  station: Station;
  channel: Channel;
  onUse: (channel: Channel) => void;
  onSaveFavorite?: (channel: Channel) => void;
  isSaved?: (dial: FavoriteDial) => boolean;
}) {
  const dial = channelToDial(station, channel);
  const dialable = dial != null;
  const saved = dialable && isSaved ? isSaved(dial) : false;
  return (
    // The ☆ save star below is a SIBLING of the Use-chip button inside this
    // wrapper — never nested inside it (spec §Rail; the anti-pattern this
    // component's own review gate checks for). Visual grouping is CSS only.
    <span className="station-finder__bmchip">
      <button
        type="button"
        data-testid={`use-${channel.mode}-${channel.frequencyKhz}`}
        className="station-finder__chipuse"
        disabled={!dialable}
        title={
          !dialable
            ? 'No tuxlink modem for this mode'
            : `Open the ${MODE_LABEL[channel.mode] ?? channel.mode} modem and prefill this channel`
        }
        onClick={() => onUse(channel)}
      >
        <span className={`station-finder__sw station-finder__sw--${channel.mode}`} />
        {mhz(channel.frequencyKhz)}
      </button>
      {onSaveFavorite && (
        <button
          type="button"
          data-testid={`save-${channel.mode}-${channel.frequencyKhz}`}
          className={`station-finder__save${saved ? ' is-saved' : ''}`}
          disabled={!dialable}
          aria-pressed={saved}
          title={
            !dialable
              ? 'No tuxlink modem for this mode'
              : saved
                ? 'Remove from favorites'
                : 'Save to favorites'
          }
          onClick={() => onSaveFavorite(channel)}
        >
          {saved ? '★' : '☆'}
        </button>
      )}
    </span>
  );
}

export function BandMatrix(props: BandMatrixProps) {
  const { station, prediction, predictionStatus, utcHour, bandActivity, onSaveFavorite, isSaved } = props;
  const [expanded, setExpanded] = useState<Set<Band>>(new Set());

  const best = predictionStatus === 'ok' && prediction ? bestBandNow(prediction, utcHour) : null;

  const onUseChannel = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (!dial) return;
    // Verbatim from the pre-matrix StationRail `onUse` (tuxlink-8fkkk Task B):
    // the clicked dial MUST lead candidates — `rankedDialsFor` is best-first
    // and may reorder (or, under its cap, even omit) the clicked channel, so
    // force it to the front and append the rest, deduped.
    const ranked = rankedDialsFor(station, dial.mode, prediction, utcHour);
    const sameDial = (a: FavoriteDial, b: FavoriteDial) => a.gateway === b.gateway && a.freq === b.freq;
    const candidates = [dial, ...ranked.filter((d) => !sameDial(d, dial))];
    if (props.onUse) props.onUse(dial, candidates);
    else emitGatewayPrefill(dial, candidates);
  };

  const onSaveChannel = (channel: Channel) => {
    const dial = channelToDial(station, channel);
    if (!dial || !onSaveFavorite) return;
    onSaveFavorite(dial);
  };

  const toggleExpanded = (band: Band) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(band)) next.delete(band);
      else next.add(band);
      return next;
    });
  };

  return (
    <div className="station-finder__bandmatrix" data-testid="band-matrix">
      <div className="station-finder__bmheader" data-testid="bandmatrix-header">
        {predictionStatus === 'ok' && prediction ? (
          <>
            Bands &amp; channels · you → {station.baseCallsign}
            {best && <span className="station-finder__best">best now: {bandLabel(best.band)}</span>}
          </>
        ) : predictionStatus === 'no-location' ? (
          'Set your location in the status bar to see the path forecast.'
        ) : (
          'Forecast unavailable — showing channels without reliability.'
        )}
      </div>

      {ROWS.map((band) => {
        const isVhf = band === 'vhf-uhf';
        const pc =
          !isVhf && prediction ? prediction.channels.find((c) => bandForKhz(c.frequencyKhz) === band) : undefined;
        const rel = predictionStatus === 'ok' && pc ? pc.relByHour[utcHour] ?? 0 : null;

        const channels = station.channels.filter((ch) => ch.band === band);
        const sortedChannels = channels
          .map((ch) => ({
            ch,
            rel: prediction ? (channelReliability(ch, prediction, utcHour)?.rel ?? null) : null,
          }))
          .sort((a, b) => {
            if (a.rel != null && b.rel != null) return b.rel - a.rel;
            if (a.rel != null) return -1;
            if (b.rel != null) return 1;
            return a.ch.frequencyKhz - b.ch.frequencyKhz;
          })
          .map((x) => x.ch);

        const visible = sortedChannels.slice(0, VISIBLE_CHIP_CAP);
        const hidden = sortedChannels.slice(VISIBLE_CHIP_CAP);
        const isExpanded = expanded.has(band);

        const showDot = !NEVER_SAMPLEABLE.has(band);
        const dot = showDot ? (bandActivity?.get(band) ?? NO_DATA_DOT) : null;

        return (
          <div
            key={band}
            className={`station-finder__bmrow${best?.band === band ? ' is-best' : ''}${
              channels.length === 0 ? ' is-empty' : ''
            }`}
            data-testid={`bandmatrix-row-${band}`}
          >
            <span className="station-finder__bn">{bandLabel(band)}</span>
            {dot ? (
              <span
                className={`station-finder__dot station-finder__dot--${dot.tier}`}
                style={{ opacity: dot.opacity }}
                title={dotTitle(dot)}
                aria-hidden="true"
                data-testid={`bandmatrix-dot-${band}`}
              />
            ) : (
              // Never-sampleable band (60m / VHF-UHF): render NO dot at all —
              // not a hollow one — per the openness invariant. The spacer
              // keeps the row's grid columns aligned without asserting
              // knowledge the FT-8 engine structurally cannot have.
              <span className="station-finder__bmdotslot" aria-hidden="true" />
            )}
            <span className="station-finder__bmforecast">
              {isVhf ? (
                <span className="station-finder__pct">LoS</span>
              ) : (
                <>
                  <div className="station-finder__track">
                    <div
                      className="station-finder__fill"
                      style={{
                        width: `${Math.round((rel ?? 0) * 100)}%`,
                        background: rel != null ? tierColorVar(relToTier(rel)) : undefined,
                      }}
                    />
                  </div>
                  <span className="station-finder__pct">{rel != null ? `${Math.round(rel * 100)}%` : '—'}</span>
                </>
              )}
            </span>
            <div className="station-finder__bmchips">
              {channels.length === 0 ? (
                <span className="station-finder__bmnone">no channel</span>
              ) : (
                <>
                  {visible.map((ch) => (
                    <DialChip
                      key={`${ch.mode}-${ch.frequencyKhz}-${ch.ssid ?? ''}`}
                      station={station}
                      channel={ch}
                      onUse={onUseChannel}
                      onSaveFavorite={onSaveFavorite ? onSaveChannel : undefined}
                      isSaved={isSaved}
                    />
                  ))}
                  {isExpanded &&
                    hidden.map((ch) => (
                      <DialChip
                        key={`${ch.mode}-${ch.frequencyKhz}-${ch.ssid ?? ''}`}
                        station={station}
                        channel={ch}
                        onUse={onUseChannel}
                        onSaveFavorite={onSaveFavorite ? onSaveChannel : undefined}
                        isSaved={isSaved}
                      />
                    ))}
                  {hidden.length > 0 && (
                    <button
                      type="button"
                      className="station-finder__more"
                      data-testid={`bandmatrix-more-${band}`}
                      aria-expanded={isExpanded}
                      onClick={() => toggleExpanded(band)}
                    >
                      {isExpanded ? 'show less' : `+${hidden.length}`}
                    </button>
                  )}
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
