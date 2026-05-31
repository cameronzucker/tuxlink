// src/radio/modes/TelnetRadioPanel.tsx
//
// Telnet CMS panel per spec §5.1. Smallest content surface: no modem
// to configure (the CMS endpoint comes from config). Sections rendered:
// Connection (endpoint + transport), Session (last result), Session log,
// Actions.

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection, type SessionLogEntry } from '../sections/SessionLogSection';

export interface TelnetRadioPanelProps {
  onClose: () => void;
  /** Optional initial log entries (tests inject; live wiring lands in
   *  P2 if we surface a useTelnetSessionLog hook, otherwise empty []). */
  initialLogEntries?: SessionLogEntry[];
}

export function TelnetRadioPanel({
  onClose,
  initialLogEntries = [],
}: TelnetRadioPanelProps) {
  const [busy, setBusy] = useState(false);

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
      sub="cms.winlink.org"
      onClose={onClose}
    >
      <section className="radio-panel-sec">
        <h5>Connection</h5>
        <div className="radio-panel-field">
          <span>Endpoint</span>
          <span className="radio-panel-readonly">cms.winlink.org:8773</span>
        </div>
        <div className="radio-panel-field">
          <span>Transport</span>
          <span className="radio-panel-readonly">CMS-SSL (TLS)</span>
        </div>
      </section>

      <section className="radio-panel-sec">
        <h5>Session</h5>
        <div className="radio-panel-mono">
          {/* Last-result + state line; wiring TBD in implementation. */}
          {busy ? 'Connecting…' : 'Idle — Start to begin a session.'}
        </div>
      </section>

      <SessionLogSection entries={initialLogEntries} />

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
