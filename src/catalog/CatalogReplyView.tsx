// CatalogReplyView — renders a received NWS catalog reply as a structured view
// (tuxlink-qyjr): a shared header plus either a Tabular State Forecast (SFT) grid
// or a Zone Forecast Product (ZFP) of period sections, with a Show-raw toggle.
// Parse-with-fallback (design §Reply rendering): the Rust parser degrades to raw
// on any deviation; this view also falls back to raw if the invoke itself fails.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
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

/** A received `PUB_*` "Update Via Radio" reply self-identifies with a
 *  `WINLINK <MODE> CHANNEL LISTING` header. Cheap client-side gate for whether to
 *  offer the ingest action; the Rust command does the authoritative parse. */
function isChannelListing(text: string): boolean {
  return /WINLINK\b[\s\S]*\bCHANNEL LISTING/i.test(text);
}

/** Ingest action for a radio-delivered station-listing reply (tuxlink-xrbw):
 *  parses the listing into the offline cache so Find-a-Station shows the gateways
 *  with no internet. Operator-triggered + transparent (no silent magic). */
function IngestStationsAction({ body }: { body: string }) {
  const [state, setState] = useState<
    | { kind: 'idle' }
    | { kind: 'busy' }
    | { kind: 'done'; mode: string; count: number }
    | { kind: 'error'; message: string }
  >({ kind: 'idle' });

  async function add() {
    setState({ kind: 'busy' });
    try {
      const out = await invoke<{ mode: string; count: number }>('catalog_ingest_listing_reply', {
        body,
      });
      setState({ kind: 'done', mode: out.mode, count: out.count });
    } catch (e) {
      setState({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }

  if (state.kind === 'done') {
    return (
      <p className="catalog-reply__ingest-done" data-testid="ingest-done">
        Added {state.count} {state.mode} gateway{state.count === 1 ? '' : 's'} to Station Intelligence.
      </p>
    );
  }
  return (
    <div className="catalog-reply__ingest">
      <button
        type="button"
        className="catalog-reply__ingest-btn"
        data-testid="ingest-stations"
        disabled={state.kind === 'busy'}
        onClick={add}
      >
        {state.kind === 'busy' ? 'Adding…' : 'Add to Station Intelligence'}
      </button>
      {state.kind === 'error' && (
        <span className="catalog-reply__ingest-err" role="alert" data-testid="ingest-error">
          {state.message}
        </span>
      )}
    </div>
  );
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
  if (view.kind === 'raw') {
    // A radio-delivered station-listing reply parses as raw here; offer to ingest
    // it into Find-a-Station (tuxlink-xrbw). Ordinary raw replies are unaffected.
    return (
      <>
        {isChannelListing(view.text) && <IngestStationsAction body={view.text} />}
        <pre className="catalog-reply__raw">{view.text}</pre>
      </>
    );
  }

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
