// src/radio/sections/SignalSection.tsx
//
// Spec §5.3 — ARDOP signal-quality section. Renders three primitives:
//
//   - Quality big-number indicator (left column, 90 px). Reads
//     ModemStatus.quality (Option<u8> from PINGACK / PING via tuxlink-1637).
//     `null` renders as `—` — pre-ping the dock has no telemetry to display.
//   - S/N trend Sparkline (right column). 60-sample rolling buffer
//     supplied by the panel's useSampleHistory hook; warn-amber below 3 dB,
//     bad-red below 0 dB (typical ARDOP decoder margin).
//   - Recent-frame FrameRibbon below both columns. ARQ subprotocol frames
//     color-coded by type so the operator can read recent on-air traffic
//     at a glance.
//
// VARA HF (future, spec §5.4) is expected to slot the OFDM constellation
// into this same section by passing in an alternate children renderer;
// the structural primitives (Quality + sparkline + recent-frames) line
// up across both panels.

import { Sparkline } from '../charts/Sparkline';
import { FrameRibbon, type ArdopFrameType } from '../charts/FrameRibbon';
import './SignalSection.css';

export interface SignalSectionProps {
  /** ardopcf Quality 0..=100 from the last PINGACK / PING; null = no data yet. */
  quality: number | null;
  /** S/N samples for the trend Sparkline (60 samples ≈ 60 s). Oldest → newest. */
  snrSamples: number[];
  /** Recent ARQ frame types (latest 14 render in the ribbon). */
  recentFrames: ArdopFrameType[];
  /** Latest S/N reading from the modem; rendered as the current value. */
  snrCurrent: number | null;
}

/**
 * Format a signed dB value with an explicit '+' on positives. Negative
 * values get their leading '-' from `toFixed` automatically.
 */
function formatDb(value: number): string {
  return `${value >= 0 ? '+' : ''}${value.toFixed(1)} dB`;
}

export function SignalSection({
  quality,
  snrSamples,
  recentFrames,
  snrCurrent,
}: SignalSectionProps) {
  const avgSnr =
    snrSamples.length > 0
      ? snrSamples.reduce((a, b) => a + b, 0) / snrSamples.length
      : null;
  return (
    <section className="radio-panel-sec signal-section" data-testid="signal-section">
      <h5>Signal</h5>
      <div className="signal-top">
        <div className="quality" data-testid="quality-score">
          <div className="qv">{quality === null ? '—' : quality}</div>
          <div className="qk">Quality</div>
          <div className="qs">/100</div>
        </div>
        <div className="snr-trend">
          <div className="lab-row">
            <span className="k">S/N trend</span>
            <span className="v">
              {snrCurrent === null ? '— dB' : formatDb(snrCurrent)}
            </span>
          </div>
          <Sparkline
            samples={snrSamples}
            height={28}
            warnBelow={3}
            badBelow={0}
          />
          <div className="lab-row signal-axis">
            <span className="signal-axis-tick">−60s</span>
            <span className="signal-axis-avg">
              avg <strong>{avgSnr === null ? '—' : formatDb(avgSnr)}</strong>
            </span>
            <span className="signal-axis-tick">now</span>
          </div>
        </div>
      </div>
      <FrameRibbon frames={recentFrames} />
    </section>
  );
}
