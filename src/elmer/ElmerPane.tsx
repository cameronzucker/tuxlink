/**
 * ElmerPane — Elmer agent chat surface (AC-11, AC-12, AC-13, AC-14).
 *
 * Layout (AC-13, field-path discipline):
 *   - Primary field path: message list + input/Send row + Stop button +
 *     the EgressArmControl chip (read-only ribbon view).
 *   - Secondary (behind a disclosure): endpoint/model picker.
 *   - ONE calibrated footer (AC-12): "Elmer can be wrong or misled by
 *     message content — check the actual message before you send."
 *   - NO operator-set mode toggle (AC-13).
 *
 * Message list (AC-11, AC-12):
 *   - Turn items → user/assistant bubbles (prose).
 *   - Chip items → visually DISTINCT tool-call chips with the tool name +
 *     status. Ground-truth rendering: chips come from actual tool events,
 *     never from model prose (AC-12).
 *
 * Outcome states (AC-14):
 *   - needsOperator → operator-review callout (surface the OutboxApprovalDialog).
 *   - toolDenied    → tool denied callout (surface the OutboxApprovalDialog if
 *                     the detail signals outbox-gated).
 *   - offline       → friendly "local model unreachable" state.
 *   - cancelled     → "stopped" callout.
 *   - error         → generic error callout.
 *   - running       → "Elmer is thinking…" indicator.
 */

import { memo, useState, useRef, useEffect, useCallback, useMemo, type KeyboardEvent } from 'react';
import { useElmer, keyStatusForOrigins, type ElmerItem, type ElmerPhase } from './useElmer';
import { EgressArmControl } from '../shell/EgressArmControl';
import type { EgressStatusDto } from '../security/egressTypes';
import { PRESETS, inferPreset, isLoopback, originOf, nextModelForPreset } from './elmerModelConfig';
import type { SetKey, KeySource, KeyStatusByOrigin } from './elmerModelConfig';
import { ModelTilePicker } from './ModelTilePicker';
import { renderMarkdown } from '../shell/markdownRender';
import { sanitizeHtml } from '../shell/sanitizeHtml';
import './ElmerPane.css';

// ---------------------------------------------------------------------------
// Detect remedy mapping (G3, R2.6)
// ---------------------------------------------------------------------------

/**
 * Map a detect error reason string (from DetectError::to_reason() on the Rust
 * side, surfaced as the `Error.message` via invoke rejection) plus the endpoint
 * loopback/preset context into a user-facing remedy string.
 *
 * Reason string prefixes from DetectError::to_reason():
 *   "no server: could not connect to ..."  → NoServer
 *   "auth error: check the API key for ..." → Auth
 *   "no models: ..."                        → ZeroModels
 *   "network error: ..."                    → Network (treated as transport)
 *   "server error: HTTP ..."                → non-2xx Status
 *   "bad URL: ..."                          → BadUrl
 */
function detectRemedy(reason: string, endpoint: string): string {
  const lower = reason.toLowerCase();

  // Auth failure (401/403) → re-enter key, using the preset label if known.
  if (lower.startsWith('auth error:')) {
    const presetId = inferPreset(endpoint);
    const preset = PRESETS.find((p) => p.id === presetId);
    const providerLabel = preset && preset.id !== 'custom' ? preset.label : 'this provider';
    return `re-enter the key for ${providerLabel}`;
  }

  // Zero models found → pull a model.
  if (lower.startsWith('no models:')) {
    return 'no models found — pull a model on the server, then Detect again';
  }

  // Transport / connection failure — differentiate loopback vs remote.
  if (lower.startsWith('no server:') || lower.startsWith('network error:')) {
    if (isLoopback(endpoint)) {
      return 'the local AI server (Ollama) may not be running — start it, then Detect again';
    }
    return "check this device's internet connection";
  }

  // Fallback: return the raw reason so the operator sees something useful.
  return reason || 'Could not detect models. Check the endpoint and key.';
}

// ---------------------------------------------------------------------------
// ThinkingIndicator constants
// ---------------------------------------------------------------------------

/** Ham-radio verb phrases cycled by ThinkingIndicator while a run is in progress. */
export const RADIO_VERBS: readonly string[] = [
  'tuning the bands',
  'listening on frequency',
  'working the pileup',
  'spinning the VFO',
  'chasing DX',
  'checking propagation',
  'reading the waterfall',
  'copying your signal',
  'pulling it out of the noise',
  'netting in',
  'keying up',
  'warming up the tubes',
  'checking the SWR',
  'rolling the dial',
  'squelching the static',
  'working simplex',
  'consulting the band plan',
  'peaking the signal',
  'calling CQ',
  'logging the contact',
];

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/**
 * Renders the text body of an assistant turn as sanitized markdown.
 * Memoized on `text` so the parse+sanitize pipeline only re-runs when the
 * content changes, not on every parent re-render.
 *
 * Safe composition: renderMarkdown → sanitizeHtml → dangerouslySetInnerHTML.
 * The sanitizeHtml layer (DOMPurify) strips all event handlers, <script>,
 * <iframe>, and other dangerous constructs — model output is UNTRUSTED.
 */
function AssistantTurnBody({ text }: { text: string }) {
  const html = useMemo(() => sanitizeHtml(renderMarkdown(text)), [text]);
  return (
    <div
      className="elmer-turn-md"
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/**
 * Collapsed "Thinking…" disclosure attached to a COMMITTED assistant item that
 * carried streamed reasoning (phase 2b). Starts collapsed; clicking the toggle
 * expands the reasoning trace. Plain text (the reasoning is a raw thinking
 * trace, not markdown). Rendered only when `reasoning` is non-empty.
 */
function ReasoningDisclosure({ reasoning }: { reasoning: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="elmer-reasoning" data-testid="elmer-reasoning" data-open={open}>
      <button
        type="button"
        className="elmer-reasoning-toggle"
        data-testid="elmer-reasoning-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {open ? '▾' : '▸'} Thinking…
      </button>
      {open && (
        <div className="elmer-reasoning-body" data-testid="elmer-reasoning-body">
          {reasoning}
        </div>
      )}
    </div>
  );
}

/**
 * Per-reply Copy button. Copies the RAW turn text (source markdown for an
 * assistant turn, plain text for a user turn) so a paste lands clean — e.g. to
 * hand an Elmer reply to another troubleshooting agent (tuxlink-efdpi). Mirrors
 * the proven `navigator.clipboard.writeText` pattern shipped in ReportIssueModal;
 * the try/catch tolerates WebKitGTK sandbox configs where the async clipboard
 * API is unavailable. Rendered always-visible (not hover-only) so it is reliably
 * clickable — the whole motivation is that selecting/copying by hand is finicky.
 */
function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      /* clipboard may be unavailable in some WebKitGTK/sandbox configs */
    }
  }, [text]);
  // Revert the "Copied" confirmation back to "Copy" after a short beat.
  useEffect(() => {
    if (!copied) return undefined;
    const id = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(id);
  }, [copied]);
  return (
    <div className="elmer-turn-actions">
      <button
        type="button"
        className="elmer-copy-btn"
        data-testid="elmer-copy-btn"
        aria-label="Copy message"
        onClick={handleCopy}
      >
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  );
}

/**
 * Transient streaming bubble (phase 2b) — shown while a streamed turn is in
 * flight (streamingAnswer or streamingReasoning non-empty), before the EV_TURN
 * finalize swaps it for the committed markdown item.
 *
 *  - The live answer renders as PLAIN text with a blinking cursor (NOT markdown
 *    — avoids half-parsed flicker mid-stream).
 *  - Above it, a "Thinking…" section shows the reasoning trace. It is expanded
 *    while only reasoning has arrived, and AUTO-COLLAPSES once the answer starts
 *    (streamingAnswer non-empty) — the operator's attention follows the answer.
 */
function StreamingBubble({
  answer,
  reasoning,
}: {
  answer: string;
  reasoning: string;
}) {
  const answerStarted = answer.length > 0;
  // Auto-collapse the reasoning once the answer starts; expanded before that.
  const reasoningOpen = !answerStarted;
  return (
    <div
      className="elmer-turn elmer-turn--assistant elmer-streaming-bubble"
      data-testid="elmer-streaming-bubble"
      data-role="assistant"
    >
      <span className="elmer-turn-role">Elmer</span>
      {reasoning.length > 0 && (
        <div
          className="elmer-reasoning"
          data-testid="elmer-reasoning"
          data-open={reasoningOpen}
        >
          <span
            className="elmer-reasoning-toggle elmer-reasoning-toggle--live"
            data-testid="elmer-reasoning-toggle"
            aria-hidden="true"
          >
            {reasoningOpen ? '▾' : '▸'} Thinking…
          </span>
          {reasoningOpen && (
            <div className="elmer-reasoning-body" data-testid="elmer-reasoning-body">
              {reasoning}
            </div>
          )}
        </div>
      )}
      {answerStarted && (
        <span className="elmer-streaming-answer">
          {answer}
          <span
            className="elmer-streaming-cursor"
            data-testid="elmer-streaming-cursor"
            aria-hidden="true"
          />
        </span>
      )}
    </div>
  );
}

/** Renders a single turn, chip, or attribution item. */
function MessageItem({ item }: { item: ElmerItem }) {
  if (item.kind === 'chip') {
    return (
      <div
        className="elmer-chip"
        data-testid="elmer-chip"
        data-tool={item.tool}
        data-status={item.status}
        role="status"
        aria-label={`Tool: ${item.tool} — ${item.status}`}
      >
        <span className="elmer-chip-icon" aria-hidden="true">⚙</span>
        <span className="elmer-chip-tool">{item.tool}</span>
        <span className="elmer-chip-status">{item.status}</span>
      </div>
    );
  }

  // G3: Model attribution marker — rendered like a chip but semantically different.
  if (item.kind === 'attribution') {
    return (
      <div
        className="elmer-chip elmer-attribution"
        data-testid="elmer-model-attribution"
        role="status"
        aria-label={`Model changed: now using ${item.model}`}
      >
        <span className="elmer-chip-status">— now using {item.model} —</span>
      </div>
    );
  }

  const isUser = item.role === 'user';
  return (
    <div
      className={`elmer-turn elmer-turn--${isUser ? 'user' : 'assistant'}`}
      data-testid={isUser ? 'elmer-turn-user' : 'elmer-turn-assistant'}
      data-role={item.role}
    >
      <span className="elmer-turn-role">{isUser ? 'You' : 'Elmer'}</span>
      {isUser
        ? <span className="elmer-turn-text">{item.text}</span>
        : (
          <>
            {item.reasoning && <ReasoningDisclosure reasoning={item.reasoning} />}
            <AssistantTurnBody text={item.text} />
          </>
        )
      }
      {item.text ? <CopyButton text={item.text} /> : null}
    </div>
  );
}

/**
 * "Elmer is thinking…" indicator shown while a run is in progress.
 *
 * Cycles a ham-radio verb phrase every ~3 s and shows an elapsed-time counter.
 * The pulsing dot (::before pseudo-element) is preserved.
 *
 * Accessibility: the outer role="status" carries a stable sr-only label so
 * screen readers get a single announcement; the cycling verb + elapsed are
 * aria-hidden so they do not spam the AT with each tick.
 */
function ThinkingIndicator() {
  const [verb, setVerb] = useState<string>(() => RADIO_VERBS[Math.floor(Math.random() * RADIO_VERBS.length)]);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    // 1-second tick — advances elapsed every tick, advances verb every 3rd tick.
    let ticks = 0;
    let lastVerb = verb;

    const id = setInterval(() => {
      ticks += 1;
      setElapsed((s) => s + 1);

      if (ticks % 3 === 0) {
        // Pick a random verb that is not the current one.
        const pool = RADIO_VERBS.filter((v) => v !== lastVerb);
        const next = pool[Math.floor(Math.random() * pool.length)];
        lastVerb = next;
        setVerb(next);
      }
    }, 1000);

    return () => clearInterval(id);
    // Intentionally exclude `verb` from deps — `lastVerb` is a closure variable
    // that tracks current without causing a re-register on every verb change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Format: <60s → "12s"; >=60s → "2m 05s"
  const elapsedLabel =
    elapsed < 60
      ? `${elapsed}s`
      : `${Math.floor(elapsed / 60)}m ${String(elapsed % 60).padStart(2, '0')}s`;

  return (
    <div
      className="elmer-thinking"
      data-testid="elmer-thinking"
      role="status"
    >
      {/* Stable sr-only label — announced once; doesn't change on each tick. */}
      <span className="elmer-thinking-sr-only">Elmer is working</span>
      {/* Cycling verb — visual-only, not announced. */}
      <span
        className="elmer-thinking-verb"
        data-testid="elmer-thinking-verb"
        aria-hidden="true"
      >
        Elmer is {verb}…
      </span>
      {/* Elapsed counter — visual-only, not announced. */}
      <span
        className="elmer-thinking-elapsed"
        data-testid="elmer-thinking-elapsed"
        aria-hidden="true"
      >
        {elapsedLabel}
      </span>
    </div>
  );
}

/** Renders a terminal-outcome callout. */
function OutcomeCallout({
  phase,
  detail,
  onSwitchProvider,
}: {
  phase: ElmerPhase;
  detail: string;
  /** T10: Called when the operator clicks "Switch provider" from a rateLimited callout. */
  onSwitchProvider?: () => void;
}) {
  if (phase === 'idle' || phase === 'running' || phase === 'done') return null;

  // T10: rateLimited is handled separately — it has a "Switch provider" action button.
  if (phase === 'rateLimited') {
    return (
      <div
        className="elmer-outcome elmer-outcome--rateLimited"
        data-testid="elmer-outcome-rate-limited"
        role="alert"
      >
        <span className="elmer-outcome-rate-limited-msg">
          {detail || 'The provider has reached its request limit for this period (429 rate-limited). No automatic retry — try again later or switch to a different provider.'}
        </span>
        {onSwitchProvider && (
          <button
            type="button"
            className="elmer-outcome-switch-btn"
            data-testid="elmer-switch-provider-btn"
            onClick={onSwitchProvider}
          >
            Switch provider
          </button>
        )}
      </div>
    );
  }

  const callouts: Partial<Record<ElmerPhase, { label: string; testId: string }>> = {
    cancelled: {
      label: 'Run stopped.',
      testId: 'elmer-outcome-cancelled',
    },
    needsOperator: {
      label: detail || 'Operator review required before Elmer can continue.',
      testId: 'elmer-outcome-needs-operator',
    },
    toolDenied: {
      label: detail || 'A tool call was not permitted.',
      testId: 'elmer-outcome-tool-denied',
    },
    offline: {
      label:
        'The local Elmer model is not reachable. Check that the model endpoint is running, then try again.',
      testId: 'elmer-outcome-offline',
    },
    error: {
      label: detail || 'Something went wrong. Try again or check the session log.',
      testId: 'elmer-outcome-error',
    },
  };

  const callout = callouts[phase];
  if (!callout) return null;

  return (
    <div
      className={`elmer-outcome elmer-outcome--${phase}`}
      data-testid={callout.testId}
      role="alert"
    >
      {callout.label}
    </div>
  );
}

// ---------------------------------------------------------------------------
// ModelForm — the Model form (R2.6, G2)
// ---------------------------------------------------------------------------

interface ModelFormProps {
  onSave: (args: { agentEndpoint: string; agentModel: string; key: SetKey; agentTurnTimeoutSecs: number }) => Promise<void>;
  onDetect: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  detectState: import('./useElmer').DetectState;
  initialEndpoint: string;
  initialModel: string;
  initialKeyStatus: import('./elmerModelConfig').KeyStatus;
  initialTurnTimeoutSecs: number;
}

export function ModelForm({
  onSave,
  onDetect,
  detectState,
  initialEndpoint,
  initialModel,
  initialKeyStatus,
  initialTurnTimeoutSecs,
}: ModelFormProps) {
  const [endpoint, setEndpoint] = useState(initialEndpoint);
  const [model, setModel] = useState(initialModel);
  // Focused when the operator picks 'Custom…' so they can type their endpoint.
  const endpointInputRef = useRef<HTMLInputElement>(null);
  const [turnTimeoutSecs, setTurnTimeoutSecs] = useState(() => initialTurnTimeoutSecs);

  // Key affordance state.
  // - keyStatus 'present': show [Replace] [Remove]; after Remove flag clearPending.
  //   After Replace, show an empty input. Empty on save → keep; non-empty → set.
  // - keyStatus 'absent' (non-loopback): show empty input. Non-empty → set; empty → keep.
  // - keyStatus 'unreadable': show quiet message.
  // - loopback endpoint: hide key section entirely.
  const [keyStatus] = useState(initialKeyStatus);
  const [replaceMode, setReplaceMode] = useState(false);
  const [newKeyValue, setNewKeyValue] = useState('');
  const [clearPending, setClearPending] = useState(false);

  // For absent key input.
  const [absentKeyValue, setAbsentKeyValue] = useState('');

  // BUG 1 FIX — track the origin the key affordance belongs to.
  // Seeded from the loaded config endpoint's origin. When the live endpoint's
  // origin diverges from this (operator edits the endpoint to a different host),
  // we reset all pending key state so a stale action (Remove/Replace) cannot
  // carry across an origin change and apply to the wrong keyring account.
  const [keyAffordanceOrigin, setKeyAffordanceOrigin] = useState(
    () => originOf(initialEndpoint),
  );

  // Effect: reset key affordance state when the endpoint's origin changes.
  // This fires whenever `endpoint` changes and the derived origin differs from
  // the origin the current affordance belongs to. An empty/unparseable endpoint
  // produces originOf('')==='' which is treated as "unknown" — we reset in that
  // case too so no stale action survives into a blank-endpoint Save.
  useEffect(() => {
    const liveOrigin = originOf(endpoint);
    if (liveOrigin !== keyAffordanceOrigin) {
      // Origin changed — drop all pending key state to prevent cross-origin pollution.
      setClearPending(false);
      setReplaceMode(false);
      setNewKeyValue('');
      setAbsentKeyValue('');
      setKeyAffordanceOrigin(liveOrigin);
    }
  }, [endpoint, keyAffordanceOrigin]);

  // Determine the current provider preset from the endpoint.
  const currentPreset = inferPreset(endpoint);
  const endpointIsLoopback = isLoopback(endpoint);

  // BUG 1 FIX (rendering side) — the loaded keyStatus belongs to the saved
  // config's origin. When the live endpoint's origin has changed (operator
  // hand-edited it to a different host), we have no stored-key signal for
  // that new origin until the form is Saved and reloaded. Treat it as
  // 'absent' so the form shows an empty key input rather than the stale
  // "Key stored 🔒" label from the old origin.
  // The useEffect above resets pending state (clearPending/replaceMode/etc.)
  // and updates keyAffordanceOrigin when origin changes. Because state updates
  // are batched, we use the already-updated keyAffordanceOrigin to derive the
  // effective status — if they still differ in the same render cycle (before
  // the batch lands), we also clamp to 'absent' there.
  const liveOriginForRender = originOf(endpoint);
  const originMatchesLoadedConfig =
    liveOriginForRender !== '' && liveOriginForRender === originOf(initialEndpoint);
  const effectiveKeyStatus = originMatchesLoadedConfig ? keyStatus : 'absent';

  // Handle provider preset selection.
  // GUARD: if the endpoint has been hand-edited (its value doesn't match any known
  // preset's canonical endpoint), confirm before overwriting (R2.6).
  // "Hand-edited" means: the endpoint differs from what the current inferred preset
  // would have filled. An endpoint that exactly matches a known preset default is
  // NOT considered hand-edited — switching presets replaces it freely.
  const handlePresetChange = useCallback((presetId: string) => {
    const preset = PRESETS.find((p) => p.id === presetId);
    if (!preset) return;

    // 'Custom…' means "I'll enter my own endpoint." Clear the preset endpoint so
    // inferPreset('') resolves to 'custom' (the controlled <select> sticks on
    // Custom instead of snapping back to the inferred preset) and focus the
    // endpoint input so the operator can type. Previously this early-returned,
    // leaving the select to snap back — so Custom looked unselectable/broken.
    if (presetId === 'custom') {
      setEndpoint('');
      endpointInputRef.current?.focus();
      return;
    }

    // Determine if the current endpoint is a known-preset canonical value
    // (i.e., the user hasn't hand-edited it beyond a preset default).
    const endpointMatchesAPresetDefault = PRESETS.some(
      (p) => p.endpoint && p.endpoint === endpoint,
    );
    const endpointIsEmpty = !endpoint;

    // Only show the confirm guard if the endpoint is non-empty AND was hand-edited
    // (i.e., it doesn't exactly match any preset's canonical endpoint).
    const isDirty = !endpointIsEmpty && !endpointMatchesAPresetDefault;

    if (isDirty) {
      const proceed = window.confirm(
        `Replace the current endpoint with the ${preset.label} default?`,
      );
      if (!proceed) return;
    }

    setEndpoint(preset.endpoint);
    // Pre-fill the new provider's default model, but only when the current model is
    // untouched (still the outgoing preset's default) — a hand-edited model is
    // preserved. Decoupled from the endpoint-dirty confirm above, which is about the
    // endpoint only. Shared helper so the tile picker can't drift (tuxlink-wpqwy/T7).
    const nextModel = nextModelForPreset(endpoint, model, presetId);
    if (nextModel !== null) {
      setModel(nextModel);
    }
  }, [endpoint, model]);

  // Build the SetKey payload for the Save action.
  const buildSetKey = useCallback((): SetKey => {
    if (clearPending) {
      return { action: 'clear' };
    }
    // Use effectiveKeyStatus (origin-aware), NOT the stale mount-only keyStatus.
    // On an inter-provider switch the form renders the 'absent' key input off
    // effectiveKeyStatus, so the operator types their new key into
    // `absentKeyValue`. Reading raw keyStatus here would take the 'present'
    // branch, IGNORE that value, and send `keep` — leaving the new provider with
    // NO key → 401 on the next message (the reported inter-provider bug).
    if (effectiveKeyStatus === 'present') {
      if (replaceMode && newKeyValue) {
        return { action: 'set', value: newKeyValue };
      }
      // Replace mode with empty value → keep.
      return { action: 'keep' };
    }
    if (effectiveKeyStatus === 'absent') {
      if (absentKeyValue) {
        return { action: 'set', value: absentKeyValue };
      }
      return { action: 'keep' };
    }
    // unreadable / other → keep.
    return { action: 'keep' };
  }, [clearPending, effectiveKeyStatus, replaceMode, newKeyValue, absentKeyValue]);

  // Build KeySource for detect call.
  //
  // BUG 2 FIX — derive KeySource from the CURRENT form state, not from the
  // loaded keyStatus alone. If the operator has typed a key into the form
  // (but not yet Saved), Detect must use that inline value so it probes with
  // the key they actually intend — not the old stored key (or none).
  //
  // Priority order:
  //   1. Loopback endpoint → no key needed.
  //   2. Pending inline key in the form (Replace mode with a value typed, OR
  //      absent-state key input with a value typed) → inline.
  //   3. Stored key present for the same origin (no pending change) → useStored.
  //   4. Otherwise → none.
  const buildKeySource = useCallback((): KeySource => {
    if (endpointIsLoopback) {
      return { source: 'none' };
    }
    // Mirror the render-side effectiveKeyStatus (#981): the key affordance is
    // 'absent' whenever the live origin diverges from the LOADED config's origin
    // (an inter-provider switch) OR the stored status was already absent. The form
    // renders the absent-key input off that same condition, so the operator types
    // their new key into `absentKeyValue`. Keying the inline-key term off the raw
    // mount-only `keyStatus` dropped that freshly-typed key on a provider switch
    // (it fell through to 'useStored', probing the OLD origin's key) — so Detect/
    // Test produced a false auth failure (tuxlink-wpqwy / Task 6, the Detect-path
    // analog of the #981 buildSetKey fix).
    const liveOrigin = originOf(endpoint);
    const originMatchesLoadedConfig =
      liveOrigin !== '' && liveOrigin === originOf(initialEndpoint);
    const effectiveAbsent = !originMatchesLoadedConfig || keyStatus === 'absent';
    // Check for a pending inline key value in the form.
    const inlineKey =
      (replaceMode && newKeyValue) ? newKeyValue :
      (effectiveAbsent && absentKeyValue) ? absentKeyValue :
      null;
    if (inlineKey) {
      return { source: 'inline', value: inlineKey };
    }
    // Use the stored key only when the live origin still matches the loaded
    // config's origin and there is no pending removal (clearPending).
    if (keyStatus === 'present' && originMatchesLoadedConfig && !clearPending) {
      return { source: 'useStored' };
    }
    return { source: 'none' };
  }, [endpointIsLoopback, endpoint, initialEndpoint, keyStatus, replaceMode, newKeyValue, absentKeyValue, clearPending]);

  // Save status — surfaces the result of `elmer_config_set` instead of the
  // previous `void handleSave()` swallow. A rejecting config_set (e.g. an empty
  // key, an unparseable endpoint, a keyring/config-write failure) used to fail
  // SILENTLY: the form kept showing the new selection while the backend never
  // changed, so the next message ran the old model with no error. Now the
  // operator sees why a save failed, and gets a confirmation when it succeeds.
  const [saveState, setSaveState] = useState<{ kind: 'idle' } | { kind: 'ok'; model: string } | { kind: 'error'; message: string }>(
    { kind: 'idle' },
  );

  const handleSave = useCallback(async () => {
    const timeout = Number.isFinite(turnTimeoutSecs) ? Math.round(turnTimeoutSecs) : 900;
    setSaveState({ kind: 'idle' });
    try {
      await onSave({
        agentEndpoint: endpoint,
        agentModel: model,
        key: buildSetKey(),
        agentTurnTimeoutSecs: timeout,
      });
      setSaveState({ kind: 'ok', model });
    } catch (e) {
      setSaveState({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }, [onSave, endpoint, model, buildSetKey, turnTimeoutSecs]);

  const handleDetect = useCallback(async () => {
    await onDetect({
      agentEndpoint: endpoint,
      keySource: buildKeySource(),
    });
  }, [onDetect, endpoint, buildKeySource]);

  const handleDetectedModelSelect = useCallback((selectedModel: string) => {
    setModel(selectedModel);
  }, []);

  return (
    <div className="elmer-model-form" data-testid="elmer-model-form">
      {/* Provider preset select */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="elmer-provider-select">
          Provider
        </label>
        <select
          id="elmer-provider-select"
          className="elmer-form-select"
          data-testid="elmer-provider-select"
          value={currentPreset}
          onChange={(e) => handlePresetChange(e.target.value)}
        >
          {PRESETS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      {/* Endpoint input */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="elmer-endpoint-input">
          Endpoint
        </label>
        <input
          id="elmer-endpoint-input"
          ref={endpointInputRef}
          type="text"
          className="elmer-form-input elmer-form-input--mono"
          data-testid="elmer-endpoint-input"
          value={endpoint}
          onChange={(e) => setEndpoint(e.target.value)}
          spellCheck={false}
          autoComplete="off"
        />
      </div>

      {/* API key affordance (hidden for loopback) */}
      {!endpointIsLoopback && (
        <div className="elmer-form-row elmer-form-row--key" data-testid="elmer-key-section">
          <span className="elmer-form-label">API key</span>
          {/* Use effectiveKeyStatus (not keyStatus) so that when the operator
              edits the endpoint to a different origin, the stale "Key stored 🔒"
              label from the old origin is not shown for the new one (Bug 1 fix). */}
          {effectiveKeyStatus === 'present' && !clearPending ? (
            <div className="elmer-key-stored">
              <span className="elmer-key-stored-label">Key stored 🔒</span>
              {replaceMode ? (
                <input
                  type="text"
                  className="elmer-form-input elmer-form-input--mono elmer-key-replace-input"
                  data-testid="elmer-key-replace-input"
                  placeholder="Paste new key…"
                  value={newKeyValue}
                  onChange={(e) => setNewKeyValue(e.target.value)}
                  autoComplete="off"
                  autoFocus
                />
              ) : (
                <div className="elmer-key-stored-actions">
                  <button
                    type="button"
                    className="elmer-key-action-btn"
                    data-testid="elmer-key-replace-btn"
                    onClick={() => setReplaceMode(true)}
                  >
                    Replace
                  </button>
                  <button
                    type="button"
                    className="elmer-key-action-btn elmer-key-action-btn--danger"
                    data-testid="elmer-key-remove-btn"
                    onClick={() => setClearPending(true)}
                  >
                    Remove
                  </button>
                </div>
              )}
            </div>
          ) : effectiveKeyStatus === 'present' && clearPending ? (
            <div className="elmer-key-clear-pending">
              <span className="elmer-key-clear-label">Key will be removed on save</span>
              <button
                type="button"
                className="elmer-key-action-btn"
                data-testid="elmer-key-clear-cancel-btn"
                onClick={() => setClearPending(false)}
              >
                Cancel
              </button>
            </div>
          ) : effectiveKeyStatus === 'absent' ? (
            <input
              type="text"
              className="elmer-form-input elmer-form-input--mono"
              data-testid="elmer-key-input"
              placeholder="API key (optional)"
              value={absentKeyValue}
              onChange={(e) => setAbsentKeyValue(e.target.value)}
              autoComplete="off"
            />
          ) : effectiveKeyStatus === 'unreadable' ? (
            <span className="elmer-key-unreadable" data-testid="elmer-key-unreadable">
              Could not read the saved key (keyring locked)
            </span>
          ) : null}
        </div>
      )}

      {/* Model input + Detect button */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="elmer-model-input">
          Model
        </label>
        <div className="elmer-model-row">
          <input
            id="elmer-model-input"
            type="text"
            className="elmer-form-input elmer-form-input--mono"
            data-testid="elmer-model-input"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
          <button
            type="button"
            className="elmer-detect-btn"
            data-testid="elmer-detect-btn"
            disabled={detectState.status === 'detecting'}
            onClick={handleDetect}
          >
            {detectState.status === 'detecting' ? 'Detecting…' : 'Detect'}
          </button>
        </div>
      </div>

      {/* Per-turn timeout */}
      <div className="elmer-form-row">
        <label className="elmer-form-label" htmlFor="elmer-turn-timeout-input">
          Per-turn timeout (seconds)
        </label>
        <div className="elmer-model-row">
          <input
            id="elmer-turn-timeout-input"
            type="number"
            className="elmer-form-input"
            data-testid="elmer-turn-timeout-input"
            value={turnTimeoutSecs}
            min={30}
            max={3600}
            step={30}
            onChange={(e) => {
              const raw = e.target.value;
              const parsed = parseInt(raw, 10);
              setTurnTimeoutSecs(Number.isNaN(parsed) ? 900 : parsed);
            }}
          />
          <span className="elmer-save-hint" style={{ marginLeft: '0.5em' }}>
            ≈ {Math.round(turnTimeoutSecs / 60)} min
          </span>
        </div>
      </div>

      {/* Detect results */}
      {detectState.status === 'success' && detectState.models.length > 0 && (
        <div className="elmer-detect-results">
          <span className="elmer-detect-count">
            ✓ {detectState.models.length} model{detectState.models.length !== 1 ? 's' : ''} detected
          </span>
          <select
            className="elmer-form-select"
            data-testid="elmer-detected-models-select"
            value={model}
            onChange={(e) => handleDetectedModelSelect(e.target.value)}
          >
            {detectState.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </div>
      )}
      {detectState.status === 'success' && detectState.models.length === 0 && (
        <div className="elmer-detect-zero" data-testid="elmer-detect-zero">
          No models found at this endpoint.
        </div>
      )}
      {detectState.status === 'error' && (
        <div className="elmer-detect-error" data-testid="elmer-detect-error">
          {detectRemedy(detectState.reason, endpoint)}
        </div>
      )}

      {/* Save & use */}
      <div className="elmer-form-save-row">
        <button
          type="button"
          className="elmer-save-btn"
          data-testid="elmer-save-btn"
          onClick={() => { void handleSave(); }}
        >
          Save &amp; use
        </button>
        <span className="elmer-save-hint">Applies to your next message — no restart.</span>
      </div>
      {saveState.kind === 'error' && (
        <div className="elmer-save-error" data-testid="elmer-save-error" role="alert">
          Couldn’t save: {saveState.message}
        </div>
      )}
      {saveState.kind === 'ok' && (
        <div className="elmer-save-ok" data-testid="elmer-save-ok" role="status">
          Saved — Elmer will use <strong>{saveState.model}</strong> on your next message.
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main pane
// ---------------------------------------------------------------------------

export interface ElmerPaneProps {
  /** Live egress-grant snapshot (from useEgressArm in AppShell). Drives the
   *  arm control in the drawer header (relocated here from the ribbon). */
  egressStatus?: EgressStatusDto;
  /** Arm send-authority for the chosen duration (seconds). */
  onArm?: (durationSecs: number) => void;
  /** Disarm send-authority immediately. */
  onDisarm?: () => void;
  /** Re-arm after a tainted session (clears taint + quarantines the tainted
   *  turns — the 2ouqf quarantine_and_rearm path). */
  onRearm?: (durationSecs: number) => void;
  /** True while an arm/disarm/rearm round-trip is in flight. */
  egressBusy?: boolean;
  /** Last arm/disarm/rearm error, surfaced inline by the arm control. */
  egressError?: string | null;
  /** Close the pane (AppShell sets elmerOpen=false). */
  onClose?: () => void;
  /** When true on mount, open the Model section disclosure so the operator
   *  lands directly on the endpoint/model picker (tuxlink-1wi5w). */
  expandModel?: boolean;
}

export const ElmerPane = memo(function ElmerPane({
  egressStatus,
  onArm,
  onDisarm,
  onRearm,
  egressBusy,
  egressError,
  onClose,
  expandModel,
}: ElmerPaneProps) {
  const {
    items,
    streamingAnswer,
    streamingReasoning,
    phase,
    lastOutcome,
    send,
    stop,
    modelConfig,
    modelConfigState,
    configRead,
    configSet,
    detectModels,
    detectState,
  } = useElmer();
  const [input, setInput] = useState('');
  // T10: When the operator clicks "Switch provider" from a rate-limit callout,
  // show the tile picker with a paygo pre-selection. Null = normal message list.
  const [switchProviderFocusTier, setSwitchProviderFocusTier] = useState<import('./elmerModelConfig').ProviderTier | null>(null);
  // tuxlink-1wi5w: when expandModel is true, open the Model section on mount
  // so the operator lands directly on the endpoint/model picker.
  const [advancedOpen, setAdvancedOpen] = useState(() => expandModel === true);
  // T8b: Counter that increments each time the model section is opened, so the
  // picker re-renders (and re-fetches keyStatusByOrigin) on every open — not just
  // on initial state. This replaces the initial-state-only open path (:906).
  const [openCounter, setOpenCounter] = useState(() => (expandModel === true ? 1 : 0));
  // T8b: Per-origin key-status map passed to the tile picker. Populated once
  // when the picker opens (not per-keystroke).
  const [keyStatusByOrigin, setKeyStatusByOrigin] = useState<KeyStatusByOrigin>({});
  const listEndRef = useRef<HTMLDivElement>(null);
  // Ref to track whether configRead has been called, so the eager-load
  // on mount and the disclosure open don't double-call it.
  const configReadCalledRef = useRef(false);

  // Derived: true when the operator has NOT yet completed setup (onboarded=false).
  // Declared here (before any useEffect that closes over it) so it is in-scope in
  // the deps arrays below.  The original declaration lived later in the component;
  // this hoist does not change semantics — modelConfig and modelConfigState are both
  // available from useElmer() above.
  const notOnboarded =
    modelConfigState === 'loaded' &&
    modelConfig !== null &&
    !modelConfig.onboarded;

  // G3: Eagerly load config on mount so the empty-state "Connect a model"
  // button can be shown without the operator needing to open the disclosure first.
  useEffect(() => {
    if (!configReadCalledRef.current) {
      configReadCalledRef.current = true;
      void configRead();
    }
  }, [configRead]);

  // T8b: Fetch keyStatusForOrigins whenever the tile picker is visible.
  //
  // Original gate: only when advancedOpen + openCounter > 0 (gear disclosure open).
  // Fix (tuxlink-wpqwy codex-adrev): the picker is ALSO shown during first-run
  // (notOnboarded=true) and 429-recovery (switchProviderFocusTier !== null) — both
  // render ModelTilePicker WITHOUT opening the advanced disclosure.  Without this
  // broadened gate, keyStatusByOrigin stays {} and the per-tile "key saved" badges
  // never populate during onboarding or 429-switch, defeating T4.
  //
  // OR-condition: fire whenever the picker is visible for any reason.
  //   advancedOpen          → gear disclosure (existing F6-reopen path, openCounter)
  //   notOnboarded          → first-run tile picker in the chat area
  //   switchProviderFocusTier !== null → 429-recovery picker
  //
  // The openCounter dependency is kept so that gear-reopen (F6) re-fetches on
  // every toggle, and the existing F6 reopen test (renders with expandModel=true
  // + onboarded=true, asserts fetch fires on advancedOpen toggle) still passes.
  useEffect(() => {
    const pickerVisible = advancedOpen || notOnboarded || switchProviderFocusTier !== null;
    if (!pickerVisible) return;
    // For the advancedOpen path, maintain the openCounter guard so that the effect
    // is not triggered before the counter is bumped (initial mount with the
    // disclosure closed produces openCounter=0 and advancedOpen=false, so the
    // pickerVisible check above already short-circuits in that case).
    if (advancedOpen && openCounter === 0) return;
    const origins = PRESETS
      .filter((p) => p.endpoint)
      .map((p) => {
        try { return new URL(p.endpoint).origin; } catch { return ''; }
      })
      .filter(Boolean);
    void keyStatusForOrigins(origins).then((map) => {
      // Guard against a nullish map over the IPC boundary (fail-closed to no badges).
      setKeyStatusByOrigin(map ?? {});
    }).catch(() => {
      // Backend unavailable / errored — fall back to an empty map (no badges).
      setKeyStatusByOrigin({});
    });
  }, [advancedOpen, openCounter, notOnboarded, switchProviderFocusTier]);

  // phase 2b — true while a streamed turn is in flight (live buffers non-empty),
  // before EV_TURN finalizes and clears them. Drives the transient streaming
  // bubble and suppresses the duplicate pre-first-token ThinkingIndicator.
  const isStreaming = streamingAnswer.length > 0 || streamingReasoning.length > 0;

  // Auto-scroll to the bottom of the message list on each new item — and as the
  // live streaming buffers grow, so the cursor stays in view mid-stream.
  // Guard for jsdom (tests) where scrollIntoView is not implemented.
  useEffect(() => {
    if (typeof listEndRef.current?.scrollIntoView === 'function') {
      listEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [items, streamingAnswer, streamingReasoning]);

  const handleSend = () => {
    const msg = input.trim();
    if (!msg || phase === 'running') return;
    setInput('');
    send(msg);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // Ctrl+Enter or Enter without Shift sends.
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const isRunning = phase === 'running';
  const isOffline = phase === 'offline';

  return (
    <aside className="elmer-pane" data-testid="elmer-pane" aria-label="Elmer assistant">
      {/* Header: title + close */}
      <div className="elmer-header">
        <span className="elmer-header-title">Elmer</span>
        <span className="elmer-header-sub">AI assistant</span>
        <span className="elmer-header-spacer" />
        <button
          type="button"
          className="elmer-close-button"
          data-testid="elmer-close"
          aria-label="Close Elmer"
          title="Close"
          onClick={() => onClose?.()}
        >
          ×
        </button>
      </div>

      {/* Agent-send authority — relocated from the dashboard ribbon (the merged
          ribbon chip shows state + opens this drawer; the actual arm/disarm/
          re-arm controls live here). onRearm is the 2ouqf quarantine_and_rearm
          path. Rendered only when AppShell wires the egress hook. */}
      {egressStatus && onArm && onDisarm && (
        <div className="elmer-arm-strip" data-testid="elmer-arm-strip">
          <EgressArmControl
            status={egressStatus}
            onArm={onArm}
            onDisarm={onDisarm}
            onRearm={onRearm}
            busy={egressBusy}
            error={egressError}
          />
        </div>
      )}

      {/* T8b: First-run gate — when the operator has not yet completed model
          setup (onboarded=false), render the tile picker IN PLACE of the message
          list. The picker replaces the list entirely; the items.length===0 coupling
          is dropped (state in comment: picker replaces the list). Once onboarded,
          the normal message list is shown.
          T10: "Switch provider" from a rate-limit callout also opens the picker
          in this slot (with focusTier=paygo for paygo pre-selection).
          Settings: when the operator opens the gear/disclosure while onboarded,
          the picker also opens here (settingsPickerOpen) so settings and first-run
          use the SAME surface — one picker mount covers all three cases. */}
      {/* settingsPickerOpen: onboarded + gear disclosure open → show picker in main slot. */}
      {(() => {
        const settingsPickerOpen = !notOnboarded && advancedOpen;
        const showPicker = notOnboarded || switchProviderFocusTier !== null || settingsPickerOpen;
        if (showPicker) {
          if (!(modelConfigState === 'loaded' && modelConfig)) return null;
          return (
            <>
              {/* Back-to-chat affordance: available in switch-provider flow AND
                  settings-picker flow, but NOT during first-run onboarding. */}
              {!notOnboarded && (switchProviderFocusTier !== null || settingsPickerOpen) && (
                <button
                  type="button"
                  className="elmer-back-to-chat-btn"
                  data-testid="elmer-back-to-chat-btn"
                  onClick={() => {
                    setSwitchProviderFocusTier(null);
                    setAdvancedOpen(false);
                  }}
                >
                  ← Back to chat
                </button>
              )}
              <ModelTilePicker
                onSave={async (args) => {
                  // After a SUCCESSFUL save, return to the message list:
                  //   - switch-provider flow: clear switchProviderFocusTier
                  //   - settings flow: close the gear disclosure (advancedOpen=false)
                  // On failure, remain on the picker so the operator can retry.
                  await configSet(args);
                  // A successful save from the picker returns to chat regardless of
                  // which path opened it. Clear both unconditionally rather than
                  // gating on captured render-time flags — a re-render between the
                  // click and configSet resolving could otherwise leave a stale
                  // flag and the picker stuck open. (First-run: onboarded flips
                  // true on the next config read, so these are harmless no-ops.)
                  setSwitchProviderFocusTier(null);
                  setAdvancedOpen(false);
                }}
                onDetect={detectModels}
                detectState={detectState}
                keyStatusByOrigin={keyStatusByOrigin}
                initialEndpoint={modelConfig.agentEndpoint}
                initialModel={modelConfig.agentModel}
                initialKeyStatus={modelConfig.keyStatus}
                initialTurnTimeoutSecs={modelConfig.agentTurnTimeoutSecs ?? 900}
                focusTier={switchProviderFocusTier ?? undefined}
              />
            </>
          );
        }
        /* Message list (normal onboarded path) */
        return (
          <div className="elmer-messages" data-testid="elmer-messages" role="log" aria-live="polite">
            {items.map((item) => (
              <MessageItem key={item.id} item={item} />
            ))}
            {/* phase 2b — transient live streaming bubble; carries the liveness once
                the first token arrives, so the pre-token ThinkingIndicator is hidden
                while it is shown (no double indicator). */}
            {isStreaming && (
              <StreamingBubble answer={streamingAnswer} reasoning={streamingReasoning} />
            )}
            {isRunning && !isStreaming && <ThinkingIndicator />}
            {lastOutcome && (
              <OutcomeCallout
                phase={phase}
                detail={lastOutcome.detail}
                onSwitchProvider={() => { setSwitchProviderFocusTier('paygo'); }}
              />
            )}
            <div ref={listEndRef} />
          </div>
        );
      })()}

      {/* Offline-endpoint friendly state (AC-14) */}
      {isOffline && (
        <div className="elmer-offline-banner" data-testid="elmer-offline-banner" role="alert">
          The local Elmer model is not reachable. Verify the endpoint is running.
        </div>
      )}

      {/* T8b: When not yet onboarded, show a one-line hint above the input area
          so the operator knows why the input is disabled. Dropped once onboarded. */}
      {notOnboarded && (
        <p
          className="elmer-onboarding-hint"
          data-testid="elmer-onboarding-hint"
          role="note"
        >
          Choose a model above to start chatting with Elmer.
        </p>
      )}

      {/* Input area + Stop (always visible, AC-11) */}
      <div className="elmer-input-row" data-testid="elmer-input-row">
        <textarea
          className="elmer-input"
          data-testid="elmer-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask Elmer…"
          rows={3}
          disabled={isRunning || notOnboarded}
          aria-label="Message to Elmer"
        />
        <div className="elmer-input-actions">
          <button
            type="button"
            className="elmer-send-button"
            data-testid="elmer-send"
            disabled={isRunning || notOnboarded || !input.trim()}
            onClick={handleSend}
          >
            Send
          </button>
          {/* Stop is always visible (AC-11); disabled when idle so the
              operator still sees it but it only does something during a run. */}
          <button
            type="button"
            className="elmer-stop-button"
            data-testid="elmer-stop"
            disabled={!isRunning}
            onClick={stop}
          >
            Stop
          </button>
        </div>
      </div>

      {/* Secondary: endpoint/model disclosure toggle (AC-13).
          When onboarded and the operator opens this, the picker appears in the
          main slot (settingsPickerOpen path above). The disclosure body only
          shows loading/error status — the ModelForm is reached via the Other
          tile in the picker, never mounted directly here while onboarded.
          T8b (F6 reopen): the gear toggle bumps openCounter so the picker
          re-fetches keyStatusByOrigin and re-renders on every open — not just
          on initial-state open (the :906 initial-state-only path is replaced). */}
      <div className="elmer-advanced" data-testid="elmer-advanced">
        <button
          type="button"
          className="elmer-advanced-toggle"
          data-testid="elmer-advanced-toggle"
          aria-expanded={advancedOpen}
          onClick={() => {
            const next = !advancedOpen;
            setAdvancedOpen(next);
            // T8b: bump the open-counter every time the section opens, so the
            // keyStatusForOrigins effect re-runs (gear-reopen / F6).
            if (next) {
              setOpenCounter((c) => c + 1);
            }
            // Load config only if it hasn't been triggered yet (the eager-mount
            // load may have already initiated it — check the ref, not the state,
            // to avoid a double-call between mount useEffect and toggle click).
            if (next && !configReadCalledRef.current) {
              configReadCalledRef.current = true;
              void configRead();
            }
          }}
        >
          {advancedOpen ? '▴' : '▾'} Endpoint / model
        </button>
        {advancedOpen && (
          <div className="elmer-advanced-body" data-testid="elmer-advanced-body">
            {modelConfigState === 'loading' && (
              <p className="elmer-advanced-loading">Loading…</p>
            )}
            {modelConfigState === 'error' && (
              <p className="elmer-advanced-error">Could not load config.</p>
            )}
            {/* When onboarded, the picker renders in the main slot (settingsPickerOpen).
                The disclosure body intentionally shows no form here — the model picker
                above is the model-settings surface. ModelForm is reached via the
                Other tile in the picker (custom/openrouter endpoint path). */}
          </div>
        )}
      </div>

      {/* ONE calibrated footer (AC-12) */}
      <div className="elmer-footer" data-testid="elmer-footer">
        Elmer can be wrong or misled by message content — check the actual message before you send.
        {/* T10: Persistent provider-class indicator for cloud tiers. Shows only when
            onboarded and the active endpoint maps to a known cloud preset. Local
            (loopback) endpoints do not show a cloud indicator. */}
        {modelConfigState === 'loaded' && modelConfig && modelConfig.onboarded && (() => {
          const presetId = inferPreset(modelConfig.agentEndpoint);
          const preset = PRESETS.find((p) => p.id === presetId);
          if (!preset || preset.tier === 'local' || preset.tier === 'other') return null;
          const tierLabel = preset.tier === 'free' ? 'free cloud' : 'cloud';
          return (
            <span
              className="elmer-provider-indicator"
              data-testid="elmer-provider-indicator"
            >
              {' '}· Using {preset.label} ({tierLabel})
            </span>
          );
        })()}
      </div>
    </aside>
  );
});
