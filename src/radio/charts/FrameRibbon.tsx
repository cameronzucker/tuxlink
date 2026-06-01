// src/radio/charts/FrameRibbon.tsx
//
// Spec §5.3 — horizontal ribbon of recent ARQ subprotocol frame types
// (CON / IDLE / DATA / ACK / NAK / REJ). Each cell shows the token with
// a color tied to its type so the operator gets a fast visual read on
// recent on-air traffic without parsing log text. The optional legend
// renders six small color swatches with labels.
//
// Truncation policy: render only the most-recent 14 cells so the ribbon
// fits the 360 px right-panel column. The history buffer upstream
// (`useFrameHistory` in the ARDOP panel) may hold more — the ribbon
// trims at render time.

import './FrameRibbon.css';

export type ArdopFrameType = 'CON' | 'IDLE' | 'DATA' | 'ACK' | 'NAK' | 'REJ';

export interface FrameRibbonProps {
  /** Recent frames, oldest → newest. The last 14 cells render. */
  frames: ArdopFrameType[];
  /** Whether to render the legend below the ribbon. Default true. */
  showLegend?: boolean;
}

const ALL_TYPES: ArdopFrameType[] = ['CON', 'IDLE', 'DATA', 'ACK', 'NAK', 'REJ'];

export function FrameRibbon({ frames, showLegend = true }: FrameRibbonProps) {
  return (
    <>
      <div className="frame-ribbon" data-testid="frame-ribbon">
        {frames.slice(-14).map((f, i) => (
          <div
            key={i}
            className={`frame-cell frame-${f.toLowerCase()}`}
            title={f}
          >
            {f}
          </div>
        ))}
      </div>
      {showLegend && (
        <div className="frame-legend" data-testid="frame-legend">
          {ALL_TYPES.map((t) => (
            <span key={t}>
              <i className={`frame-${t.toLowerCase()}`} />
              {t}
            </span>
          ))}
        </div>
      )}
    </>
  );
}
