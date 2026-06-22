// useWinlinkLayerToggle (tuxlink-s1o1 Task 8) — persisted boolean gate for the
// Winlink map layer (diamonds + link arc). Mirrors usePersistedBucketFilter's
// storage-guard + safe-default + useRef initial-read + useCallback persist idiom.
//
// Persisted under localStorage key `tuxlink:winlink-layer` as
//   { on: boolean, withinHours: number }
// Default (absent/corrupt storage): { on: false, withinHours: 6 }

import { useCallback, useRef, useState } from 'react';

const STORAGE_KEY = 'tuxlink:winlink-layer';

interface StoredToggle {
  on: boolean;
  withinHours: number;
}

function getStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

function readSaved(): StoredToggle {
  const defaults = (): StoredToggle => ({ on: false, withinHours: 6 });
  const storage = getStorage();
  if (!storage) return defaults();
  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (!raw) return defaults();
    const v = JSON.parse(raw) as { on?: unknown; withinHours?: unknown };
    if (typeof v?.on !== 'boolean' || typeof v?.withinHours !== 'number') return defaults();
    return { on: v.on, withinHours: v.withinHours };
  } catch {
    return defaults();
  }
}

export interface WinlinkLayerToggle {
  on: boolean;
  toggle: () => void;
  withinHours: number;
  setWithinHours: (h: number) => void;
}

export function useWinlinkLayerToggle(): WinlinkLayerToggle {
  const initialRef = useRef<StoredToggle | undefined>(undefined);
  if (initialRef.current === undefined) initialRef.current = readSaved();
  const initial = initialRef.current;

  const [on, setOn] = useState<boolean>(initial.on);
  const [withinHours, setWithinHoursState] = useState<number>(initial.withinHours);

  const persist = useCallback((nextOn: boolean, nextHours: number) => {
    const storage = getStorage();
    if (!storage) return;
    try {
      storage.setItem(STORAGE_KEY, JSON.stringify({ on: nextOn, withinHours: nextHours }));
    } catch {
      // best-effort; toggle still works in-session
    }
  }, []);

  const toggle = useCallback(() => {
    setOn((prev) => {
      const next = !prev;
      setWithinHoursState((h) => {
        persist(next, h);
        return h;
      });
      return next;
    });
  }, [persist]);

  const setWithinHours = useCallback(
    (h: number) => {
      setWithinHoursState(h);
      setOn((cur) => {
        persist(cur, h);
        return cur;
      });
    },
    [persist],
  );

  return { on, toggle, withinHours, setWithinHours };
}
