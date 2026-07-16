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
//
// Cross-window snapshot handshake (tuxlink-dmwte task 9, spec §7), copied from
// useEnvStations.ts:38-131's host/client mechanics: a freshly-popped Tac Map
// window starts with an empty accumulator and would otherwise show no pins
// until the next beacon (minutes, on a sparse channel). The pop-out (client)
// requests a snapshot on mount; the main shell (host) answers with its
// current per-callsign positions, so the new window shows the live roster
// immediately and keeps updating from the shared event stream thereafter.
//
// Retry amendment (spec §7): unlike useEnvStations' single fire-and-forget
// request, the client here re-emits the request every 250 ms until the first
// reply lands or 3 s elapses — the pop-out window can spin up and register
// its listener before the main shell's host listener is live (no ordering
// guarantee across OS windows), so a single request can go unanswered. The
// bounded retry self-heals that race without polling forever.

import { useEffect, useMemo, useRef, useState } from 'react';
import { listen, emit } from '@tauri-apps/api/event';
import type { HeardPosition, InboundPosDto } from './aprsTypes';

const SNAPSHOT_REQUEST = 'aprs-positions:request-snapshot';
const SNAPSHOT_REPLY = 'aprs-positions:snapshot';
/// Retry cadence for the client's snapshot request (spec §7 retry amendment).
const SNAPSHOT_RETRY_MS = 250;
/// Total time the client keeps retrying before giving up cleanly.
const SNAPSHOT_GIVE_UP_MS = 3000;

/// A heard position is dropped from the map after this long without a re-beacon.
/// Default 3 h: while many stations beacon every ~10–30 min, others (mountaintop
/// digis, fixed weather sites) beacon only ~hourly, so a 1 h TTL dropped pins
/// that were still current. The pin greys at STALE_MS (1 h) first, then drops
/// here. User-configurable timings are a follow-up (tuxlink-uhd7 note).
export const POSITION_TTL_MS = 3 * 60 * 60 * 1000;
/// How often the silent-station sweep runs, so a pin drops even with no new
/// traffic on the channel.
const PRUNE_INTERVAL_MS = 60 * 1000;

export interface UseAprsPositionsOptions {
  /// `'host'` (the main shell, mounted from launch) answers snapshot requests
  /// with its current per-callsign positions. `'client'` (a pop-out window,
  /// e.g. the popped Tac Map) requests + seeds from a snapshot on mount, with
  /// the spec §7 retry amendment (250 ms cadence, 3 s give-up). Omitted ⇒
  /// neither — a standalone instance (e.g. a unit test, or any caller that
  /// predates this handshake) emits/subscribes nothing beyond the existing
  /// `aprs-position:new` feed.
  snapshotRole?: 'host' | 'client';
}

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

export function useAprsPositions(opts?: UseAprsPositionsOptions): UseAprsPositions {
  const role = opts?.snapshotRole;
  // Keyed by callsign so a re-beacon (or a move) overwrites the prior fix.
  const [byCall, setByCall] = useState<Map<string, HeardPosition>>(new Map());
  // Latest accumulator, read by the host's snapshot responder without making
  // the subscription effect depend on `byCall` (mirrors useEnvStations).
  const byCallRef = useRef(byCall);
  byCallRef.current = byCall;

  useEffect(() => {
    let mounted = true;
    const unlisteners: Array<() => void> = [];

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
        unlisteners.push(un);
      })
      .catch(() => {
        // listen() unavailable (jsdom without Tauri — mocked in tests).
      });

    if (role === 'host') {
      // Answer a new window's request with the current roster.
      listen(SNAPSHOT_REQUEST, () => {
        if (!mounted) return;
        void emit(SNAPSHOT_REPLY, [...byCallRef.current.values()]).catch(() => {});
      })
        .then((un) => {
          if (!mounted) un();
          else unlisteners.push(un);
        })
        .catch(() => {});
    }

    if (role === 'client') {
      // Retry state (spec §7 retry amendment): re-emit the request every
      // SNAPSHOT_RETRY_MS until the first reply lands, giving up cleanly
      // after SNAPSHOT_GIVE_UP_MS. Both timers are cleared on reply, on
      // give-up, and on unmount — never left running past any of the three.
      let retryTimer: ReturnType<typeof setInterval> | null = null;
      let giveUpTimer: ReturnType<typeof setTimeout> | null = null;
      const stopRetry = () => {
        if (retryTimer !== null) {
          clearInterval(retryTimer);
          retryTimer = null;
        }
        if (giveUpTimer !== null) {
          clearTimeout(giveUpTimer);
          giveUpTimer = null;
        }
      };
      unlisteners.push(stopRetry);

      // Register the reply listener FIRST, then request — so the host's
      // answer (whenever it arrives) can't be missed.
      listen<HeardPosition[]>(SNAPSHOT_REPLY, (e) => {
        if (!mounted) return;
        stopRetry();
        const incoming = e.payload ?? [];
        setByCall((prev) => {
          const next = new Map(prev);
          for (const p of incoming) {
            const existing = next.get(p.call);
            // Don't clobber a fresher locally-heard fix with an older snapshot.
            if (!existing || existing.at < p.at) next.set(p.call, p);
          }
          return pruneStale(next, Date.now());
        });
      })
        .then((un) => {
          if (!mounted) {
            un();
            return;
          }
          unlisteners.push(un);
          const request = () => void emit(SNAPSHOT_REQUEST).catch(() => {});
          request(); // fire immediately on mount
          retryTimer = setInterval(request, SNAPSHOT_RETRY_MS);
          giveUpTimer = setTimeout(stopRetry, SNAPSHOT_GIVE_UP_MS);
        })
        .catch(() => {});
    }

    // Periodic sweep so a station that goes silent eventually drops off the map
    // even when no further traffic arrives to trigger the per-fix prune.
    const sweep = setInterval(() => {
      if (!mounted) return;
      setByCall((prev) => pruneStale(prev, Date.now()));
    }, PRUNE_INTERVAL_MS);

    return () => {
      mounted = false;
      for (const un of unlisteners) un();
      clearInterval(sweep);
    };
  }, [role]);

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
