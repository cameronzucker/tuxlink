// CatalogReplyView — renders a received NWS catalog reply as a structured view
// (tuxlink-qyjr): a shared header plus either a Tabular State Forecast (SFT) grid
// or a Zone Forecast Product (ZFP) of period sections, with a Show-raw toggle.
// Parse-with-fallback (design §Reply rendering): the Rust parser degrades to raw
// on any deviation; this view also falls back to raw if the invoke itself fails.

import { useEffect, useState } from 'react';
import { parseReply } from './useCatalog';
import type {
  ReplyView,
  Forecast,
  ForecastDay,
  ForecastRegion,
  ForecastZone,
  ForecastCell,
} from './stationTypes';
import { WeatherGlyph } from './WeatherGlyph';
import './CatalogReplyView.css';

/// Title-case an UPPERCASE NWS period label ("REST OF TONIGHT" → "Rest of Tonight").
function titleCasePeriod(label: string): string {
  const small = new Set(['of', 'and', 'the', 'to', 'in']);
  return label
    .toLowerCase()
    .split(/\s+/)
    .map((w, i) => (i > 0 && small.has(w) ? w : w.charAt(0).toUpperCase() + w.slice(1)))
    .join(' ');
}

/// "00" → "0%"; "MM"/"" → "–".
function pop(p: string): string {
  if (!p || p === 'MM' || p === '-') return '–';
  const n = parseInt(p, 10);
  return Number.isNaN(n) ? '–' : `${n}%`;
}

function Cell({ cell }: { cell: ForecastCell }) {
  const wet = cell.popDay !== '00' && cell.popDay !== '' && cell.popDay !== 'MM';
  return (
    <td className="fcst-cell">
      <WeatherGlyph code={cell.condition} />
      <span className="temp">
        <span className="lo">{cell.low}</span>
        <span className="sep">/</span>
        <span className="hi">{cell.high}</span>
      </span>
      <span className={wet ? 'pop wet' : 'pop'}>{pop(cell.popDay)}</span>
    </td>
  );
}

function TabularView({ regions, days }: { regions: ForecastRegion[]; days: ForecastDay[] }) {
  return (
    <>
      {regions.map((r) => (
        <div key={r.name} className="region-block">
          {r.name && <div className="region">{r.name.toLowerCase()}</div>}
          <div className="fcst-scroll">
            <table className="fcst">
              <thead>
                <tr>
                  <th className="loc" />
                  {days.map((d) => (
                    <th key={`${d.dow}-${d.date}`}>
                      <span className="dow">{d.dow}</span>
                      <span className="date">{d.date}</span>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {r.locations.map((loc) => (
                  <tr key={loc.name}>
                    <td className="loc">{loc.name}</td>
                    {loc.cells.map((cell, i) => (
                      <Cell key={i} cell={cell} />
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ))}
    </>
  );
}

function ZoneView({ zones }: { zones: ForecastZone[] }) {
  return (
    <>
      {zones.map((z) => (
        <section key={z.name} className="zone">
          <div className="zone__name">{z.name}</div>
          {z.cities && <div className="zone__cities">{z.cities}</div>}
          {z.periods.map((p, i) => (
            <div key={i} className="period">
              <div className="period__label">{titleCasePeriod(p.label)}</div>
              <div className="period__text">{p.text}</div>
            </div>
          ))}
        </section>
      ))}
    </>
  );
}

function ForecastBody({ forecast }: { forecast: Forecast }) {
  if (forecast.kind === 'tabular') return <TabularView regions={forecast.regions} days={forecast.days} />;
  if (forecast.kind === 'zone') return <ZoneView zones={forecast.zones} />;
  return null; // 'none' → header + raw only
}

export function CatalogReplyView({ subject, body }: { subject: string; body: string }) {
  const [view, setView] = useState<ReplyView | null>(null);
  const [showRaw, setShowRaw] = useState(false);

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const v = await parseReply(subject, body);
        if (live) setView(v);
      } catch {
        if (live) setView({ kind: 'raw', text: body }); // never error/blank — fall back to raw
      }
    })();
    return () => {
      live = false;
    };
  }, [subject, body]);

  if (!view) return <pre className="catalog-reply__raw">{body}</pre>;
  if (view.kind === 'raw') return <pre className="catalog-reply__raw">{view.text}</pre>;

  const structured = view.forecast.kind !== 'none';

  return (
    <div className="catalog-reply" data-testid="catalog-reply">
      <div className="catalog-reply__head">
        {view.title && <div className="catalog-reply__title">{view.title}</div>}
        <div className="catalog-reply__meta">
          {view.office && <span>{view.office}</span>}
          {view.product && <code>{view.product.split(/\s+/).slice(0, 2).join(' ')}</code>}
          {view.issued && <span>Issued {view.issued}</span>}
        </div>
      </div>

      <ForecastBody forecast={view.forecast} />

      <button
        type="button"
        className="catalog-reply__toggle"
        data-testid="catalog-reply-toggle"
        onClick={() => setShowRaw((s) => !s)}
      >
        {showRaw ? 'Hide raw' : structured ? 'Show raw' : 'Show full text'}
      </button>
      {showRaw && <pre className="catalog-reply__raw">{view.raw}</pre>}
    </div>
  );
}
