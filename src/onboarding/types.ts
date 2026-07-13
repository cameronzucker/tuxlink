export type HintFallback = 'skip' | 'center';

export interface HintEntry {
  id: string;                    // anchor id == entry id
  anchor: string;                // data-tour-anchor attribute value (same as id)
  title: string;
  body: string;
  requiredPanelState?: string;   // key into the probe registry; absent = always ok
  fallback: HintFallback;
  openHint?: string;             // "how to open this surface" — point_at unmounted error
}

export type PanelStateProbe = () => boolean;
