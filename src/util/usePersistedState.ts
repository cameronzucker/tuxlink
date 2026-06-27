// usePersistedState (tuxlink-9hjw3) — a small `useState` that survives unmount
// by mirroring to localStorage under a `tuxlink:` key. For TRANSIENT view state
// (toggles, last-used tab, filter text) that should feel sticky across a
// close/reopen — NOT operator config (that lives in the backend config store).
//
// Mirrors the safe-storage + validate-on-read posture of `usePersistedViewport`
// (tuxlink-dwzu): the `window.localStorage` getter itself can throw (storage
// disabled / opaque origin / strict privacy), and a stored value can be
// cross-version or hand-edited garbage — both degrade to `initial`, never crash
// the render.

import { useCallback, useState } from 'react';

/** Resolve localStorage defensively — the getter can throw, not just return
 *  null (Codex adrev pattern from usePersistedViewport). */
function getStorage(): Storage | null {
  try {
    if (typeof window === 'undefined') return null;
    return window.localStorage ?? null;
  } catch {
    return null;
  }
}

/** Read + JSON-parse a persisted value, validating it with `isValid`. Returns
 *  `initial` on absent / corrupt / invalid data. */
function readPersisted<T>(storageKey: string, initial: T, isValid: (v: unknown) => v is T): T {
  const storage = getStorage();
  if (!storage) return initial;
  try {
    const raw = storage.getItem(storageKey);
    if (raw === null) return initial;
    const parsed = JSON.parse(raw) as unknown;
    return isValid(parsed) ? parsed : initial;
  } catch {
    return initial;
  }
}

/**
 * A `useState` whose value persists to localStorage under `tuxlink:<key>`.
 *
 * - The initial read happens ONCE at mount (lazy initializer), so a remount
 *   (panel close/reopen) restores the operator's last value instead of the
 *   default.
 * - `isValid` guards against cross-version / hand-edited garbage — an invalid
 *   stored value falls back to `initial`.
 * - Writes are best-effort; a full/blocked store keeps the in-memory value.
 */
export function usePersistedState<T>(
  key: string,
  initial: T,
  isValid: (v: unknown) => v is T,
): [T, (next: T) => void] {
  const storageKey = `tuxlink:${key}`;
  const [value, setValue] = useState<T>(() => readPersisted(storageKey, initial, isValid));

  const set = useCallback(
    (next: T) => {
      setValue(next);
      const storage = getStorage();
      if (!storage) return;
      try {
        storage.setItem(storageKey, JSON.stringify(next));
      } catch {
        // storage full / disabled — keep the in-memory value, skip persistence.
      }
    },
    [storageKey],
  );

  return [value, set];
}
