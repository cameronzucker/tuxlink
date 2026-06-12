// Same-window event signalling that the active LAN tile source changed, so an
// already-mounted map (via useTileSource) re-reads the source without an app
// restart. Mirrors the favorites/prefillEvent.ts pattern. (tuxlink-9rek)
//
// Without this, `useTileSource` reads config only on mount: clicking "Use this
// source" in Settings sets the gatekeeper live + persists, but a map mounted
// before that click never picks up the new source until the app restarts.

export const TILE_SOURCE_CHANGED_EVENT = 'tuxlink:tile-source-changed';

/** Notify any mounted map that the configured tile source changed. */
export function emitTileSourceChanged(): void {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent(TILE_SOURCE_CHANGED_EVENT));
}

/** Subscribe to tile-source changes; returns an unsubscribe fn. */
export function listenTileSourceChanged(onChange: () => void): () => void {
  if (typeof window === 'undefined') return () => {};
  const handler = () => onChange();
  window.addEventListener(TILE_SOURCE_CHANGED_EVENT, handler);
  return () => window.removeEventListener(TILE_SOURCE_CHANGED_EVENT, handler);
}
