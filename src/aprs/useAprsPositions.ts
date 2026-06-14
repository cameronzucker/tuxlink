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

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { HeardPosition, InboundPosDto } from './aprsTypes';

export interface UseAprsPositions {
  /// Heard stations' latest positions, one per callsign (latest-position-wins).
  positions: HeardPosition[];
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
        next.set(p.sender, {
          call: p.sender,
          lat: p.lat,
          lon: p.lon,
          symbolTable: p.symbolTable,
          symbolCode: p.symbolCode,
          comment: p.comment,
          at: Date.now(),
        });
        return next;
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

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  return { positions: [...byCall.values()] };
}
