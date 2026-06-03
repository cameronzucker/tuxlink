// useCatalog — one-shot load of the bundled WLE catalog from the Rust side
// via the `catalog_list` Tauri command. The bundled file does not change at
// runtime, so this caches the result for the panel's lifetime.
//
// bd issue: tuxlink-ddiq.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { CatalogEntry } from './types';

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
