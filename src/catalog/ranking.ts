// Ordered QSY-candidate ranking for Find-a-Station (tuxlink-8fkkk Task B).
//
// When the operator picks a channel via "Use →", the panel can walk the
// station's OTHER channels for that mode if the primary dial fails to connect
// (the backend's qsy_on_fail walk). This builds the ordered candidate list the
// panel sends as `qsyCandidates`. Ordering mirrors Find-a-Station's own
// reliability ranking: best-first when a path prediction is available, else the
// existing frequency-ascending order (groupChannelsByMode).

import type { Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import { channelToDial, channelReliability, groupChannelsByMode } from './channelGrouping';
import type { FavoriteDial, RadioMode } from '../favorites/types';
import type { ListingMode } from './stationTypes';

/** Cap the candidate list so a QSY walk visits a sane number of frequencies. */
export const RANKED_DIALS_CAP = 5;

/**
 * Ordered dials for `station`'s channels in `mode`, ranked the way Find-a-Station
 * ranks them: reliability DESC when `prediction` (+ `utcHour`) is supplied and a
 * per-channel reliability is computable, otherwise frequency ASC (the
 * `groupChannelsByMode` order). Channels `channelToDial` cannot map are dropped.
 * Capped to {@link RANKED_DIALS_CAP}.
 */
export function rankedDialsFor(
  station: Station,
  mode: RadioMode,
  prediction?: PathPrediction | null,
  utcHour?: number,
): FavoriteDial[] {
  // Find this mode's channels in the canonical freq-ascending grouping.
  const group = groupChannelsByMode(station).find((g) => g.mode === (mode as ListingMode));
  if (!group) return [];

  // Pair each channel with its dial (dropping unmappable ones) + reliability.
  const ranked = group.channels
    .map((ch) => {
      const dial = channelToDial(station, ch);
      if (!dial) return null;
      const rel =
        prediction && utcHour != null
          ? channelReliability(ch, prediction, utcHour)?.rel ?? null
          : null;
      return { dial, rel };
    })
    .filter((x): x is { dial: FavoriteDial; rel: number | null } => x !== null);

  // Reliability ranking only applies when at least one channel has a value;
  // otherwise keep the frequency-ascending order verbatim. A stable sort keeps
  // the freq-asc order as the tiebreaker between equal-reliability channels.
  const anyRel = ranked.some((x) => x.rel != null);
  const ordered = anyRel
    ? ranked.slice().sort((a, b) => (b.rel ?? -1) - (a.rel ?? -1))
    : ranked;

  return ordered.slice(0, RANKED_DIALS_CAP).map((x) => x.dial);
}
