// usePersistedBucketFilter (tuxlink-8fjx) — the APRS map's station-category
// filter state: which buckets are shown + whether the layers panel is collapsed.
// Persisted to localStorage per surface (e.g. `tuxlink:map-filter:aprs`), mirroring
// usePersistedViewport. Default (absent/corrupt storage): every bucket ON,
// collapsed — the map draws all heard stations, RF-honest. Unknown stored keys are
// dropped so a future bucket rename degrades gracefully instead of crashing.

import { useCallback, useRef, useState } from 'react';
import { ALL_BUCKET_KEYS, type BucketKey } from '../aprs/stationBuckets';

interface StoredFilter {
  enabled: BucketKey[];
  collapsed: boolean;
}

function getStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function readSaved(key: string): StoredFilter {
  const allOn = (): StoredFilter => ({ enabled: [...ALL_BUCKET_KEYS], collapsed: true });
  const storage = getStorage();
  if (!storage) return allOn();
  try {
    const raw = storage.getItem(key);
    if (!raw) return allOn();
    const v = JSON.parse(raw) as { enabled?: unknown; collapsed?: unknown };
    if (!Array.isArray(v?.enabled)) return allOn();
    const enabled = (v.enabled as unknown[]).filter(
      (k): k is BucketKey => typeof k === 'string' && (ALL_BUCKET_KEYS as string[]).includes(k),
    );
    return { enabled, collapsed: typeof v.collapsed === 'boolean' ? v.collapsed : true };
  } catch {
    return allOn();
  }
}

export interface PersistedBucketFilter {
  enabled: Set<BucketKey>;
  collapsed: boolean;
  toggleBucket: (key: BucketKey) => void;
  setAll: (on: boolean) => void;
  toggleCollapsed: () => void;
}

export function usePersistedBucketFilter(key: string): PersistedBucketFilter {
  const initialRef = useRef<StoredFilter | undefined>(undefined);
  if (initialRef.current === undefined) initialRef.current = readSaved(key);
  const initial = initialRef.current;

  const [enabled, setEnabled] = useState<Set<BucketKey>>(() => new Set(initial.enabled));
  const [collapsed, setCollapsed] = useState<boolean>(initial.collapsed);

  const persist = useCallback(
    (nextEnabled: Set<BucketKey>, nextCollapsed: boolean) => {
      const storage = getStorage();
      if (!storage) return;
      try {
        storage.setItem(
          key,
          JSON.stringify({ enabled: [...nextEnabled], collapsed: nextCollapsed }),
        );
      } catch {
        // best-effort; filter still works in-session
      }
    },
    [key],
  );

  const toggleBucket = useCallback(
    (bucket: BucketKey) => {
      setEnabled((prev) => {
        const next = new Set(prev);
        if (next.has(bucket)) next.delete(bucket);
        else next.add(bucket);
        persist(next, collapsed);
        return next;
      });
    },
    [persist, collapsed],
  );

  const setAll = useCallback(
    (on: boolean) => {
      const next = new Set<BucketKey>(on ? ALL_BUCKET_KEYS : []);
      setEnabled(next);
      persist(next, collapsed);
    },
    [persist, collapsed],
  );

  const toggleCollapsed = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev;
      setEnabled((cur) => {
        persist(cur, next);
        return cur;
      });
      return next;
    });
  }, [persist]);

  return { enabled, collapsed, toggleBucket, setAll, toggleCollapsed };
}
