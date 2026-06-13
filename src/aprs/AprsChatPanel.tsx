// src/aprs/AprsChatPanel.tsx
//
// APRS tactical-chat inline surface (Task 13). The visible product: a
// per-callsign thread list, a conversation view (in/out bubbles), a composer
// (callsign + text + Send), delivery-state chips on outgoing bubbles, and a
// listening indicator.
//
// Inline only — NO pop-up windows (hard project rule). The surface is
// constrained to a realistic reading-pane width (not stretched full-width)
// and reuses the radio-panel theme tokens for visual consistency.
//
// RF-honesty: delivery chips reflect ONLY the backend-reported `DeliveryState`
// (sent / acked / timedOut / rejected) — no fabricated "delivered". `send`
// delegates entirely to `useAprsChat().send`, which inserts the outgoing
// bubble ONLY on a successful backend ack of queueing; a rejected send is
// caught here and surfaced as an inline notice with NO bubble.
//
// Start/Stop listening (Task 14): the toggle below arms/disarms the backend
// listener via aprs_listen_start / aprs_listen_stop and reflects the hook's
// `listening` state. A failed start (e.g. the Bluetooth/serial link is not
// configured) is caught and surfaced through the same inline error notice the
// composer uses — no fabricated "listening" state.

import { useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ChatMessage, DeliveryState, Thread } from './aprsTypes';
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

function DeliveryChip({ state, msg }: { state: DeliveryState; msg?: ChatMessage }) {
  const chip = CHIP[state];
  const label =
    state === 'acked' && msg?.ackedAt != null
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

function Bubble({ msg }: { msg: ChatMessage }) {
  return (
    <div
      className={`aprs-bubble aprs-bubble-${msg.direction}`}
      data-testid="aprs-bubble"
      data-direction={msg.direction}
    >
      <span className="aprs-bubble-text">{msg.text}</span>
      <span className="aprs-bubble-meta">
        <span className="aprs-bubble-time" data-testid="aprs-bubble-time">
          {formatTime(msg.at)}
        </span>
        {msg.direction === 'out' && msg.state && <DeliveryChip state={msg.state} msg={msg} />}
      </span>
    </div>
  );
}

export interface AprsChatPanelProps {
  /// Per-callsign conversation map (owned by AppShell's lifted useAprsChat).
  threads: Record<string, Thread>;
  /// Whether the backend listener is armed (mirrors the backend).
  listening: boolean;
  /// Send `text` to `call`; resolves with the backend msgid (rejects → no bubble).
  send: (call: string, text: string) => Promise<string>;
  /// Optional device-control slot rendered above the composer. The seam for the
  /// UV-Pro native control surface; undefined until the native backend lands.
  controlStrip?: ReactNode;
}

export function AprsChatPanel({ threads, listening, send, controlStrip }: AprsChatPanelProps) {
  const [callsign, setCallsign] = useState('');
  const [text, setText] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [toggling, setToggling] = useState(false);

  const callsigns = Object.keys(threads);
  const hasThreads = callsigns.length > 0;

  // The active conversation: the explicitly-selected thread, falling back to
  // the composer's callsign (so an outgoing send shows up immediately even
  // before a thread is clicked).
  const activeCall = selected ?? (callsign.trim() ? callsign.trim().toUpperCase() : null);
  const activeThread: Thread | null = activeCall ? threads[activeCall] ?? null : null;

  const onSelectThread = (call: string) => {
    setSelected(call);
    setCallsign(call);
    setSendError(null);
  };

  // Start/Stop listening toggle (Task 14). The backend is the source of truth:
  // `listening` flips when the backend emits aprs-listening:change, NOT
  // optimistically here. A failed start surfaces through the inline notice (the
  // listener could not arm — e.g. the radio link is not configured) and leaves
  // the indicator in its honest "not listening" state.
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
    const call = callsign.trim().toUpperCase();
    const body = text.trim();
    if (!call || !body || sending) return;
    setSendError(null);
    setSending(true);
    try {
      await send(call, body);
      setText('');
      setSelected(call);
    } catch (err) {
      // RF-honesty: the hook inserts NO bubble on a rejected send. Surface the
      // failure as an inline notice instead of a phantom "sent" message.
      setSendError(
        err instanceof Error ? err.message : 'Send rejected — not queued.',
      );
    } finally {
      setSending(false);
    }
  };

  return (
    <section className="aprs-chat" data-testid="aprs-chat-panel">
      <header className="aprs-chat-h">
        <span className="aprs-chat-title">APRS tactical chat</span>
        <span
          className={`aprs-listening ${listening ? 'aprs-listening-on' : 'aprs-listening-off'}`}
          data-testid="aprs-listening-indicator"
          data-listening={listening}
        >
          <span className="aprs-listening-dot" />
          {listening ? 'Listening' : 'Not listening — radio disconnected'}
        </span>
        <span className="aprs-open-channel" data-testid="aprs-open-channel" title="APRS is received by every station in range and digipeated — not a private channel.">
          Heard by all stations in range
        </span>
        <button
          type="button"
          className="aprs-listen-toggle"
          data-testid="aprs-listen-toggle"
          aria-pressed={listening}
          disabled={toggling}
          onClick={onToggleListening}
        >
          {listening ? 'Stop listening' : 'Start listening'}
        </button>
      </header>

      <div className="aprs-chat-body">
        <aside className="aprs-thread-list" data-testid="aprs-thread-list">
          {hasThreads ? (
            <ul className="aprs-thread-ul">
              {callsigns.map((call) => {
                const last = threads[call].messages.at(-1);
                return (
                  <li key={call}>
                    <button
                      type="button"
                      className={`aprs-thread-item ${activeCall === call ? 'aprs-thread-item-active' : ''}`}
                      data-testid="aprs-thread-item"
                      aria-pressed={activeCall === call}
                      onClick={() => onSelectThread(call)}
                    >
                      <span className="aprs-thread-call">{call}</span>
                      {last && (
                        <span className="aprs-thread-preview">{last.text}</span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : (
            <p className="aprs-thread-empty">No conversations</p>
          )}
        </aside>

        <main className="aprs-conversation">
          <div className="aprs-bubbles" data-testid="aprs-bubbles">
            {!hasThreads && (
              <p className="aprs-empty-state" data-testid="aprs-empty-state">
                No conversations yet — send a message or wait for inbound.
              </p>
            )}
            {activeThread &&
              activeThread.messages.map((msg) => <Bubble key={msg.id} msg={msg} />)}
          </div>

          {sendError && (
            <p className="aprs-send-error" data-testid="aprs-send-error" role="alert">
              {sendError}
            </p>
          )}

          {controlStrip}

          <form className="aprs-composer" onSubmit={onSubmit}>
            <label className="aprs-composer-call">
              <span>Callsign</span>
              <input
                type="text"
                className="aprs-input"
                data-testid="aprs-composer-callsign"
                placeholder="W7RPT-9"
                value={callsign}
                spellCheck={false}
                autoCapitalize="characters"
                autoCorrect="off"
                onChange={(e) => {
                  setCallsign(e.target.value);
                  // A hand-typed callsign drops the selected-thread pin so the
                  // composer drives the active conversation.
                  setSelected(null);
                }}
              />
            </label>
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
              disabled={sending || !callsign.trim() || !text.trim()}
            >
              Send
            </button>
          </form>
        </main>
      </div>
    </section>
  );
}
