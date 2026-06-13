// src/aprs/AprsChatPanel.tsx
//
// APRS tactical-chat inline surface — the OPEN CHANNEL model. The visible
// product, in ONE narrow column that fits the ~400px radio dock:
//
//   header   — title + listening state + a quiet open-channel honesty cue
//   feed     — one flat, time-ordered list of every message heard on the
//              channel plus our own sends (`from → to`, or `→ all` for a
//              broadcast), with honest delivery states on our directed sends
//   composer — a COMPACT recipient control (type a callsign OR pick from the
//              heard-stations dropdown; empty ⇒ broadcast), a message input,
//              Send, and a compact editable digipeater Path field
//
// APRS is a party line, not a private chat: there is NO per-callsign thread
// list, NO conversations roster, NO side column. The heard-stations list is a
// dropdown on the recipient field, not a visible column.
//
// Inline only — NO pop-up windows (hard project rule). The surface is a single
// narrow column constrained to the dock width; it does not require or add side
// columns.
//
// RF-honesty: delivery chips reflect ONLY the backend-reported `DeliveryState`
// (sent / acked / timedOut / rejected) — no fabricated "delivered". Broadcasts
// are fire-and-forget: they show "broadcast · sent" and NEVER a delivery
// checkmark. `send` delegates to `useAprsChat().send`, which appends the
// outgoing message ONLY on a successful backend ack of queueing; a rejected
// send is caught here and surfaced as an inline notice with NO message.
//
// Start/Stop listening: the toggle arms/disarms the backend listener via
// aprs_listen_start / aprs_listen_stop and reflects the hook's `listening`
// state. A failed start is caught and surfaced through the same inline error
// notice the composer uses — no fabricated "listening" state.

import { useEffect, useId, useRef, useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  AprsConfigDto,
  ChannelMessage,
  DeliveryState,
  HeardStation,
} from './aprsTypes';
import './AprsChatPanel.css';

/// APRS message text budget — the per-message character cap that makes bounded
/// airtime real (matches the backend codec's ≤67 text limit).
const APRS_TEXT_MAX = 67;

/// Format a local epoch-ms timestamp as a short 24-hour HH:MM clock time.
/// 24-hour (`hour12: false`) matches ham-radio convention and the rest of the
/// tuxlink UI (the status-bar clock), and keeps the output locale-deterministic
/// (no AM/PM suffix — otherwise CI's en-US locale renders "02:08 PM"). Exported
/// for unit testing.
export function formatTime(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

/// Map a delivery state to its operator-facing chip label + variant class.
/// Honest states only — there is no synthetic "delivered".
const CHIP: Record<DeliveryState, { label: string; variant: string }> = {
  sent: { label: 'Sent', variant: 'neutral' },
  acked: { label: 'Acked', variant: 'success' },
  timedOut: { label: 'Timed out', variant: 'warning' },
  rejected: { label: 'Rejected', variant: 'error' },
};

function DeliveryChip({ state, msg }: { state: DeliveryState; msg: ChannelMessage }) {
  const chip = CHIP[state];
  const label =
    state === 'acked' && msg.ackedAt != null
      ? `${chip.label} ${formatTime(msg.ackedAt)}`
      : chip.label;
  return (
    <span
      className={`aprs-chip aprs-chip-${chip.variant}`}
      data-testid="aprs-delivery-chip"
      data-state={state}
    >
      {label}
    </span>
  );
}

/// One row in the channel feed. The address line reads `FROM → TO` (or
/// `→ all` for a broadcast). Outbound rows are subtly distinguished (a left
/// accent rule) — intentionally LIGHT, not heavy chat bubbles, because this is
/// a shared channel log, not a private conversation.
function FeedRow({ msg }: { msg: ChannelMessage }) {
  const broadcast = msg.to === null;
  return (
    <li
      className={`aprs-msg aprs-msg-${msg.direction}`}
      data-testid="aprs-msg"
      data-direction={msg.direction}
      data-broadcast={broadcast}
    >
      <div className="aprs-msg-head">
        <span className="aprs-msg-addr" data-testid="aprs-msg-addr">
          {msg.direction === 'out' ? (
            <span className="aprs-msg-from">me</span>
          ) : (
            <span className="aprs-msg-from">{msg.from}</span>
          )}
          <span className="aprs-msg-arrow" aria-hidden="true">
            {' → '}
          </span>
          {broadcast ? (
            <span className="aprs-msg-to aprs-msg-to-all">all</span>
          ) : (
            <span className="aprs-msg-to">{msg.to}</span>
          )}
        </span>
        <span className="aprs-msg-time" data-testid="aprs-msg-time">
          {formatTime(msg.at)}
        </span>
      </div>
      <div className="aprs-msg-body">
        <span className="aprs-msg-text">{msg.text}</span>
        {msg.direction === 'out' && (
          <span className="aprs-msg-state">
            {broadcast ? (
              // Broadcast is fire-and-forget: surface "broadcast · sent" with NO
              // delivery checkmark, ever.
              <span
                className="aprs-chip aprs-chip-broadcast"
                data-testid="aprs-broadcast-chip"
              >
                Broadcast · sent
              </span>
            ) : (
              msg.state && <DeliveryChip state={msg.state} msg={msg} />
            )}
          </span>
        )}
      </div>
    </li>
  );
}

export interface AprsChatPanelProps {
  /// The open channel — one flat, time-ordered feed (owned by AppShell's lifted
  /// useAprsChat).
  messages: ChannelMessage[];
  /// Stations heard on the channel, most-recent-first; backs the recipient
  /// dropdown.
  heardStations: HeardStation[];
  /// Whether the backend listener is armed (mirrors the backend).
  listening: boolean;
  /// Send `text` to `recipient` (null/empty ⇒ broadcast); resolves with the
  /// backend tracking id (rejects → no message appended).
  send: (recipient: string | null, text: string) => Promise<string>;
  /// Read the live APRS config — used to seed the Path field.
  getConfig: () => Promise<AprsConfigDto>;
  /// Persist the APRS config (full DTO) — used to save an edited Path.
  setConfig: (dto: AprsConfigDto) => Promise<void>;
  /// Optional device-control slot rendered above the composer. The seam for the
  /// UV-Pro native control surface; undefined until the native backend lands.
  controlStrip?: ReactNode;
}

export function AprsChatPanel({
  messages,
  heardStations,
  listening,
  send,
  getConfig,
  setConfig,
  controlStrip,
}: AprsChatPanelProps) {
  const [recipient, setRecipient] = useState('');
  const [text, setText] = useState('');
  const [sendError, setSendError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [toggling, setToggling] = useState(false);

  // Path control — seeded from the live config, persisted on blur/commit. The
  // full config DTO is cached so a save is a read-modify-write (the backend
  // command takes the whole DTO).
  const [config, setLocalConfig] = useState<AprsConfigDto | null>(null);
  const [path, setPath] = useState('');
  const [pathError, setPathError] = useState<string | null>(null);

  const recipientListId = useId();
  const feedRef = useRef<HTMLOListElement | null>(null);

  // Seed the Path field once from the backend config.
  useEffect(() => {
    let mounted = true;
    getConfig()
      .then((cfg) => {
        if (!mounted) return;
        setLocalConfig(cfg);
        setPath(cfg.path);
      })
      .catch(() => {
        // Backend absent (tests) — Path stays empty/editable; no crash.
      });
    return () => {
      mounted = false;
    };
  }, [getConfig]);

  // Keep the feed pinned to the newest message as traffic arrives.
  useEffect(() => {
    const el = feedRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length]);

  const broadcastMode = recipient.trim() === '';

  // Start/Stop listening toggle. The backend is the source of truth:
  // `listening` flips when the backend emits aprs-listening:change, NOT
  // optimistically here. A failed start surfaces through the inline notice.
  const onToggleListening = async () => {
    if (toggling) return;
    setSendError(null);
    setToggling(true);
    try {
      await invoke(listening ? 'aprs_listen_stop' : 'aprs_listen_start');
    } catch (err) {
      setSendError(
        err instanceof Error
          ? err.message
          : listening
            ? 'Could not stop listening.'
            : 'Could not start listening — check the radio link.',
      );
    } finally {
      setToggling(false);
    }
  };

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body || sending) return;
    // Empty recipient ⇒ broadcast (send normalizes empty/whitespace to null).
    const to = recipient.trim() ? recipient.trim().toUpperCase() : null;
    setSendError(null);
    setSending(true);
    try {
      await send(to, body);
      setText('');
    } catch (err) {
      // RF-honesty: the hook appends NO message on a rejected send. Surface the
      // failure as an inline notice instead of a phantom "sent" row.
      setSendError(err instanceof Error ? err.message : 'Send rejected — not queued.');
    } finally {
      setSending(false);
    }
  };

  // Persist an edited Path (read-modify-write of the full config DTO). Called on
  // blur and on Enter in the Path field; a no-op when unchanged or config is
  // not yet loaded.
  const commitPath = async () => {
    if (!config) return;
    const next = path.trim();
    if (next === config.path) {
      setPathError(null);
      return;
    }
    setPathError(null);
    try {
      const dto: AprsConfigDto = { ...config, path: next };
      await setConfig(dto);
      setLocalConfig(dto);
    } catch (err) {
      setPathError(err instanceof Error ? err.message : 'Could not save path.');
    }
  };

  const hasMessages = messages.length > 0;

  return (
    <section className="aprs-chat" data-testid="aprs-chat-panel">
      <header className="aprs-chat-h">
        <span className="aprs-chat-title">APRS channel</span>
        <span
          className={`aprs-listening ${listening ? 'aprs-listening-on' : 'aprs-listening-off'}`}
          data-testid="aprs-listening-indicator"
          data-listening={listening}
        >
          <span className="aprs-listening-dot" />
          {listening ? 'Listening' : 'Not listening — radio disconnected'}
        </span>
        <button
          type="button"
          className="aprs-listen-toggle"
          data-testid="aprs-listen-toggle"
          aria-pressed={listening}
          disabled={toggling}
          onClick={onToggleListening}
        >
          {listening ? 'Stop' : 'Start'}
        </button>
      </header>

      <p
        className="aprs-open-channel"
        data-testid="aprs-open-channel"
        title="APRS is received by every station in range and digipeated — not a private channel."
      >
        Open channel — every station in range hears this.
      </p>

      <ol className="aprs-feed" data-testid="aprs-feed" ref={feedRef}>
        {!hasMessages && (
          <li className="aprs-empty-state" data-testid="aprs-empty-state">
            No traffic yet — heard messages and your sends appear here.
          </li>
        )}
        {messages.map((msg) => (
          <FeedRow key={msg.id} msg={msg} />
        ))}
      </ol>

      {sendError && (
        <p className="aprs-send-error" data-testid="aprs-send-error" role="alert">
          {sendError}
        </p>
      )}

      {controlStrip}

      <form className="aprs-composer" onSubmit={onSubmit}>
        <div className="aprs-composer-row">
          <label className="aprs-composer-recipient">
            <span className="aprs-visually-hidden">Recipient callsign</span>
            <input
              type="text"
              className="aprs-input aprs-input-recipient"
              data-testid="aprs-composer-recipient"
              list={recipientListId}
              placeholder="To (empty = all)"
              value={recipient}
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
              onChange={(e) => setRecipient(e.target.value)}
            />
            <datalist id={recipientListId} data-testid="aprs-heard-stations">
              {heardStations.map((s) => (
                <option key={s.call} value={s.call} />
              ))}
            </datalist>
          </label>
          <span
            className={`aprs-recipient-mode ${broadcastMode ? 'aprs-recipient-mode-broadcast' : 'aprs-recipient-mode-directed'}`}
            data-testid="aprs-recipient-mode"
            data-broadcast={broadcastMode}
          >
            {broadcastMode ? '→ all' : '→ directed'}
          </span>
        </div>

        <div className="aprs-composer-row">
          <label className="aprs-composer-text">
            <span className="aprs-visually-hidden">Message</span>
            <input
              type="text"
              className="aprs-input"
              data-testid="aprs-composer-text"
              placeholder="Message"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </label>
          <span
            className={`aprs-char-count ${text.length > APRS_TEXT_MAX ? 'aprs-char-count-over' : ''}`}
            data-testid="aprs-char-count"
            aria-live="polite"
          >
            {text.length} / {APRS_TEXT_MAX}
          </span>
          <button
            type="submit"
            className="aprs-send-btn"
            data-testid="aprs-send-btn"
            disabled={sending || !text.trim()}
          >
            Send
          </button>
        </div>

        <div className="aprs-composer-row aprs-path-row">
          <label className="aprs-composer-path">
            <span className="aprs-path-label">Path</span>
            <input
              type="text"
              className="aprs-input aprs-input-path"
              data-testid="aprs-composer-path"
              placeholder="WIDE1-1,WIDE2-1"
              value={path}
              spellCheck={false}
              autoCapitalize="characters"
              autoCorrect="off"
              onChange={(e) => setPath(e.target.value)}
              onBlur={commitPath}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void commitPath();
                }
              }}
            />
          </label>
          {pathError && (
            <span className="aprs-path-error" data-testid="aprs-path-error" role="alert">
              {pathError}
            </span>
          )}
        </div>
      </form>
    </section>
  );
}
