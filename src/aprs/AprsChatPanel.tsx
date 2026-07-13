// src/aprs/AprsChatPanel.tsx
//
// APRS tactical-chat inline surface — the OPEN CHANNEL model. The visible
// product, in ONE narrow column that fits the ~400px radio dock:
//
//   header   — title + a quiet open-channel honesty cue (connection state lives
//              in the dock-header AprsConnectStrip, not here)
//   feed     — one flat, time-ordered list of every message heard on the
//              channel plus our own sends (`from → to`, or `→ all` for a
//              broadcast), with honest delivery states on our directed sends.
//              Tapping an inbound row seeds a reply (mechanic B).
//   composer — ONE compose field with inline addressing (`W1AW: msg` directs;
//              otherwise broadcast), a live `→ target` indicator, Send, and a
//              compact editable digipeater Path field
//
// APRS is a party line, not a private chat: there is NO per-callsign thread
// list, NO conversations roster, NO side column, and NO separate recipient
// field. Directed addressing is a leading `CALL:` token in the one compose
// field; tapping a heard station's feed row seeds that token.
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
// Connection is NOT in this panel (settled design): the operator connects from
// the compact AprsConnectStrip in the dock header. This panel is chat-only and
// hosts no Start/Stop control.
//
// Addressing is INLINE in the single compose field (settled: no separate "To:"
// field). Two mechanics:
//   A) A leading callsign token followed by a COLON — `W1AW: msg` — directs the
//      message; no `CALL:` token ⇒ broadcast (the colon is required so ordinary
//      text whose first word is callsign-shaped is never an accidental directed
//      send — Codex adrev 2026-06-14 P1).
//   B) Tapping an inbound feed row seeds the compose field with `<CALL>: `.
// `parseCompose` is the single pure source of truth for the parse.

import { useEffect, useRef, useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import type {
  AprsConfigDto,
  ChannelMessage,
  DeliveryState,
} from './aprsTypes';
import { decodeAprsInfo, type AprsPacketCategory } from './aprsDecode';
import { useFirstOpenTip } from '../onboarding/HintProvider';
import './AprsChatPanel.css';

/// Short monitor-style tag per decoded packet category (tuxlink-hzwc bug #2),
/// shown ahead of the readable summary so the operator can scan traffic types.
const CATEGORY_TAG: Record<AprsPacketCategory, string> = {
  position: 'POS',
  weather: 'WX',
  telemetry: 'TLM',
  status: 'STATUS',
  object: 'OBJ',
  item: 'ITEM',
  mice: 'MIC-E',
  message: 'MSG',
  unknown: 'RAW',
};

/// APRS message text budget — the per-message character cap that makes bounded
/// airtime real (matches the backend codec's ≤67 text limit).
const APRS_TEXT_MAX = 67;

/// Amateur-callsign shape for the inline-addressing leading token: 1-2
/// letters/digits, a digit, 1-3 letters, an optional `-SSID` suffix (1-2
/// alphanumerics). Anchored to the start; an EXPLICIT COLON delimiter is
/// REQUIRED for the leading token to count as an addressee. Case-insensitive;
/// the recipient is normalized to uppercase by `parseCompose`.
///
/// The colon is mandatory (Codex adrev 2026-06-14, P1): a whitespace-only
/// delimiter mis-addressed ordinary text whose first word happened to be
/// callsign-shaped — `K9S are on site`, `B2B test` — silently turning a
/// broadcast into a directed send. APRS is broadcast-by-default, so any
/// ambiguity MUST fall to broadcast, never to an accidental directed
/// transmission. Tap-to-reply seeds `CALL: ` (colon form), so the two
/// addressing paths agree.
const CALLSIGN_TOKEN = /^([A-Za-z0-9]{1,2}[0-9][A-Za-z]{1,3}(?:-[A-Za-z0-9]{1,2})?):/;

/// Parse the addressee out of the single compose field. A leading callsign token
/// immediately followed by a colon (`W1AW: msg`) directs the message to that
/// callsign (body = the remainder, left-trimmed); anything else — INCLUDING a
/// callsign-shaped word with no colon — is a BROADCAST (recipient null, body =
/// the whole input verbatim). Exported for unit testing and the live target
/// indicator.
export function parseCompose(input: string): { recipient: string | null; body: string } {
  const lead = input.replace(/^\s+/, '');
  const m = CALLSIGN_TOKEN.exec(lead);
  if (m) {
    const recipient = m[1].toUpperCase();
    // Body is everything after the matched `CALL:` token, left-trimmed.
    const body = lead.slice(m[0].length).replace(/^\s+/, '');
    return { recipient, body };
  }
  return { recipient: null, body: input };
}

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
///
/// Tap-to-reply (mechanic B): tapping an INBOUND row seeds the compose field
/// with the sender's callsign token via `onReplyTo`. Outbound rows are not
/// reply targets (replying to yourself is meaningless).
function FeedRow({ msg, onReplyTo }: { msg: ChannelMessage; onReplyTo?: (call: string) => void }) {
  const broadcast = msg.to === null;
  const replyable = msg.direction === 'in' && Boolean(onReplyTo);
  // A raw non-message frame is decoded into a readable monitor line; the raw
  // info field is preserved on the row's `title` as a "show raw" affordance
  // (tuxlink-hzwc bug #2).
  const decoded = msg.kind === 'raw' ? decodeAprsInfo(msg.text) : null;
  return (
    <li
      className={`aprs-msg aprs-msg-${msg.direction}${replyable ? ' aprs-msg-replyable' : ''}${decoded ? ' aprs-msg-monitor' : ''}`}
      data-testid="aprs-feed-row"
      data-direction={msg.direction}
      data-broadcast={broadcast}
      data-kind={msg.kind}
      role={replyable ? 'button' : undefined}
      tabIndex={replyable ? 0 : undefined}
      onClick={replyable ? () => onReplyTo?.(msg.from) : undefined}
      onKeyDown={
        replyable
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onReplyTo?.(msg.from);
              }
            }
          : undefined
      }
    >
      <div className="aprs-msg-head">
        <span className="aprs-msg-addr" data-testid="aprs-msg-addr">
          {msg.direction === 'out' ? (
            <span className="aprs-msg-from">me</span>
          ) : (
            <span className="aprs-msg-from">{msg.from}</span>
          )}
          {decoded ? (
            // Monitor rows are beacons heard on the channel, not directed
            // messages — tag the traffic type instead of a "→ all" addressee.
            <span
              className={`aprs-msg-cat aprs-msg-cat-${decoded.category}`}
              data-testid="aprs-msg-cat"
            >
              {CATEGORY_TAG[decoded.category]}
            </span>
          ) : (
            <>
              <span className="aprs-msg-arrow" aria-hidden="true">
                {' → '}
              </span>
              {broadcast ? (
                <span className="aprs-msg-to aprs-msg-to-all">all</span>
              ) : (
                <span className="aprs-msg-to">{msg.to}</span>
              )}
            </>
          )}
        </span>
        <span className="aprs-msg-time" data-testid="aprs-msg-time">
          {formatTime(msg.at)}
        </span>
      </div>
      <div className="aprs-msg-body">
        <span className="aprs-msg-text" title={decoded ? msg.text : undefined}>
          {decoded ? decoded.summary : msg.text}
        </span>
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
  send,
  getConfig,
  setConfig,
  controlStrip,
}: AprsChatPanelProps) {
  // tuxlink-10bkw Task 6: first-open discretionary tip (hintRegistry 'aprs').
  useFirstOpenTip('aprs');
  const [text, setText] = useState('');
  // Directed-send target (tuxlink-hzwc bug #3). `null` ⇒ broadcast (APRS
  // default). Set by tapping a heard station OR by typing a leading `CALL:`
  // token (auto-lifted into the chip). The message field then holds ONLY the
  // body — the callsign lives in the chip, not duplicated into the text.
  const [target, setTarget] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  // Path control — seeded from the live config, persisted on blur/commit. The
  // full config DTO is cached so a save is a read-modify-write (the backend
  // command takes the whole DTO).
  const [config, setLocalConfig] = useState<AprsConfigDto | null>(null);
  const [path, setPath] = useState('');
  const [pathError, setPathError] = useState<string | null>(null);

  const feedRef = useRef<HTMLOListElement | null>(null);
  const composeRef = useRef<HTMLInputElement | null>(null);

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

  // Tap-to-reply (mechanic B): set the directed target to the heard station and
  // focus the field. The callsign goes into the CHIP, not the message text —
  // the field stays clean for the body (bug #3).
  const onReplyTo = (call: string) => {
    setTarget(call.toUpperCase());
    requestAnimationFrame(() => composeRef.current?.focus());
  };

  // Field onChange with inline-addressing convenience: when no target is set
  // and the operator types a leading `CALL:` token, lift it into the chip and
  // keep only the remainder as the body. This preserves manual directing
  // without a separate "To" field, and agrees with tap-to-reply (both end in a
  // chip + clean field).
  const onChangeText = (val: string) => {
    if (target === null) {
      const lead = val.replace(/^\s+/, '');
      const m = parseCompose(lead);
      if (m.recipient) {
        setTarget(m.recipient);
        setText(m.body);
        return;
      }
    }
    setText(val);
  };

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body || sending) return;
    setSendError(null);
    setSending(true);
    try {
      // `target` null ⇒ broadcast (send normalizes anyway). The chip persists
      // after a directed send (chat-style follow-ups); the operator clears it
      // via the chip's ✕ to return to broadcast.
      await send(target, body);
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
    <section className="aprs-chat" data-testid="aprs-chat-panel" data-tour-anchor="aprs">
      <header className="aprs-chat-h">
        <span className="aprs-chat-title">APRS Channel</span>
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
          <FeedRow key={msg.id} msg={msg} onReplyTo={onReplyTo} />
        ))}
      </ol>

      {sendError && (
        <p className="aprs-send-error" data-testid="aprs-send-error" role="alert">
          {sendError}
        </p>
      )}

      {controlStrip}

      <form className="aprs-composer" onSubmit={onSubmit}>
        {target && (
          // Directed-send chip on its own row, so the message field reclaims the
          // full width (bug #3 — the callsign is no longer duplicated into the
          // field AND a separate indicator).
          <div className="aprs-composer-chiprow">
            <span
              className="aprs-compose-chip"
              data-testid="aprs-compose-target"
              data-recipient={target}
            >
              <span className="aprs-compose-chip-arrow" aria-hidden="true">→ </span>
              {target}
              <button
                type="button"
                className="aprs-compose-chip-x"
                data-testid="aprs-compose-target-clear"
                aria-label={`Clear ${target} — broadcast to all instead`}
                title="Clear target (broadcast to all)"
                onClick={() => {
                  setTarget(null);
                  requestAnimationFrame(() => composeRef.current?.focus());
                }}
              >
                ×
              </button>
            </span>
          </div>
        )}
        <div className="aprs-composer-row">
          {!target && (
            <span
              className="aprs-compose-target aprs-compose-target-broadcast"
              data-testid="aprs-compose-target"
              data-recipient=""
              aria-live="polite"
              title="Tap a heard station (or type W1AW: ) to direct; otherwise it goes to all."
            >
              → all
            </span>
          )}
          <label className="aprs-composer-text">
            <span className="aprs-visually-hidden">Message (tap a station or type a callsign to direct)</span>
            <input
              ref={composeRef}
              type="text"
              className="aprs-input"
              data-testid="aprs-composer-text"
              placeholder={target ? `Message to ${target}` : 'Message — tap a station to direct'}
              value={text}
              onChange={(e) => onChangeText(e.target.value)}
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
