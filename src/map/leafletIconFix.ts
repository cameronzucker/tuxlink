/**
 * Shared Leaflet default-marker-icon fix (side-effect module).
 *
 * Leaflet's default icon resolves its image URLs relative to the CSS, which
 * breaks under Vite bundling — markers render as broken-image boxes. Importing
 * this module once (for side effect) rewires the default icon to the bundled
 * marker PNGs. Safe to import from multiple components; the merge is idempotent.
 *
 * Originally inline in `src/compose/PositionMapWidget.tsx`; extracted here (C8)
 * so `BaseMap` and every map consumer share ONE icon fix.
 */
import L from 'leaflet';
import iconUrl from 'leaflet/dist/images/marker-icon.png';
import iconRetinaUrl from 'leaflet/dist/images/marker-icon-2x.png';
import shadowUrl from 'leaflet/dist/images/marker-shadow.png';

delete (L.Icon.Default.prototype as unknown as Record<string, unknown>)._getIconUrl;
L.Icon.Default.mergeOptions({ iconUrl, iconRetinaUrl, shadowUrl });
