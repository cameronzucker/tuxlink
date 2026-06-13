/**
 * TileLayerBridge — a stock react-leaflet `TileLayer` over the Tauri `tile`
 * URI scheme. Phase 0 chose the `tile://` scheme (NOT invoke + blob:), so this
 * is a plain `TileLayer` whose template the WebKitGTK webview resolves against
 * the registered `tile` protocol handler — no custom GridLayer, no object-URLs,
 * no `revokeObjectURL`, no leak surface.
 *
 * The Linux template is `tile://localhost/{z}/{x}/{y}` with `subdomains: []`
 * (no `{s}` shard placeholder — there is one local handler). The `tile:` CSP
 * token is live. `tms` follows the source scheme; `maxNativeZoom` is the
 * source's max zoom capped to the app max so Leaflet up-scales its OWN native
 * tiles (rather than 404-ing) above the source's native resolution.
 *
 * ## Cancel-on-pan/zoom (Phase 9.2, §8.5)
 *
 * With the `tile` URI scheme each tile is a plain `<img>` whose `src` is a
 * `tile://…` URL the WebKitGTK protocol handler resolves. Cancellation is
 * therefore handled by Leaflet's NATIVE tile lifecycle — there is NO
 * `AbortController` and NO object-URL here (that machinery belonged to the
 * rejected `invoke` + `blob:` path; an `AbortController` is NOT applicable to a
 * `tile://` `<img>`). On pan/zoom Leaflet fires `tileunload` for every tile that
 * scrolls out of view and REMOVES its `<img>` element; WebKitGTK aborts the
 * in-flight load of a removed `<img>`, so the FRONTEND request is cancelled by
 * that DOM teardown — no per-tile cleanup code is required or possible.
 *
 * The BACKEND fetch the URI handler already spawned for an in-flight tile keeps
 * running, but it is bounded two ways: the 5 s per-fetch timeout
 * (`tiles::fetch::TILE_TIMEOUT`) caps the worst case, and the single-flight
 * de-duplication (`tiles::fetch::fetch_tile_single_flight`) means at most ONE
 * wasted fetch per tile no matter how many times the operator re-pans across it.
 * No `updateWhenIdle`/`keepBuffer` override is set, so the stock buffering
 * applies (Leaflet keeps a small ring of just-off-screen tiles and unloads the
 * rest) — no pile-up of stale in-flight loads.
 *
 * SHAPE is asserted via the test mock (C1); real fetch/render is grim-verified.
 */
import { TileLayer } from 'react-leaflet';
import type { TileSource } from './tileSource';

/** Linux `tile` URI-scheme template the WebKitGTK webview requests tiles from. */
export const TILE_URL_TEMPLATE = 'tile://localhost/{z}/{x}/{y}';

export interface TileLayerBridgeProps {
  /** The validated LAN tile source whose scheme/maxZoom drive the layer. */
  source: TileSource;
  /** The app-wide max zoom; caps `maxNativeZoom` so the source never exceeds it. */
  appMaxZoom: number;
}

export function TileLayerBridge({ source, appMaxZoom }: TileLayerBridgeProps) {
  const maxNativeZoom = Math.min(source.maxZoom, appMaxZoom);
  return (
    <TileLayer
      url={TILE_URL_TEMPLATE}
      subdomains={[]}
      // tms is ALWAYS false here: the `tile://` URL is an internal transport to
      // OUR backend handler, not a real TMS server. Leaflet flips Y for `tms`
      // when it fills the {y} token, and the backend (`build_tile_url` →
      // `TileCoord::upstream_y`) ALSO flips Y for a TMS source — so honoring the
      // source scheme HERE double-flips and serves the vertically-mirrored tile
      // (bd tuxlink-k61j B1). The backend is the SOLE Y-flip site; the webview
      // always speaks standard top-origin XYZ across the scheme boundary.
      tms={false}
      // `minZoom`/`maxZoom` bound the layer to the source's advertised range and
      // let Leaflet up-scale its own native tiles in the [maxNativeZoom, maxZoom]
      // band instead of 404-ing past native resolution. Below `minZoom` no tile
      // is requested, which suppresses spurious coverage-404s (→ false `partial`).
      minZoom={source.minZoom}
      maxZoom={appMaxZoom}
      maxNativeZoom={maxNativeZoom}
      {...(source.attribution ? { attribution: source.attribution } : {})}
    />
  );
}
