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
// Start/Stop listening is NOT owned here — Task 14 adds it. This panel shows a
// read-only listening indicator.

import { useState } from 'react';
import type { FormEvent } from 'react';
import { useAprsChat } from './useAprsChat';
import type { ChatMessage, DeliveryState, Thread } from './aprsTypes';
import './AprsChatPanel.css';

/// Map a delivery state to its operator-facing chip label + variant class.
/// Honest states only — there is no synthetic "delivered".
const CHIP: Record<DeliveryState, { label: string; variant: string }> = {
  sent: { label: 'Sent', variant: 'neutral' },
  acked: { label: 'Acked', variant: 'success' },
  timedOut: { label: 'Timed out', variant: 'warning' },
  rejected: { label: 'Rejected', variant: 'error' },
};

function DeliveryChip({ state }: { state: DeliveryState }) {
  const chip = CHIP[state];
  return (
    <span
      className={`aprs-chip aprs-chip-${chip.variant}`}
      data-testid="aprs-delivery-chip"
      data-state={state}
    >
      {chip.label}
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
      {msg.direction === 'out' && msg.state && <DeliveryChip state={msg.state} />}
    </div>
  );
}

export function AprsChatPanel() {
  const { threads, listening, send } = useAprsChat();
  const [callsign, setCallsign] = useState('');
  const [text, setText] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

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
