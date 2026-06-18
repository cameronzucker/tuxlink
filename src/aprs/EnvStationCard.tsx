// src/aprs/EnvStationCard.tsx
//
// One station's card in the source-reactive environmental panel (tuxlink-2phz).
// The card AUTO-COMPOSES from the channels the station actually emits — there is
// no weather / telemetry mode. A `kind → renderer` map routes each channel:
//   wind_dir  → a compass widget (angular)
//   pressure  → a graded chart + a computed rise/fall trend
//   rain      → a totals + fill-bar block (not a time series)
//   else      → a graded X/Y-grid chart (temp, humidity, generic T#, …)
//   bits      → LED pills (honoring the BITS sense)
// Graded charts share one time axis so a column reads all channels at one
// instant. RF-honesty: unscaled telemetry shows as a raw count, absent channels
// are simply not drawn, and a stale station is dimmed.

import type { ChannelSample, EnvChannel, EnvStation } from './envStations';
import { isStale } from './envStations';

/// Channel kinds rendered as a stacked graded chart (everything that is a
/// time-series magnitude). wind_dir/speed/gust render in the compass block;
/// rain renders as a totals block.
const CHART_KINDS = new Set(['temperature', 'humidity', 'pressure', 'luminosity', 'snow', 'generic']);

const SVG_W = 240;
const SVG_H = 46;

/// A nicely-rounded [min, max] enclosing the samples, with a little headroom so
/// the line never rides the chart edge. A flat series gets a symmetric band so
/// it renders as a centered line rather than a divide-by-zero.
function yRange(samples: ChannelSample[]): [number, number] {
  const vals = samples.map((s) => s.value);
  let lo = Math.min(...vals);
  let hi = Math.max(...vals);
  if (lo === hi) {
    const pad = Math.max(Math.abs(hi) * 0.05, 1);
    return [lo - pad, hi + pad];
  }
  const pad = (hi - lo) * 0.12;
  return [lo - pad, hi + pad];
}

function fmtTick(v: number): string {
  if (Math.abs(v) >= 100) return v.toFixed(0);
  if (Math.abs(v) >= 10) return v.toFixed(0);
  return v.toFixed(1);
}

/// A graded X/Y-grid mini-chart: faint grid, area fill under the line, the line,
/// and a current-value dot. Reads magnitude (Y labeled), not just trend.
function GradedChart({ channel }: { channel: EnvChannel }) {
  const h = channel.history.length > 0 ? channel.history : [{ value: channel.value, at: 0 }];
  const [lo, hi] = yRange(h);
  const span = hi - lo;
  const n = h.length;
  const x = (i: number) => (n === 1 ? SVG_W : (i / (n - 1)) * SVG_W);
  const y = (v: number) => SVG_H - ((v - lo) / span) * SVG_H;
  const pts = h.map((s, i) => `${x(i).toFixed(1)},${y(s.value).toFixed(1)}`).join(' ');
  const area = `0,${SVG_H} ${pts} ${SVG_W},${SVG_H}`;
  const lastX = x(n - 1);
  const lastY = y(h[n - 1].value);
  return (
    <div className="env-plot" data-testid={`env-chart-${channel.key}`}>
      <div className="env-grid" aria-hidden="true" />
      <svg viewBox={`0 0 ${SVG_W} ${SVG_H}`} preserveAspectRatio="none" aria-hidden="true">
        <polygon className="env-area" points={area} />
        <polyline className="env-line" points={pts} fill="none" />
        <circle className="env-dot" cx={lastX} cy={lastY} r={2.4} />
      </svg>
      <span className="env-yhi">{fmtTick(hi)}</span>
      <span className="env-ylo">{fmtTick(lo)}</span>
    </div>
  );
}

type Trend = { dir: 'rising' | 'falling' | 'steady'; delta: number };

/// Rise/fall over the channel's buffered history. APRS pressure trend is the
/// classic use; a <0.3 hPa swing reads as steady (instrument noise).
function trendOf(channel: EnvChannel, steadyEps: number): Trend {
  const h = channel.history;
  if (h.length < 2) return { dir: 'steady', delta: 0 };
  const delta = h[h.length - 1].value - h[0].value;
  if (Math.abs(delta) < steadyEps) return { dir: 'steady', delta };
  return { dir: delta > 0 ? 'rising' : 'falling', delta };
}

function PressureTrend({ channel }: { channel: EnvChannel }) {
  const t = trendOf(channel, 0.3);
  const glyph = t.dir === 'rising' ? '▲' : t.dir === 'falling' ? '▼' : '→';
  return (
    <span className={`env-trend is-${t.dir}`} data-testid="env-pressure-trend">
      {glyph} {t.dir} {t.delta !== 0 ? `${t.delta > 0 ? '+' : ''}${t.delta.toFixed(1)}` : ''}
    </span>
  );
}

/// Wind compass: an arrow pointing FROM the reported bearing toward the center
/// (meteorological convention — wind FROM 270° draws from the west edge inward).
function WindCompass({ dirDeg, speed }: { dirDeg: number; speed: EnvChannel | undefined }) {
  // Arrow tail sits on the rim at the FROM bearing; head points to center.
  const r = 38;
  const rad = (dirDeg * Math.PI) / 180;
  // Screen coords: 0°=N=up, 90°=E=right. tail at the bearing, head toward center.
  const tailX = 54 + r * Math.sin(rad);
  const tailY = 54 - r * Math.cos(rad);
  const headX = 54 + 14 * Math.sin(rad);
  const headY = 54 - 14 * Math.cos(rad);
  return (
    <svg className="env-compass" data-testid="env-compass" viewBox="0 0 108 108">
      <circle cx="54" cy="54" r="46" className="env-compass-face" />
      <circle cx="54" cy="54" r="30" className="env-compass-ring" fill="none" />
      <line className="env-compass-arrow" x1={tailX} y1={tailY} x2={headX} y2={headY} />
      <circle className="env-compass-head" cx={headX} cy={headY} r={3} />
      <text x="54" y="15" textAnchor="middle" className="env-compass-card">N</text>
      <text x="98" y="58" textAnchor="middle" className="env-compass-card">E</text>
      <text x="54" y="103" textAnchor="middle" className="env-compass-card">S</text>
      <text x="11" y="58" textAnchor="middle" className="env-compass-card">W</text>
      {speed && (
        <>
          <text x="54" y="52" textAnchor="middle" className="env-compass-speed">{Math.round(speed.value)}</text>
          <text x="54" y="64" textAnchor="middle" className="env-compass-unit">{speed.unit}</text>
        </>
      )}
    </svg>
  );
}

function compassBearing(deg: number): string {
  const dirs = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  return dirs[Math.round(deg / 45) % 8];
}

function fmtAgo(ms: number): string {
  if (ms < 0) ms = 0;
  const min = ms / 60000;
  if (min < 1) return 'now';
  if (min < 90) return `−${Math.round(min)}m`;
  return `−${(min / 60).toFixed(min / 60 < 10 ? 1 : 0)}h`;
}

/// The four shared time-axis ticks, derived from the actual buffered span of the
/// charted channels (telemetry fills minutes, weather hours — the axis adapts).
function SharedTimeAxis({ spanMs }: { spanMs: number }) {
  const ticks = [spanMs, (spanMs * 2) / 3, spanMs / 3, 0];
  return (
    <div className="env-xaxis" data-testid="env-xaxis">
      {ticks.map((t, i) => (
        <span key={i}>{fmtAgo(t)}</span>
      ))}
    </div>
  );
}

export function EnvStationCard({ station, now }: { station: EnvStation; now: number }) {
  const stale = isStale(station, now);
  const windDir = station.channels.find((c) => c.kind === 'wind_dir');
  const windSpeed = station.channels.find((c) => c.kind === 'wind_speed');
  const windGust = station.channels.find((c) => c.kind === 'wind_gust');
  const charted = station.channels.filter((c) => CHART_KINDS.has(c.kind));
  const hasRaw = station.channels.some((c) => !c.scaled);

  // Shared axis span = oldest sample among the charted channels → now.
  let oldest = now;
  for (const c of charted) {
    if (c.history.length > 0 && c.history[0].at > 0) oldest = Math.min(oldest, c.history[0].at);
  }
  const spanMs = Math.max(now - oldest, 0);

  return (
    <div
      className={`env-card${stale ? ' is-stale' : ''}`}
      data-testid={`env-card-${station.call}`}
    >
      <div className="env-chead">
        <span className="env-call">{station.call}</span>
        {station.project && <span className="env-proj">{station.project}</span>}
        <span className="env-meta">
          {hasRaw && <span className="env-chip is-raw" title="No EQNS heard — values are raw counts">raw counts</span>}
          <span className={`env-chip ${stale ? 'is-stale' : 'is-fresh'}`}>
            {stale ? `stale · ${fmtAgo(now - station.lastHeard).replace('−', '')}` : 'live'}
          </span>
          {station.seq !== null && <span className="env-seq">#{station.seq}</span>}
        </span>
      </div>

      {windDir && (
        <div className="env-wind">
          <WindCompass dirDeg={windDir.value} speed={windSpeed} />
          <div className="env-wind-stats">
            <div className="env-stat">
              <div className="env-stat-k">Wind</div>
              <div className="env-stat-v">{compassBearing(windDir.value)} {Math.round(windDir.value)}°</div>
            </div>
            {windGust && (
              <div className="env-stat">
                <div className="env-stat-k">Gust</div>
                <div className="env-stat-v">{Math.round(windGust.value)}<small>{windGust.unit}</small></div>
              </div>
            )}
          </div>
        </div>
      )}

      {charted.length > 0 && (
        <div className="env-charts">
          {charted.map((c) => (
            <div className="env-chan" key={c.key}>
              <div className="env-cn">
                <b>{c.label}</b>
                <span>
                  {c.scaled ? c.unit : 'raw'}
                  {c.kind === 'pressure' && <> · <PressureTrend channel={c} /></>}
                </span>
              </div>
              <GradedChart channel={c} />
              <div className="env-cval">
                {c.scaled ? c.value.toFixed(c.value % 1 === 0 ? 0 : 1) : Math.round(c.value)}
                <span>{c.scaled ? c.unit : 'raw'}</span>
              </div>
            </div>
          ))}
          {spanMs > 0 && <SharedTimeAxis spanMs={spanMs} />}
        </div>
      )}

      {station.rain && (station.rain.in1h !== null || station.rain.in24h !== null || station.rain.sinceMidnight !== null) && (
        <div className="env-rain" data-testid="env-rain">
          <span className="env-rain-label">Rain</span>
          <div className="env-rainbar">
            <i style={{ width: `${Math.min((station.rain.in1h ?? 0) * 100, 100)}%` }} />
          </div>
          <span className="env-rain-vals">
            {station.rain.in1h !== null && <><b>{station.rain.in1h.toFixed(2)}"</b> 1h</>}
            {station.rain.in24h !== null && <> · <b>{station.rain.in24h.toFixed(2)}"</b> 24h</>}
            {station.rain.sinceMidnight !== null && <> · <b>{station.rain.sinceMidnight.toFixed(2)}"</b> today</>}
          </span>
        </div>
      )}

      {station.bits.length > 0 && (
        <div className="env-bits" data-testid="env-bits">
          {station.bits.map((b) => {
            const on = b.value === b.sense;
            return (
              <span key={b.key} className={`env-bit${on ? ' is-on' : ''}`}>
                <span className="env-led" aria-hidden="true" />
                {b.label}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
