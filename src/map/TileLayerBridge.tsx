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
 * source's max zoom capped to the app max so Leaflet up-scales (rather than
 * 404s) above the source's native resolution.
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
      tms={source.scheme === 'Tms'}
      maxNativeZoom={maxNativeZoom}
      {...(source.attribution ? { attribution: source.attribution } : {})}
    />
  );
}
