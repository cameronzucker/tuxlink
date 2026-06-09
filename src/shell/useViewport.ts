import { useEffect, useState } from 'react';

/**
 * The single source of truth for the FZ-M1 compact breakpoint. The CSS
 * `@media (max-width: 1365px) and (any-pointer: coarse)` blocks (compactShell.css,
 * RadioDrawer.css, wizard.css, dialog/forms/compose compact blocks) MUST mirror
 * this exact string.
 * Because the hook evaluates the identical media query via `matchMedia`, the
 * CSS-driven layout and the JS-driven interactive state can never disagree
 * about whether we are in compact mode.
 *
 * Strictly below 1366px so the "desktop >=1366px is unchanged" invariant holds
 * with no boundary overlap (Codex adrev R1 #1).
 *
 * tuxlink-mt73: ALSO gated on `any-pointer: coarse` (the device has a
 * touch-capable pointer). Width alone over-fired: the Compose window is a
 * SEPARATE webview at a fixed 1100px width, so a width-only query put a normal
 * desktop Compose window into FZ-M1 touch mode. Requiring a coarse pointer
 * keeps the FZ-M1 touchscreen compact while a mouse-only desktop stays desktop
 * at any window width. (`any-pointer`, not `pointer`, so a docked mouse on the
 * FZ-M1 does not disable touch mode — the touchscreen still counts.)
 *
 * tuxlink-h7q7 / docs/design/2026-06-07-fzm1-responsive-design.md.
 */
export const COMPACT_MEDIA_QUERY = '(max-width: 1365px) and (any-pointer: coarse)';

export interface Viewport {
  /** True when the viewport is at/below the FZ-M1 compact breakpoint. */
  isCompact: boolean;
}

/**
 * Reports whether the app is in compact (tablet) mode. Used only for the
 * JS-stateful compact bits (the radio slide-over drawer open/close + the
 * icon-rail expand overlay). All *static* compact layout/typography lives in
 * CSS media queries that need no JS — see the design's §Components.
 */
export function useViewport(): Viewport {
  const [isCompact, setIsCompact] = useState<boolean>(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return false;
    return window.matchMedia(COMPACT_MEDIA_QUERY).matches;
  });

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia(COMPACT_MEDIA_QUERY);
    const onChange = (e: MediaQueryListEvent) => setIsCompact(e.matches);
    setIsCompact(mql.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  return { isCompact };
}
