// CatalogReplyView — renders a received catalog reply as a structured view (area-weather)
// or raw, with a toggle. Parse-with-fallback (design §Reply rendering): the Rust parser
// degrades to raw on any deviation; this view also falls back to raw if the invoke itself fails.

import { useEffect, useState } from 'react';
import { parseReply } from './useCatalog';
import type { ReplyView } from './stationTypes';
import './CatalogReplyView.css';

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

  return (
    <div className="catalog-reply">
      <dl className="catalog-reply__structured">
        <dt>Office</dt>
        <dd>{view.office}</dd>
        <dt>Product</dt>
        <dd>{view.product}</dd>
        {view.issued && (
          <>
            <dt>Issued</dt>
            <dd>{view.issued}</dd>
          </>
        )}
      </dl>
      <button type="button" className="catalog-reply__toggle" onClick={() => setShowRaw((s) => !s)}>
        {showRaw ? 'Hide raw' : 'Show raw'}
      </button>
      {showRaw && <pre className="catalog-reply__raw">{view.raw}</pre>}
    </div>
  );
}
