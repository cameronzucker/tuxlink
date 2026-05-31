// src/radio/modes/TelnetRadioPanel.tsx
//
// Telnet CMS panel per spec §5.1. Smallest content surface: no modem
// to configure (the CMS endpoint comes from config). Sections:
// Connection (live endpoint + transport from config_read), Session
// (current state), Session log (live tail via useSessionLog), Actions.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';

export interface TelnetRadioPanelProps {
  onClose: () => void;
}

// Minimal shape of config_read's response that this panel consumes.
// The full ConfigViewDto lives in src/shell/useStatus.ts; redeclaring
// the narrow slice here keeps the panel decoupled from the status-bar
// data layer. CmsTransport is 'CmsSsl' | 'Telnet' per Rust serde
// (rename_all = "PascalCase").
interface TelnetConfigSlice {
  host: string;
  transport: 'CmsSsl' | 'Telnet';
}

const DEFAULT_HOST = 'cms.winlink.org';
const DEFAULT_TRANSPORT: TelnetConfigSlice['transport'] = 'CmsSsl';

function formatTransport(t: TelnetConfigSlice['transport']): string {
  return t === 'CmsSsl' ? 'CMS-SSL (TLS)' : 'Telnet (cleartext)';
}

function formatEndpoint(host: string, transport: TelnetConfigSlice['transport']): string {
  // Conventional Winlink ports: 8773 for CMS-SSL, 8772 for cleartext.
  const port = transport === 'CmsSsl' ? 8773 : 8772;
  return `${host}:${port}`;
}

export function TelnetRadioPanel({ onClose }: TelnetRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [config, setConfig] = useState<TelnetConfigSlice | null>(null);
  const logEntries = useSessionLog();

  useEffect(() => {
    let cancelled = false;
    invoke<TelnetConfigSlice>('config_read')
      .then((c) => {
        if (!cancelled) setConfig(c);
      })
      .catch(() => {
        // Pre-wizard / config absent — fall back to defaults below.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const host = config?.host ?? DEFAULT_HOST;
  const transport = config?.transport ?? DEFAULT_TRANSPORT;

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
      sub={host}
      onClose={onClose}
    >
      <section className="radio-panel-sec">
        <h5>Connection</h5>
        <div className="radio-panel-field">
          <span>Endpoint</span>
          <span className="radio-panel-readonly">{formatEndpoint(host, transport)}</span>
        </div>
        <div className="radio-panel-field">
          <span>Transport</span>
          <span className="radio-panel-readonly">{formatTransport(transport)}</span>
        </div>
      </section>

      <section className="radio-panel-sec">
        <h5>Session</h5>
        <div className="radio-panel-mono">
          {busy ? 'Connecting…' : 'Idle — Start to begin a session.'}
        </div>
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
