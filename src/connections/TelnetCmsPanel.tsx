// src/connections/TelnetCmsPanel.tsx
// Telnet-CMS connection pane — container/presentational split mirroring
// PacketConnectionPanel. Renders in the reading-pane slot when the sidebar's
// "Winlink (CMS) → Telnet" connection is selected (no separate OS window).
//
// The CMS controls were previously only reachable from SettingsPanel; this
// pane gives them a permanent home in the connection accordion. SettingsPanel
// retains its copy until that task's removal commit.

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { CmsTransport } from '../shell/useStatus';
import './TelnetCmsPanel.css';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CMS_HOST_QUICK_PICKS: { host: string; label: string }[] = [
  { host: 'cms-z.winlink.org', label: 'cms-z.winlink.org (dev)' },
  { host: 'server.winlink.org', label: 'server.winlink.org (production)' },
];

const CMS_TRANSPORT_OPTIONS: { value: CmsTransport; label: string; help: string }[] = [
  {
    value: 'CmsSsl',
    label: 'TLS · 8773',
    help: 'TLS-wrapped (port 8773). Used by Winlink Express against production server.winlink.org.',
  },
  {
    value: 'Telnet',
    label: 'Plaintext · 8772',
    help: 'Plaintext (port 8772). The dev host cms-z.winlink.org has no TLS listener — use Plaintext there.',
  },
];

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface TelnetCmsPanelProps {
  /** Current CMS host value. */
  host: string;
  /** Current transport selection. */
  transport: CmsTransport;
  /** Called when the operator commits a change (blur/Enter for host, immediate for transport). */
  onPersist: (next: { host: string; transport: CmsTransport }) => void;
}

// ---------------------------------------------------------------------------
// Presentational component
// ---------------------------------------------------------------------------

export function TelnetCmsPanel({ host: hostProp, transport: transportProp, onPersist }: TelnetCmsPanelProps) {
  // Internal draft host — seeded from prop, re-synced when prop changes.
  const [host, setHost] = useState(hostProp);
  useEffect(() => {
    setHost(hostProp);
  }, [hostProp]);

  const commitHost = () => {
    onPersist({ host: host.trim(), transport: transportProp });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const onPickHost = (picked: string) => {
    setHost(picked);
    onPersist({ host: picked, transport: transportProp });
  };

  const onTransportChange = (value: CmsTransport) => {
    onPersist({ host: host.trim(), transport: value });
  };

  return (
    <div className="reading-pane telnet-cms-panel" data-testid="telnet-cms-panel-root">
      {/* Header */}
      <div className="telnet-cms-head">
        <h2 className="telnet-cms-title">Winlink (CMS) · Telnet</h2>
      </div>
      <p className="telnet-cms-sub">
        CMS server endpoint — shown here in the connection pane; no separate window.
      </p>

      {/* Host block */}
      <div className="telnet-cms-blk" data-testid="host-block">
        <div className="telnet-cms-blk-h">
          <span>Server host</span>
        </div>
        <label className="telnet-cms-field">
          Host
          <input
            type="text"
            className="telnet-cms-input"
            data-testid="conn-host"
            value={host}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder="cms-z.winlink.org"
            onChange={(e) => setHost(e.target.value)}
            onBlur={commitHost}
            onKeyDown={onKeyDown}
          />
        </label>
        <div className="telnet-cms-quickpicks">
          {CMS_HOST_QUICK_PICKS.map((q) => (
            <button
              key={q.host}
              type="button"
              className="telnet-cms-quickpick"
              onClick={() => onPickHost(q.host)}
            >
              {q.label}
            </button>
          ))}
        </div>
        <p className="telnet-cms-hint">
          cms-z.winlink.org is the dev server (accepts unregistered clients, Plaintext only).
          server.winlink.org is production.
        </p>
      </div>

      {/* Transport block */}
      <div className="telnet-cms-blk" data-testid="transport-block">
        <div className="telnet-cms-blk-h">
          <span>Transport</span>
        </div>
        {CMS_TRANSPORT_OPTIONS.map((o) => (
          <label key={o.value} className="telnet-cms-opt">
            <input
              type="radio"
              name="cms-transport"
              value={o.value}
              checked={transportProp === o.value}
              onChange={() => onTransportChange(o.value)}
            />
            <span className="telnet-cms-opt-text">
              <span className="telnet-cms-opt-label">{o.label}</span>
              <span className="telnet-cms-opt-help">{o.help}</span>
            </span>
          </label>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Container — loads config on mount, persists changes via invoke()
// ---------------------------------------------------------------------------

/** Container: loads CMS config on mount, persists host/transport changes. */
export function TelnetCmsPanelContainer() {
  const [host, setHost] = useState('');
  const [transport, setTransport] = useState<CmsTransport>('CmsSsl');

  useEffect(() => {
    void invoke<{ host: string; transport: CmsTransport }>('config_read')
      .then((c) => {
        setHost(c.host ?? '');
        setTransport(c.transport ?? 'CmsSsl');
      })
      .catch(() => { /* pre-config — render with defaults */ });
  }, []);

  const onPersist = (next: { host: string; transport: CmsTransport }) => {
    setHost(next.host);
    setTransport(next.transport);
    void invoke('config_set_connect', { host: next.host, transport: next.transport }).catch(() => {});
  };

  return (
    <TelnetCmsPanel
      host={host}
      transport={transport}
      onPersist={onPersist}
    />
  );
}
