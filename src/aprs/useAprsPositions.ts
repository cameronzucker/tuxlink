// src/aprs/useAprsPositions.ts
//
// React hook backing the APRS Tac Chat map. APRS is an OPEN CHANNEL (party
// line): every station's position beacon is heard by all. This hook subscribes
// to the backend `aprs-position:new` event (positions DECODED from heard frames
// — RF-honesty: only real, on-the-wire fixes, never estimated) and accumulates
// the most-recent position per station, deduped by callsign (latest-position-
// wins, so a station that moves replaces its prior pin).
//
// Mirrors useAprsChat.ts's listen pattern (mounted-guarded subscribe, jsdom
// catch). This hook is RX-only — there is no send/config surface.

import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { HeardPosition, InboundPosDto } from './aprsTypes';

/// A heard position is dropped from the map after this long without a re-beacon.
/// Default 3 h: while many stations beacon every ~10–30 min, others (mountaintop
/// digis, fixed weather sites) beacon only ~hourly, so a 1 h TTL dropped pins
/// that were still current. The pin greys at STALE_MS (1 h) first, then drops
/// here. User-configurable timings are a follow-up (tuxlink-uhd7 note).
export const POSITION_TTL_MS = 3 * 60 * 60 * 1000;
/// How often the silent-station sweep runs, so a pin drops even with no new
/// traffic on the channel.
const PRUNE_INTERVAL_MS = 60 * 1000;

export interface UseAprsPositions {
  /// Heard stations' latest positions, one per callsign (latest-position-wins).
  positions: HeardPosition[];
}

/// Drop entries last heard more than [`POSITION_TTL_MS`] ago. Returns the same
/// map reference when nothing expired so React can skip a needless re-render.
function pruneStale(byCall: Map<string, HeardPosition>, now: number): Map<string, HeardPosition> {
  let expired = false;
  for (const v of byCall.values()) {
    if (now - v.at > POSITION_TTL_MS) {
      expired = true;
      break;
    }
  }
  if (!expired) return byCall;
  const next = new Map(byCall);
  for (const [call, v] of next) {
    if (now - v.at > POSITION_TTL_MS) next.delete(call);
  }
  return next;
}

export function useAprsPositions(): UseAprsPositions {
  // Keyed by callsign so a re-beacon (or a move) overwrites the prior fix.
  const [byCall, setByCall] = useState<Map<string, HeardPosition>>(new Map());

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | null = null;

    listen<InboundPosDto>('aprs-position:new', (event) => {
      if (!mounted) return;
      const p = event.payload;
      setByCall((prev) => {
        const next = new Map(prev);
        // The map identity (and pin label) is the ENTITY: an OBJECT/ITEM report
        // is keyed by its name, a station beacon by its callsign. Keying objects
        // by name (not the reporting sender) lets one station report several
        // distinct objects, each getting its own pin / latest-position-wins slot.
        const identity = p.name ?? p.sender;
        next.set(identity, {
          call: identity,
          lat: p.lat,
          lon: p.lon,
          symbolTable: p.symbolTable,
          symbolCode: p.symbolCode,
          comment: p.comment,
          ambiguity: p.ambiguity,
          via: p.via ?? [],
          // An OBJECT/ITEM report (carries `name`) plots the object's location,
          // not the transmitter's — so its via-chain must not be traced (cn84).
          isObject: p.name != null,
          at: Date.now(),
        });
        // Sweep on every fix too, so a busy channel keeps the set trimmed
        // without waiting for the interval tick.
        return pruneStale(next, Date.now());
      });
    })
      .then((un) => {
        if (!mounted) {
          un();
          return;
        }
        unlisten = un;
      })
      .catch(() => {
        // listen() unavailable (jsdom without Tauri — mocked in tests).
      });

    // Periodic sweep so a station that goes silent eventually drops off the map
    // even when no further traffic arrives to trigger the per-fix prune.
    const sweep = setInterval(() => {
      if (!mounted) return;
      setByCall((prev) => pruneStale(prev, Date.now()));
    }, PRUNE_INTERVAL_MS);

    return () => {
      mounted = false;
      unlisten?.();
      clearInterval(sweep);
    };
  }, []);

  // tuxlink-xsv5: memoize the array so its REFERENCE is stable across renders
  // (it changes only when `byCall` actually changes). Returning a fresh
  // `[...byCall.values()]` every render made every `[map, positions]`-keyed map
  // effect (sprite bake, feature-state, the GeoJSON `setData` re-push) re-run on
  // EVERY parent render — including the 1s dashboard clock + 2s drafts poll — so
  // the map re-tiled + force-re-baked every sprite continuously (the "drunk map"
  // CPU storm). Stable ref ⇒ those effects re-run only on a real position change.
  const positions = useMemo(() => [...byCall.values()], [byCall]);
  return { positions };
}
