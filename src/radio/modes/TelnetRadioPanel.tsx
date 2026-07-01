// src/radio/modes/TelnetRadioPanel.tsx
//
// Telnet CMS panel per spec §5.1, restoring the host + transport
// controls that the original reading-pane TelnetCmsPanel exposed
// (2026-05-31 operator-flagged regression on PR #176).
//
// Controls:
//   - Editable Host text input + quick-pick buttons (dev/prod CMS)
//   - Transport radio selector (TLS · 8773 / Plaintext · 8772) — port
//     is coupled to transport per Winlink protocol; both surfaces
//     stay together so the operator picks them as a single choice.
//   - Persists changes via config_set_connect (Tauri command); reads
//     initial state from config_read.
//   - Live session log via useSessionLog hook (Codex R1 fix).

import { useEffect, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button, Field } from '../../controls';
import { RadioPanel } from '../RadioPanel';
import { AuthDiagnosticBanner } from '../sections/AuthDiagnosticBanner';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { FavoritesTabs } from '../../favorites/FavoritesTabs';
import { useFavorites } from '../../favorites/useFavorites';
import { tsLocal } from '../../favorites/ts-local';
import type { FavoriteDial } from '../../favorites/types';

export interface TelnetRadioPanelProps {
  onClose: () => void;
}

type CmsTransport = 'CmsSsl' | 'Telnet';

interface TelnetConfigSlice {
  host: string;
  transport: CmsTransport;
}

const DEFAULT_HOST = 'cms.winlink.org';
const DEFAULT_TRANSPORT: CmsTransport = 'CmsSsl';

const QUICK_PICKS: { host: string; label: string }[] = [
  { host: 'cms-z.winlink.org', label: 'cms-z (dev)' },
  { host: 'server.winlink.org', label: 'server (prod)' },
];

const TRANSPORT_OPTIONS: { value: CmsTransport; label: string; port: number; help: string }[] = [
  {
    value: 'CmsSsl',
    label: 'TLS',
    port: 8773,
    help: 'TLS-wrapped (port 8773). Default; production server.winlink.org.',
  },
  {
    value: 'Telnet',
    label: 'Plaintext',
    port: 8772,
    help: 'Plaintext (port 8772). cms-z.winlink.org (dev) has no TLS listener.',
  },
];

function portFor(transport: CmsTransport): number {
  return TRANSPORT_OPTIONS.find((o) => o.value === transport)?.port ?? 8773;
}

export function TelnetRadioPanel({ onClose }: TelnetRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [host, setHost] = useState<string>(DEFAULT_HOST);
  const [transport, setTransport] = useState<CmsTransport>(DEFAULT_TRANSPORT);
  const { entries: logEntries, clear: clearLog } = useSessionLog();

  // Favorites integration (Task B6-TELNET). RADIO-1: a favorite's Connect is
  // PRE-FILL ONLY — it sets host + transport via `handlePrefill` and NEVER
  // invokes `cms_connect`. The operator's later Start click is the Part 97
  // consent gate. recordAttempt logs the HONEST on-air outcome: cms_connect is
  // a BLOCKING connect→B2F, so `reached` is recorded on resolve and `failed` in
  // the catch (no status-transition watching needed — the single call's
  // resolve/reject IS the signal).
  const { recordAttempt } = useFavorites('telnet');
  // The favorite whose Connect was last clicked. Carries its metadata into the
  // connection record IFF its gateway matches the current host. Cleared on any
  // manual host/transport edit (a hand-set target is not the prefilled favorite).
  const pendingDialRef = useRef<FavoriteDial | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<TelnetConfigSlice>('config_read')
      .then((c) => {
        if (cancelled) return;
        if (c.host) setHost(c.host);
        if (c.transport) setTransport(c.transport);
      })
      .catch(() => {
        // Pre-wizard / config absent — keep defaults.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = (next: TelnetConfigSlice) => {
    void invoke('config_set_connect', { host: next.host, transport: next.transport }).catch(() => {
      // Persist errors surface in the session log via the backend.
    });
  };

  // RADIO-1 + H7: a favorite's Connect pre-fills host + transport ONLY. It
  // persists them via config_set_connect (config PERSISTENCE, not a
  // connect/transmit) so the operator's subsequent Start dials the prefilled
  // server. It does NOT invoke cms_connect.
  const handlePrefill = (dial: FavoriteDial) => {
    const nextHost = dial.gateway;
    const nextTransport = (dial.transport ?? DEFAULT_TRANSPORT) as CmsTransport;
    setHost(nextHost);
    setTransport(nextTransport);
    persist({ host: nextHost, transport: nextTransport });
    pendingDialRef.current = dial;
  };

  // Build the dial for a connection record (H7: telnet keys on transport — no
  // freq/band). The gateway is the current host. If the prefilled favorite
  // matches it (case-insensitive — hostnames), carry its metadata into the
  // record; otherwise record a minimal manual dial keyed on the current host +
  // transport.
  const buildRecordDial = (): FavoriteDial => {
    const gw = host.trim();
    const pend = pendingDialRef.current;
    if (pend && pend.gateway.trim().toLowerCase() === gw.toLowerCase()) {
      return { ...pend, mode: 'telnet', gateway: gw, transport };
    }
    return { mode: 'telnet', gateway: gw, transport };
  };

  const commitHost = () => {
    const trimmed = host.trim();
    if (trimmed && trimmed !== host) setHost(trimmed);
    persist({ host: trimmed || DEFAULT_HOST, transport });
  };

  const onHostKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const pickHost = (picked: string) => {
    setHost(picked);
    // A manual quick-pick hand-overrides the prefilled favorite — drop the
    // association so the record doesn't carry stale metadata.
    pendingDialRef.current = null;
    persist({ host: picked, transport });
  };

  const pickTransport = (next: CmsTransport) => {
    setTransport(next);
    // Manual transport override — drop the prefilled-favorite association.
    pendingDialRef.current = null;
    persist({ host: host.trim() || DEFAULT_HOST, transport: next });
  };

  const start = async () => {
    if (busy) return;
    setBusy(true);
    // Build the dial BEFORE the await so the host/transport at click time are
    // captured (a prefill or edit mid-connect can't shift the recorded dial).
    const dial = buildRecordDial();
    try {
      await invoke('cms_connect');
      // Blocking connect→B2F resolved = honest reach.
      void recordAttempt(dial, 'reached', tsLocal());
    } catch {
      // Errors surface in the session log; record the failed attempt in the
      // CATCH (never the finally) so a pre-air guard never logs a false fail.
      void recordAttempt(dial, 'failed', tsLocal());
    } finally {
      setBusy(false);
    }
  };

  const stop = () => {
    void invoke('cms_abort').catch(() => {});
  };

  return (
    <RadioPanel
      mode={{ kind: 'telnet', intent: 'cms' }}
      state={busy ? 'connecting' : 'disconnected'}
      sub={`${host}:${portFor(transport)}`}
      onClose={onClose}
    >
      {/* Connect-target surface (Task B6-TELNET; tuxlink-fr0d: Manual-only).
          Telnet connects to a FIXED CMS host, so FavoritesTabs renders the
          manual content directly — no Favorites/Recent tabs (there is no nearby
          station to choose). The Server (Host + quick-picks) + Transport
          sections are that manual content. onPrefill is retained for the shared
          FavoritesTabs contract but is never invoked in Manual-only mode. */}
      <FavoritesTabs
        mode="telnet"
        onPrefill={handlePrefill}
        manualContent={
          <>
            <section className="radio-panel-sec">
              <h5>Server</h5>
              <label className="radio-panel-input-row">
                <span>Host</span>
                <Field
                  type="text"
                  className="radio-panel-input"
                  data-testid="telnet-host-input"
                  value={host}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder="cms.winlink.org"
                  onChange={(e) => {
                    setHost(e.target.value);
                    // A hand-typed host is not the prefilled favorite — drop the
                    // association so the record doesn't carry stale metadata.
                    pendingDialRef.current = null;
                  }}
                  onBlur={commitHost}
                  onKeyDown={onHostKey}
                />
              </label>
              <div className="radio-panel-chip-row">
                {QUICK_PICKS.map((q) => (
                  <button
                    key={q.host}
                    type="button"
                    className="radio-panel-chip"
                    data-testid={`telnet-pick-${q.host}`}
                    onClick={() => pickHost(q.host)}
                  >
                    {q.label}
                  </button>
                ))}
              </div>
            </section>

            <section className="radio-panel-sec">
              <h5>Transport</h5>
              {TRANSPORT_OPTIONS.map((o) => (
                <label key={o.value} className="radio-panel-radio-row">
                  <input
                    type="radio"
                    name="telnet-transport"
                    value={o.value}
                    checked={transport === o.value}
                    onChange={() => pickTransport(o.value)}
                    data-testid={`telnet-transport-${o.value}`}
                  />
                  <span className="radio-panel-radio-text">
                    <span className="radio-panel-radio-label">
                      {o.label} · port {o.port}
                    </span>
                    <span className="radio-panel-radio-help">{o.help}</span>
                  </span>
                </label>
              ))}
            </section>
          </>
        }
      />

      <AuthDiagnosticBanner />

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      <section className="radio-panel-sec radio-panel-act">
        <Button
          tone="primary" emphasis="soft" size="md"
          disabled={busy}
          onClick={start}
        >
          {busy ? 'Connecting…' : 'Start'}
        </Button>
        <Button
          tone="danger" emphasis="soft" size="md"
          onClick={stop}
        >
          Stop
        </Button>
      </section>
    </RadioPanel>
  );
}
