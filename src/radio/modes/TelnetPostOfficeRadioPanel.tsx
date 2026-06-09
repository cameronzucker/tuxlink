// src/radio/modes/TelnetPostOfficeRadioPanel.tsx
//
// Post Office radio panel (tuxlink-6c9y, Task B3) — the shared pane for the
// two Post Office session types, parameterized by `mode`:
//
//   - 'local'   → Telnet RMS Post Office. Logs in as <base>-L; mail is held
//                 in the relay's LOCAL pool (never forwarded globally).
//                 host:port only (default 127.0.0.1:8772). No favorites.
//   - 'network' → Network Post Office. Logs in as the FULL callsign; mail
//                 takes normal Winlink routing via the relay. host:port PLUS
//                 a saved-relay favorites list ({callsign,label,host,port}).
//
// Both modes are pure TCP/IP and OUTSIDE RADIO-1: neither keys a transmitter,
// so there is NO consent modal (design §7.5 + the no-consent test). The send
// path is *connection-determined* — the operator selects which Outbox drafts
// to send in this session via a checklist; routing is not a message attribute
// (design §3, the headline divergence from WLE's compose-time pools).
//
// Structurally mirrors TelnetP2pRadioPanel.tsx: same RadioPanel chrome, same
// radio-panel-sec / radio-panel-input-row / radio-panel-chip class system,
// same SessionLogSection + useSessionLog, same config_read identity fetch, and
// the same `{ req: {...} }` invoke wrapper (Tauri rejects flat args).
//
// Tauri commands used:
//   config_read()                                   → { callsign, grid }
//   mailbox_list({ folder: 'outbox' })  (via useMailbox) → MessageMeta[]
//   telnet_post_office_connect({ req: PostOfficeConnectReq }) → DialResult
//     ^ Phase-C backend command (B3↔C1 seam) — NOT yet implemented; the panel
//       wires the contract; tests mock it.
//   network_po_favorites_get()                      → RelayFavorite[]   (network)
//   network_po_favorites_add({ favorite })          → RelayFavorite[]
//   network_po_favorites_remove({ host, port })     → RelayFavorite[]

import { useEffect, useMemo, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useQueryClient } from '@tanstack/react-query';
import { RadioPanel } from '../RadioPanel';
import { SessionLogSection } from '../sections/SessionLogSection';
import { useSessionLog } from '../sections/useSessionLog';
import { useMailbox } from '../../mailbox/useMailbox';
import '../sections/ListenSection.css';

export type PostOfficeMode = 'local' | 'network';

export interface TelnetPostOfficeRadioPanelProps {
  /** 'local' = Telnet RMS Post Office (CALL-L); 'network' = Network Post Office. */
  mode: PostOfficeMode;
  onClose: () => void;
}

const DEFAULT_HOST = '127.0.0.1';
const DEFAULT_PORT = 8772; // RMS Relay default (design §4.3); operator-overridable.
const MIN_PORT = 1;
const MAX_PORT = 65535;

/** Kebab-case relay-state values serialized by `RelayStateDto` (Rust `#[serde(rename_all = "kebab-case")]`). */
type RelayState =
  | 'not-relay'
  | 'local-database'
  | 'radio-network'
  | 'radio-network-and-internet'
  | 'no-cms-connection-available';

interface DialResult {
  sent_count: number;
  received_count: number;
  relay_state: RelayState;
}

/** Map a relay-state kebab value to a human-readable label for the §5.9 banner strip. */
const RELAY_STATE_LABELS: Record<Exclude<RelayState, 'not-relay'>, string> = {
  'local-database': 'Local post office (holds mail locally)',
  'radio-network': 'Radio network hub',
  'radio-network-and-internet': 'Radio network + internet relay',
  'no-cms-connection-available': 'Relay reachable; CMS uplink down',
};

interface ConfigSlice {
  callsign?: string;
  grid?: string;
}

/// A saved Network PO relay favorite — mirrors the Rust `config::RelayFavorite`
/// (`{ callsign, label, host, port }`); the `(host case-insensitive, port)`
/// pair is the uniqueness key.
interface RelayFavorite {
  callsign: string;
  label: string;
  host: string;
  port: number;
}

/// Stable per-favorite key for React lists + test ids: `host:port`.
const favKey = (f: { host: string; port: number }) => `${f.host}:${f.port}`;

/**
 * Compute the login the relay will receive.
 *
 * Local mode mirrors the backend's `base_callsign_for_post_office(raw, true)`
 * (telnet.rs:455): uppercase → split on '.' take [0] → split on '-' take [0] →
 * base, then append '-L' UNCONDITIONALLY (the local-vs-global routing
 * discriminator). The backend's final step is `format!("{base}-L")` with NO
 * empty-base guard, so an empty/whitespace callsign yields the literal '-L'.
 * The indicator must show exactly what the backend would send and never
 * silently disagree — the prior `base ? \`${base}-L\` : ''` guard rendered '—'
 * for empty input while the backend would have sent '-L'.
 *
 * Network mode logs in with the full BASE callsign (SSID/qualifier stripped,
 * no '-L'), matching base_callsign_for_post_office(.., local=false) — A1's
 * vector table returns "N7CPZ" for "n7cpz-10". The design doc's "full callsign"
 * means the full BASE callsign (vs local's base + '-L'), NOT the raw SSID-bearing
 * form; the A1 vector is the precise spec and the indicator must predict it.
 * Empty → '' (network) / '-L' (local), mirroring the backend's unguarded format.
 *
 * Exported for unit reuse / parity with the backend vector table.
 */
export function loginCallsign(myCallsign: string, mode: PostOfficeMode): string {
  const base = myCallsign.trim().toUpperCase().split('.')[0].split('-')[0];
  return mode === 'network' ? base : `${base}-L`;
}

export function TelnetPostOfficeRadioPanel({
  mode,
  onClose,
}: TelnetPostOfficeRadioPanelProps) {
  const [busy, setBusy] = useState(false);
  const [host, setHost] = useState<string>(DEFAULT_HOST);
  const [port, setPort] = useState<number>(DEFAULT_PORT);
  const [myCallsign, setMyCallsign] = useState<string>('');
  const [locator, setLocator] = useState<string>('');
  // Selected Outbox MIDs to send this session. Keyed on `message.id`, so
  // partial-send survival is automatic: sent rows drop from the Outbox after
  // invalidateQueries, and an unsent-but-still-checked id stays selected
  // because the Outbox row still carries it (design §4.7).
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [result, setResult] = useState<DialResult | null>(null);
  const [connectError, setConnectError] = useState<string | null>(null);

  // Network PO favorites (network mode only). Relay-favorite add inputs live
  // alongside the host/port inputs (the favorite's endpoint is the current
  // host:port; callsign + label are the relay's metadata).
  const [favorites, setFavorites] = useState<RelayFavorite[]>([]);
  const [favCallsign, setFavCallsign] = useState<string>('');
  const [favLabel, setFavLabel] = useState<string>('');
  // Favorites add/remove error. The favorites Tauri commands are pure config
  // writes that emit NO session_log:line events, so a rejection (e.g. a
  // duplicate host:port → UiError::Rejected) would be invisible if swallowed.
  // Surface it inline beside the favorites controls instead.
  const [favoritesError, setFavoritesError] = useState<string | null>(null);

  const { entries: logEntries, clear: clearLog } = useSessionLog();
  const queryClient = useQueryClient();

  // Outbox source — the checklist content. The hook handles the 10s refetch +
  // post-connect invalidation (design §4.2/§4.7).
  const { messages: outbox } = useMailbox('outbox');

  // Load my_callsign + locator from config on mount (same pattern as the P2P
  // panel — one call, cancelled on unmount).
  useEffect(() => {
    let cancelled = false;
    invoke<ConfigSlice>('config_read')
      .then((c) => {
        if (cancelled) return;
        if (c.callsign) setMyCallsign(c.callsign);
        if (c.grid) setLocator(c.grid);
      })
      .catch(() => {
        // Pre-wizard / config absent — identity stays empty; the backend will
        // reject with a meaningful error if needed.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Load favorites on mount (network mode only).
  useEffect(() => {
    if (mode !== 'network') return;
    let cancelled = false;
    invoke<RelayFavorite[]>('network_po_favorites_get')
      .then((list) => {
        if (!cancelled && list) setFavorites(list);
      })
      .catch(() => {
        // Backend absent / config error — keep the empty list.
      });
    return () => {
      cancelled = true;
    };
  }, [mode]);

  const selectedMids = useMemo(
    // Preserve Outbox order rather than Set insertion order so the sent list
    // is legible + deterministic.
    () => outbox.filter((m) => selected.has(m.id)).map((m) => m.id),
    [outbox, selected],
  );
  const selectedCount = selectedMids.length;

  const toggleRow = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => setSelected(new Set(outbox.map((m) => m.id)));
  const selectNone = () => setSelected(new Set());

  const commitHost = () => {
    const trimmed = host.trim();
    if (trimmed && trimmed !== host) setHost(trimmed);
  };

  const onHostKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
  };

  const pickFavorite = (f: RelayFavorite) => {
    setHost(f.host);
    setPort(f.port);
    setFavCallsign(f.callsign);
    setFavLabel(f.label);
  };

  const addFavorite = async () => {
    const favorite: RelayFavorite = {
      callsign: favCallsign.trim().toUpperCase(),
      label: favLabel.trim(),
      host: host.trim(),
      port,
    };
    try {
      const updated = await invoke<RelayFavorite[]>('network_po_favorites_add', {
        favorite,
      });
      if (updated) setFavorites(updated);
      setFavoritesError(null);
    } catch (e) {
      // Duplicate host:port / empty-field rejections (UiError::Rejected) emit no
      // session-log events — surface them in the inline favorites error line.
      setFavoritesError(String(e));
    }
  };

  const removeFavorite = async (f: RelayFavorite) => {
    try {
      const updated = await invoke<RelayFavorite[]>('network_po_favorites_remove', {
        host: f.host,
        port: f.port,
      });
      if (updated) setFavorites(updated);
      setFavoritesError(null);
    } catch (e) {
      // No session-log event for favorites mutations — surface inline.
      setFavoritesError(String(e));
    }
  };

  // Connect — mirrors the P2P panel's `start()`. telnet_post_office_connect
  // drives session-log events + the inbound-selection prompt (via the bsiy
  // decide-seam) backend-side. The `{ req }` wrapper is REQUIRED (Tauri
  // rejects flat args — see the P2P panel comment).
  const start = async () => {
    if (busy) return;
    setBusy(true);
    setResult(null);
    setConnectError(null);
    try {
      const res = await invoke<DialResult>('telnet_post_office_connect', {
        req: {
          mode,
          host: host.trim() || DEFAULT_HOST,
          port,
          my_callsign: myCallsign,
          locator,
          selected_mids: selectedMids,
        },
      });
      setResult(res);
      // Sent messages moved Outbox→Sent and received messages landed in Inbox.
      // Refresh both views so the operator sees them without the 10s wait.
      // Selection survives because the Set is keyed on `m.id`: sent rows vanish
      // from the refreshed Outbox; unsent-but-checked rows stay (design §4.7).
      await queryClient.invalidateQueries({ queryKey: ['mailbox'] });
    } catch (e) {
      setConnectError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = () => {
    void invoke('telnet_post_office_abort').catch(() => {});
  };

  const intent: 'post-office' | 'network-po' =
    mode === 'local' ? 'post-office' : 'network-po';

  const login = loginCallsign(myCallsign, mode);

  const connectLabel = busy
    ? 'Connecting…'
    : selectedCount > 0
      ? `Connect & send ${selectedCount}`
      : 'Connect';

  const subText = `${host.trim() || DEFAULT_HOST}:${port}`;

  const bannerText =
    mode === 'local'
      ? 'Exchanges local mail — held at the relay for local pickup, not forwarded onto the global Winlink system.'
      : 'Exchanges normal Winlink mail over a LAN / mesh relay. The relay forwards onward and can deliver to local mesh recipients.';

  return (
    <RadioPanel
      mode={{ kind: 'telnet', intent }}
      state={busy ? 'connecting' : 'disconnected'}
      sub={subText}
      onClose={onClose}
    >
      {/* Routing banner — states what happens to the mail this session. */}
      <section className="radio-panel-sec">
        <h5>{mode === 'local' ? 'Telnet RMS Post Office' : 'Network Post Office'}</h5>
        <p className="radio-panel-radio-help" data-testid="po-banner">
          {bannerText}
        </p>
      </section>

      {/* Relay endpoint — host:port. Defaults to 127.0.0.1:8772 (design §4.3). */}
      <section className="radio-panel-sec">
        <h5>Relay</h5>
        <label className="radio-panel-input-row">
          <span>Host</span>
          <input
            type="text"
            className="radio-panel-input"
            data-testid="po-host-input"
            value={host}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            placeholder={DEFAULT_HOST}
            onChange={(e) => setHost(e.target.value)}
            onBlur={commitHost}
            onKeyDown={onHostKey}
          />
        </label>
        <label className="radio-panel-input-row">
          <span>Port</span>
          <input
            type="number"
            className="radio-panel-input"
            data-testid="po-port-input"
            value={port}
            min={MIN_PORT}
            max={MAX_PORT}
            onChange={(e) => {
              const n = parseInt(e.target.value, 10);
              if (!Number.isNaN(n) && n >= MIN_PORT && n <= MAX_PORT) setPort(n);
            }}
            onKeyDown={onHostKey}
          />
        </label>
        {/* Read-only login indicator. No password field — the handshake
            password is the non-secret constant CMSTelnet (design §4.3). */}
        <div className="radio-panel-input-row">
          <span>Logs in as</span>
          <span className="radio-panel-readonly" data-testid="po-login-indicator">
            {login || '—'}
          </span>
        </div>
      </section>

      {/* Favorites — Network PO only (design §4.4). */}
      {mode === 'network' && (
        <section className="radio-panel-sec" data-testid="po-favorites-section">
          <h5>Saved relays</h5>
          <div className="radio-panel-chip-row" data-testid="po-favorites-row">
            {favorites.map((f) => (
              <span
                key={favKey(f)}
                className="radio-panel-chip"
                data-testid={`po-favorite-${favKey(f)}`}
                role="button"
                tabIndex={0}
                onClick={() => pickFavorite(f)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    pickFavorite(f);
                  }
                }}
              >
                {f.label ? `${f.label} (${f.callsign})` : f.callsign} · {favKey(f)}
                <button
                  type="button"
                  className="radio-panel-chip-x"
                  data-testid={`po-favorite-remove-${favKey(f)}`}
                  aria-label={`Remove relay ${favKey(f)}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    void removeFavorite(f);
                  }}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
          {/* Add-favorite row: callsign + label, using the current host:port
              as the endpoint. */}
          <label className="radio-panel-input-row">
            <span>Callsign</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="po-favorite-callsign-input"
              value={favCallsign}
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
              placeholder="W7RELAY"
              onChange={(e) => setFavCallsign(e.target.value.toUpperCase())}
            />
          </label>
          <label className="radio-panel-input-row">
            <span>Label</span>
            <input
              type="text"
              className="radio-panel-input"
              data-testid="po-favorite-label-input"
              value={favLabel}
              placeholder="Home mesh relay"
              onChange={(e) => setFavLabel(e.target.value)}
            />
          </label>
          <div className="radio-panel-chip-row">
            <button
              type="button"
              className="radio-panel-chip radio-panel-chip-add"
              data-testid="po-favorite-add-btn"
              onClick={() => void addFavorite()}
            >
              + Save this relay
            </button>
          </div>
          {favoritesError && (
            <p
              className="radio-panel-radio-help"
              data-testid="po-favorites-error"
              style={{ color: 'var(--error, #f87171)' }}
            >
              {favoritesError}
            </p>
          )}
          <p className="radio-panel-radio-help">
            AREDN auto-discovery is intentionally omitted — enter the relay
            host:port manually.
          </p>
        </section>
      )}

      {/* Outbox send-selection checklist (design §4.3 + §4.6). */}
      <section className="radio-panel-sec" data-testid="po-outbox-section">
        <h5>Send from Outbox</h5>
        <div className="radio-panel-chip-row">
          <button
            type="button"
            className="radio-panel-chip"
            data-testid="po-select-all"
            disabled={outbox.length === 0}
            onClick={selectAll}
          >
            Select all
          </button>
          <button
            type="button"
            className="radio-panel-chip"
            data-testid="po-select-none"
            disabled={selectedCount === 0}
            onClick={selectNone}
          >
            Select none
          </button>
        </div>
        {outbox.length === 0 ? (
          <p className="radio-panel-radio-help" data-testid="po-outbox-empty">
            Outbox is empty — Connect to receive only.
          </p>
        ) : (
          <ul className="po-outbox-list" data-testid="po-outbox-list">
            {outbox.map((m) => (
              <li key={m.id}>
                <label
                  className="radio-panel-input-row"
                  data-testid={`po-outbox-row-${m.id}`}
                >
                  <input
                    type="checkbox"
                    data-testid={`po-outbox-check-${m.id}`}
                    checked={selected.has(m.id)}
                    onChange={() => toggleRow(m.id)}
                  />
                  <span>
                    {m.subject || '(no subject)'} →{' '}
                    {m.to.join(', ') || '(no recipient)'} · {m.bodySize} B
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}
      </section>

      <SessionLogSection entries={logEntries} onClear={clearLog} />

      {/* Result / error feedback — below the session log, inline (no modal). */}
      {result && (
        <section className="radio-panel-sec">
          <p className="radio-panel-radio-help" data-testid="po-result">
            Sent {result.sent_count}, received {result.received_count}.
          </p>
          {/* §5.9 relay-state banner: shown only when the relay identified itself
              as a relay (any state OTHER than 'not-relay'). Not shown for plain
              CMS endpoints. Informational — no action required. */}
          {result.relay_state !== 'not-relay' && (
            <p
              className="radio-panel-radio-help"
              data-testid="po-relay-banner"
              style={{ marginTop: '4px' }}
            >
              Relay: {RELAY_STATE_LABELS[result.relay_state]}
            </p>
          )}
        </section>
      )}
      {connectError && (
        <section className="radio-panel-sec">
          <p
            className="radio-panel-radio-help"
            data-testid="po-error"
            style={{ color: 'var(--error, #f87171)' }}
          >
            {connectError}
          </p>
        </section>
      )}

      {/* Actions. Connect stays ENABLED at N=0 (receive-only is a primary use
          — design §4.3). Label reflects the selection count. NO consent modal:
          Post Office is pure TCP, outside RADIO-1. */}
      <section className="radio-panel-sec radio-panel-act">
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-primary"
          data-testid="po-connect-btn"
          disabled={busy}
          onClick={start}
        >
          {connectLabel}
        </button>
        <button
          type="button"
          className="radio-panel-btn radio-panel-btn-bad"
          data-testid="po-stop-btn"
          onClick={stop}
        >
          Stop
        </button>
      </section>
    </RadioPanel>
  );
}
