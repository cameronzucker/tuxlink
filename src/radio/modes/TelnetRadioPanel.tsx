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

import { useEffect, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';

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
  const logEntries = useSessionLog();

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
    persist({ host: picked, transport });
  };

  const pickTransport = (next: CmsTransport) => {
    setTransport(next);
    persist({ host: host.trim() || DEFAULT_HOST, transport: next });
  };

  const start = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await invoke('cms_connect');
    } catch {
      // Errors surface in the session log; nothing inline.
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
      <section className="radio-panel-sec">
        <h5>Server</h5>
        <label className="radio-panel-input-row">
          <span>Host</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="telnet-host-input"
            value={host}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="cms.winlink.org"
            onChange={(e) => setHost(e.target.value)}
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

      <SessionLogSection entries={logEntries} />

      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          disabled={busy}
          onClick={start}
        >
          {busy ? 'Connecting…' : 'Start'}
        </button>
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-bad"
          onClick={stop}
        >
          Stop
        </button>
      </section>
    </RadioPanel>
  );
}
