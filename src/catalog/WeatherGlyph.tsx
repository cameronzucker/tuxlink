// WeatherGlyph — renders an NWS SFT condition code as a custom inline-SVG
// weather icon (tuxlink-n6tp). Shapes encode the sky-cover gradient (sun shrinks
// / cloud grows from Sunny → Cloudy); colours come from CSS classes so they theme
// with the app's palette tokens and render identically under WebKitGTK (SVG
// fill/stroke inherit down the tree, so one class on a <g> colours the whole sun).
// Unmapped codes fall back to today's raw-text span — the grid never blanks.
// Approved design: dev/scratch/2026-06-10-nws-weather-glyphs-mock.html.

import type { JSX } from 'react';
import { resolveGlyph, conditionTextClass, type WeatherGlyph as Glyph } from './weatherGlyph';
import './WeatherGlyph.css';

const A8 = [0, 1, 2, 3, 4, 5, 6, 7].map((i) => (i * Math.PI) / 4); // full 8 rays
const ATL = [Math.PI, Math.PI * 1.25, Math.PI * 1.5, Math.PI * 1.75]; // left + top only

const CLOUD_D = 'M2 8 a3 3 0 0 1 0.6-5.9 A4.2 4.2 0 0 1 11 1.6 a3.3 3.3 0 0 1 5.2 2.2 A2.8 2.8 0 0 1 16 8 z';

function Sun({ cx, cy, r, cls, angles }: { cx: number; cy: number; r: number; cls: string; angles: number[] }) {
  return (
    <g className={cls}>
      <circle cx={cx} cy={cy} r={r} />
      {angles.map((a, i) => (
        <line
          key={i}
          x1={(cx + Math.cos(a) * (r + 1.5)).toFixed(1)}
          y1={(cy + Math.sin(a) * (r + 1.5)).toFixed(1)}
          x2={(cx + Math.cos(a) * (r + 3.4)).toFixed(1)}
          y2={(cy + Math.sin(a) * (r + 3.4)).toFixed(1)}
        />
      ))}
    </g>
  );
}

function Cloud({ tx, ty, s, cls }: { tx: number; ty: number; s: number; cls: string }) {
  return <path className={cls} transform={`translate(${tx} ${ty}) scale(${s})`} d={CLOUD_D} strokeWidth={1.6 / s} />;
}

function drops(xs: number[], len: number) {
  return (
    <g className="wx-rain">
      {xs.map((x) => (
        <line key={x} x1={x} y1={15.5} x2={x - 0.8} y2={(15.5 + len).toFixed(1)} />
      ))}
    </g>
  );
}

function shape(g: Glyph): JSX.Element {
  const sunCls = g.accent === 'danger' ? 'wx-danger' : 'wx-sun'; // hot + sun share warn
  switch (g.kind) {
    case 'sunny':
      return <Sun cx={12} cy={12} r={5} cls={sunCls} angles={A8} />;
    case 'mosunny': // BIG sun, tiny cloud — sun dominant
      return (
        <>
          <Sun cx={10} cy={9.5} r={4.4} cls={sunCls} angles={A8} />
          <Cloud tx={8.5} ty={13.5} s={0.5} cls="wx-cloud-hi" />
        </>
      );
    case 'ptcldy': // small sun peeking, BIG cloud — cloud dominant
      return (
        <>
          <Sun cx={7.5} cy={8} r={2.8} cls={sunCls} angles={ATL} />
          <Cloud tx={3} ty={10} s={0.95} cls="wx-cloud" />
        </>
      );
    case 'mocldy':
      return (
        <>
          <Cloud tx={6.5} ty={5.5} s={0.6} cls="wx-cloud-dim" />
          <Cloud tx={3} ty={10.5} s={0.95} cls="wx-cloud" />
        </>
      );
    case 'cloudy':
      return <Cloud tx={3} ty={9} s={1.05} cls="wx-cloud-hi" />;
    case 'tstms':
      return (
        <>
          <Cloud tx={3} ty={5.5} s={0.95} cls="wx-cloud" />
          <path className="wx-bolt" strokeWidth={0.6} d="M12 12.5 l-2.4 4.6 h2.2 l-1.4 3.7 4-5.4 h-2.3 l1.6-2.9z" />
        </>
      );
    case 'rain':
      return (
        <>
          <Cloud tx={3} ty={5.5} s={0.95} cls="wx-cloud" />
          {drops([9, 13, 16.5], 4)}
        </>
      );
    case 'showers':
      return (
        <>
          <Cloud tx={3} ty={5.5} s={0.95} cls="wx-cloud" />
          {drops([10.5, 15], 3)}
        </>
      );
    case 'drizzle':
      return (
        <>
          <Cloud tx={3} ty={5.5} s={0.95} cls="wx-cloud" />
          <g className="wx-drop">
            {[9, 13, 16.5].map((x) => (
              <circle key={x} cx={x} cy={18} r={0.9} />
            ))}
          </g>
        </>
      );
    case 'snow':
      return (
        <>
          <Cloud tx={3} ty={5.5} s={0.95} cls="wx-cloud" />
          <g className="wx-snow">
            <path d="M9.5 16.5 v4 M7.8 17.5 h3.4 M8.1 16.8 l2.8 3.4 M10.9 16.8 l-2.8 3.4" />
            <path d="M15.5 17.5 v3 M14.2 18.3 h2.6" />
          </g>
        </>
      );
    case 'frost':
      return (
        <g className="wx-snow">
          <path d="M12 3 v18 M4.2 7.5 l15.6 9 M19.8 7.5 l-15.6 9" />
          <path
            strokeWidth={1.2}
            d="M12 3 l-1.8 2.2 M12 3 l1.8 2.2 M12 21 l-1.8 -2.2 M12 21 l1.8 -2.2 M4.2 7.5 l0.4 2.7 M4.2 7.5 l2.6 -0.7 M19.8 16.5 l-0.4 -2.7 M19.8 16.5 l-2.6 0.7 M19.8 7.5 l-2.6 -0.7 M19.8 7.5 l-0.4 2.7 M4.2 16.5 l2.6 0.7 M4.2 16.5 l0.4 -2.7"
          />
        </g>
      );
    case 'windy':
      return (
        <g className="wx-cloud-hi-stroke">
          <path d="M3 9 h10 a2.5 2.5 0 1 0-2.5-2.5" />
          <path d="M3 13 h14 a2.8 2.8 0 1 1-2.8 2.8" />
          <path d="M3 17 h7 a2 2 0 1 1-2 2" />
        </g>
      );
    case 'fog':
      return (
        <>
          <Cloud tx={4} ty={2.5} s={0.78} cls="wx-cloud" />
          <g className="wx-cloud">
            <line x1={5} y1={16} x2={19} y2={16} />
            <line x1={4} y1={19} x2={20} y2={19} />
            <line x1={7} y1={22} x2={17} y2={22} />
          </g>
        </>
      );
    case 'haze':
      return (
        <>
          <Sun cx={12} cy={9} r={3.4} cls="wx-haze-sun" angles={A8} />
          <g className="wx-cloud">
            <line x1={4} y1={17} x2={20} y2={17} />
            <line x1={6} y1={20} x2={18} y2={20} />
          </g>
        </>
      );
    case 'dust':
      return (
        <g className="wx-dust">
          <path d="M3 8 q4 -2 8 0 t8 0" />
          <path d="M3 12 q4 -2 8 0 t8 0" />
          <path d="M3 16 q4 -2 8 0 t8 0" />
          <path d="M4 20 q3.5 -1.8 7 0 t7 0" />
        </g>
      );
    case 'smoke':
      return (
        <g className="wx-smoke">
          <path d="M8.5 22 v-2.5 q0 -2 2 -3 t2 -3 q0 -1.8 -1.6 -2.8" />
          <path d="M14.5 22 v-2.5 q0 -2 2 -3 t2 -3" />
        </g>
      );
  }
}

/// Render an NWS condition code as a themed weather icon, or fall back to the
/// raw code as text (legacy heat class) when the code is unmapped.
export function WeatherGlyph({ code }: { code: string }) {
  const g = resolveGlyph(code);
  if (!g) return <span className={conditionTextClass(code)}>{code}</span>;
  return (
    <svg
      className="wx-glyph"
      viewBox="0 0 24 24"
      role="img"
      aria-label={g.label}
      fill="none"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <title>{g.label}</title>
      {shape(g)}
    </svg>
  );
}
