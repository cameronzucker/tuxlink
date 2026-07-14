// Shared anchor-lookup helper for HintProvider (point-at) and HintOverlay
// (spotlight geometry). tuxlink-10bkw fixwave finding #1.
//
// A `data-tour-anchor` element that IS in the DOM but lays out with a
// zero-size rect is exactly as unusable as "not found": there is nothing to
// spotlight or point at. The confirmed live case is RadioDrawer's
// `.radio-drawer` root (anchor "radio-dock"), which is `display: contents` on
// desktop >=1366px — `getBoundingClientRect()` reports all zeros for a
// `display: contents` element because it generates no box of its own. Without
// this check, the spotlight rendered a hole at the viewport's top-left corner
// and a `point_at` request against it acked "shown" even though nothing was
// actually highlighted.
export function findMountedAnchor(anchorAttr: string): Element | null {
  const el = document.querySelector(`[data-tour-anchor="${anchorAttr}"]`);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return null;
  return el;
}
