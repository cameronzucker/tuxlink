import type { SurfaceId } from './dockState';

/** Task 7 fills this in — renders the title bar + surface component + status
 * strip for a popped surface's own OS window (spec §4). Placeholder here so
 * Task 6's App.tsx route branch typechecks. */
export function PoppedSurfaceHost({ surface }: { surface: SurfaceId }): null {
  void surface;
  return null;
}
