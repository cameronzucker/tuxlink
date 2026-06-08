// useCatalog — one-shot load of the bundled WLE catalog from the Rust side
// via the `catalog_list` Tauri command. The bundled file does not change at
// runtime, so this caches the result for the panel's lifetime.
//
// bd issue: tuxlink-ddiq.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { CatalogEntry } from './types';
import type { ListingMode, StationListing, ReplyView } from './stationTypes';

export interface CatalogLoadState {
  entries: CatalogEntry[] | null;
  loading: boolean;
  error: string | null;
}

/// Fetch the catalog. `entries` is `null` until the first load completes;
/// once loaded it stays for the lifetime of the hook caller (no refetch
/// needed — the catalog is a compile-time-bundled file).
export function useCatalog(): CatalogLoadState {
  const [entries, setEntries] = useState<CatalogEntry[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    invoke<CatalogEntry[]>('catalog_list')
      .then((list) => {
        if (mounted) {
          setEntries(list);
          setLoading(false);
        }
      })
      .catch((e: unknown) => {
        if (mounted) {
          const msg = e instanceof Error ? e.message : String(e);
          setError(msg);
          setLoading(false);
        }
      });
    return () => {
      mounted = false;
    };
  }, []);

  return { entries, loading, error };
}

/// Send a catalog inquiry. Returns the MID string on success (mirrors
/// `message_send` contract).
export async function sendCatalogInquiry(filenames: string[]): Promise<string> {
  return invoke<string>('catalog_send_inquiry', { filenames });
}

// --- tuxlink-a2gd: station-list direct poll + reply parse-with-fallback wrappers ---

/// Fetch station lists for the given modes via the polite Rust cache.
/// v1 is PUBLIC-only — serviceCodes is fixed server-side, NOT a caller param.
export async function fetchStations(
  modes: ListingMode[],
  opts?: { historyHours?: number },
): Promise<StationListing[]> {
  return invoke<StationListing[]>('catalog_fetch_stations', {
    modes,
    historyHours: opts?.historyHours ?? 168,
  });
}

/// Parse a received catalog reply into a structured view (or raw). Never rejects on content.
export async function parseReply(subject: string, body: string): Promise<ReplyView> {
  return invoke<ReplyView>('catalog_parse_reply', { subject, body });
}
