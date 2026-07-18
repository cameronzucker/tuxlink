// Shared anchor-lookup helper for HintProvider (point-at) and HintOverlay
// (spotlight geometry). tuxlink-10bkw fixwave finding #1.
//
// A `data-tour-anchor` element that IS in the DOM but lays out with a
// zero-size rect is exactly as unusable as "not found": there is nothing to
// spotlight or point at. The confirmed live case was RadioDrawer's
// `.radio-drawer` root hosting the "radio-dock" anchor: it is
// `display: contents` on desktop >=1366px, and `getBoundingClientRect()`
// reports all zeros for a `display: contents` element because it generates no
// box of its own. (tuxlink-fh53x moved that anchor onto boxed elements —
// RadioPanel's root and the APRS dock surface — but the guard stays: without
// it, a zero-rect anchor rendered a spotlight hole at the viewport's top-left
// corner and a `point_at` request against it acked "shown" even though
// nothing was actually highlighted.)
export function findMountedAnchor(anchorAttr: string): Element | null {
  const el = document.querySelector(`[data-tour-anchor="${anchorAttr}"]`);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return null;
  return el;
}
