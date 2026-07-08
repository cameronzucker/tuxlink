/**
 * useElmer — state + actions for the Elmer agent pane (AC-11, AC-14, G2).
 *
 * Wires:
 *   - invoke('elmer_send', { msg })    → sends a user message; runs the agent.
 *   - listen(EV_TURN)                 → streams text turns (user + assistant).
 *   - listen(EV_CHIP)                 → streams tool-call status chips.
 *   - listen(EV_OUTCOME)              → terminal outcome (done/cancelled/error…).
 *   - invoke('elmer_stop')            → abort-first cancel of the in-flight run.
 *   - invoke('elmer_config_read')     → reads {agentEndpoint, agentModel, keyStatus, agentTurnTimeoutSecs} (G2).
 *   - invoke('elmer_config_set', ...) → saves {agentEndpoint, agentModel, key:SetKey, agentTurnTimeoutSecs} (G2).
 *   - invoke('elmer_detect_models', ...) → returns string[] of available model ids (G2).
 *
 * AC-11: the hook accumulates turn + chip events into a typed message list that
 * ElmerPane renders as bubbles (turns) and chips (tool calls). It does NOT
 * forward raw tokens as a streaming character-by-character feed — each EV_TURN
 * event is a complete turn (role + text), so we append it as a discrete message.
 *
 * AC-14 (offline-endpoint): when the outcome's `outcomeKind` is 'offline' (or
 * the detail contains a recognisable offline marker), the hook surfaces a
 * distinct `phase: 'offline'` state so ElmerPane can render a friendly fallback.
 *
 * Security: this hook only invokes `elmer_send` and `elmer_stop`. It never
 * passes a conversation transcript to the backend (AC-5: transcript owned by
 * ElmerSession in Rust, not the React pane).
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { ConfigReadDto, SetKey, KeySource, KeyStatusByOrigin } from './elmerModelConfig';
import {
  EV_CHIP,
  EV_CONTEXT,
  EV_DELTA,
  EV_OUTCOME,
  EV_TURN,
  type ElmerChipPayload,
  type ElmerContextPayload,
  type ElmerDeltaPayload,
  type ElmerOutcomePayload,
  type ElmerTurnPayload,
} from './elmerEvents';

// ---------------------------------------------------------------------------
// Message list item types
// ---------------------------------------------------------------------------

/** A prose turn in the chat (user or assistant). */
export interface ElmerTurnItem {
  kind: 'turn';
  id: string;
  role: string;
  text: string;
  /**
   * The model's accumulated reasoning trace for this turn, if it streamed any
   * (phase 2b). Carried onto the committed assistant item at finalize so the
   * thinking persists (collapsed) alongside the answer. Undefined when the turn
   * streamed no reasoning (or came from a non-streaming provider).
   */
  reasoning?: string;
}

/** A tool-call chip (distinct from prose — AC-12 ground-truth). */
export interface ElmerChipItem {
  kind: 'chip';
  id: string;
  tool: string;
  status: string;
}

/**
 * Model attribution marker — inserted mid-conversation when configSet changes
 * the active model (G3). Styled like a chip but semantically different.
 */
export interface ElmerAttributionItem {
  kind: 'attribution';
  id: string;
  model: string;
}

/**
 * A persisted error outcome (tuxlink-pgbox). An errored run emits no finalizing
 * EV_TURN, so without this the failure would live ONLY in the single-slot
 * `lastOutcome` and be overwritten by the next run's outcome — which is why an
 * operator trying to reproduce an error erased the first instance. Appending the
 * error to the transcript keeps failures in the scrollback: they accumulate, and
 * they are copyable via the per-reply Copy button, so an exact error can be
 * captured for troubleshooting. Only the unclassified `error` phase is persisted
 * here; the actionable recovery states (offline / rateLimited / toolDenied /
 * needsOperator) keep their purpose-built callouts.
 */
export interface ElmerErrorItem {
  kind: 'error';
  id: string;
  /** The raw backend outcome kind (e.g. a provider error class). */
  outcomeKind: string;
  /** Human-readable error detail from the backend, if any. */
  detail?: string;
}

export type ElmerItem =
  | ElmerTurnItem
  | ElmerChipItem
  | ElmerAttributionItem
  | ElmerErrorItem;

// ---------------------------------------------------------------------------
// Outcome / phase
// ---------------------------------------------------------------------------

/** Terminal outcome received from the backend. */
export interface ElmerOutcome {
  outcomeKind: string;
  detail: string;
}

/**
 * High-level pane phase:
 *  - 'idle'       — no run in progress, awaiting a message.
 *  - 'running'    — a run is in progress (70-117 s typical wait).
 *  - 'done'       — last run completed cleanly.
 *  - 'cancelled'  — operator stopped the run.
 *  - 'needsOperator' — egress gated; operator review required.
 *  - 'toolDenied' — a tool call was denied (may surface approval dialog).
 *  - 'offline'    — the local endpoint is unreachable (AC-14 offline state).
 *  - 'error'      — unclassified error.
 */
export type ElmerPhase =
  | 'idle'
  | 'running'
  | 'done'
  | 'cancelled'
  | 'needsOperator'
  | 'toolDenied'
  | 'offline'
  | 'rateLimited'
  | 'error';

// tuxlink-6ompo: terminal phases that represent a FAILED attempt the operator
// may need to capture verbatim. Each is appended to the transcript so failures
// ACCUMULATE in the scrollback (like a normal chat front-end) instead of a
// single-slot callout that the next run silently overwrites — the reported bug
// where iterating on an error erased the previous one. 'done' is success and
// 'cancelled' is an operator abort, so neither persists; 'needsOperator' /
// 'toolDenied' are policy gates (not failures) and keep their callouts only.
const PERSISTED_FAILURE_PHASES: ReadonlySet<ElmerPhase> = new Set<ElmerPhase>([
  'error',
  'offline',
  'rateLimited',
]);

function outcomeKindToPhase(outcomeKind: string): ElmerPhase {
  switch (outcomeKind) {
    case 'done':
      return 'done';
    case 'cancelled':
      return 'cancelled';
    case 'needsOperator':
      return 'needsOperator';
    case 'toolDenied':
      return 'toolDenied';
    case 'offline':
      return 'offline';
    // A free-tier daily cap or provider 429. The Rust outcome kind serializes as
    // `rateLimited` (camelCase, matching needsOperator/toolDenied) — Task 5's serde
    // wire-shape test pins that literal. Surfaced as a distinct recovery callout.
    case 'rateLimited':
      return 'rateLimited';
    // A genuine provider failure (transport error, non-2xx, unparseable) — the Rust
    // `RunOutcome::ProviderError` serializes as `"error"` and PERSISTS to the
    // transcript so the operator can capture a model error verbatim (tuxlink-a1xwx).
    // Before this, provider errors were folded into `needsOperator` (a callout-only
    // gate) and were lost on the next run.
    case 'error':
      return 'error';
    default:
      return 'error';
  }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Model-config state (G2)
// ---------------------------------------------------------------------------

/** Loading state for the model-config form (G2). */
export type ModelConfigLoadState = 'idle' | 'loading' | 'loaded' | 'error';

/** Detection state for the Detect button (G2). */
export type DetectState =
  | { status: 'idle' }
  | { status: 'detecting' }
  | { status: 'success'; models: string[] }
  | { status: 'error'; reason: string };

export interface UseElmer {
  /** Ordered list of turn/chip/attribution items in this conversation. */
  items: ElmerItem[];
  /**
   * Phase 2b — transient live answer buffer for a streamed turn. Accumulates
   * EV_DELTA chunks with deltaKind:'assistant'. Non-empty only while a streamed
   * turn is in flight (before EV_TURN finalizes and clears it). Rendered as
   * PLAIN text with a blinking cursor (NOT markdown — avoids half-parsed flicker).
   */
  streamingAnswer: string;
  /**
   * Phase 2b — transient live reasoning buffer for a streamed turn. Accumulates
   * EV_DELTA chunks with deltaKind:'reasoning'. Non-empty only while a streamed
   * turn is in flight; cleared at finalize (its value is carried onto the
   * committed item's `reasoning`).
   */
  streamingReasoning: string;
  /** Current pane phase (drives UI states). */
  phase: ElmerPhase;
  /** Last terminal outcome, or null if no run has completed yet. */
  lastOutcome: ElmerOutcome | null;
  /** Send a user message. No-op if a run is already in progress. */
  send: (msg: string) => void;
  /** Stop the in-flight run (abort-first cancel). */
  stop: () => void;
  /**
   * tuxlink-vbv2k — Start a fresh conversation. Clears the transcript, streaming
   * buffers, and context meter, and resets the backend conversation so the next
   * message begins with empty context (reclaims a local model's small window).
   * Cancels any in-flight run first; keeps model/endpoint config.
   */
  newConversation: () => void;
  /** G2: Loaded model config (null while loading/error). */
  modelConfig: ConfigReadDto | null;
  /** G2: Load state for model config. */
  modelConfigState: ModelConfigLoadState;
  /** G2: Load the model config from the backend. */
  configRead: () => Promise<void>;
  /** G2+G3: Save the model config. When agentModel changes mid-conversation,
   *  drops a model attribution marker into the transcript before the next turn.
   *  T8: optional advanced fields (numCtx, temperature, systemPromptOverride)
   *  are forwarded to the backend; omitting them leaves the backend values unchanged. */
  configSet: (args: { agentEndpoint: string; agentModel: string; key: SetKey; agentTurnTimeoutSecs: number; numCtx?: number | null; temperature?: number | null; systemPromptOverride?: string | null }) => Promise<void>;
  /** G2: Detect available models for the given endpoint. */
  detectModels: (args: { agentEndpoint: string; keySource: KeySource }) => Promise<void>;
  /** G2: Current detection state. */
  detectState: DetectState;
  /** G3: The model name that was active when the last configSet ran (null if never set). */
  activeModel: string | null;
  /**
   * T8b: True once the operator has completed model setup at least once. Mirrors
   * `ConfigReadDto.onboarded`. Null while config is loading or in error state.
   */
  onboarded: boolean | null;
  /**
   * T7 — Latest context-usage snapshot (null until the first EV_CONTEXT event
   * arrives). Arrives from either the native Ollama path or the OpenAI-compat
   * path; `numCtx` is null when the compat probe could not determine a window
   * (counter-mode). Once non-null, persists across turns so the meter stays
   * visible. Only the fields needed by <ContextMeter> are kept.
   */
  context: { promptTokens: number; numCtx: number | null } | null;
}

/**
 * keyStatusForOrigins — Task 4-fe frontend wrapper.
 *
 * Calls the Tauri `elmer_key_status_for_origins` command and returns a
 * `KeyStatusByOrigin` map (statuses only, never key values). Designed to be
 * called once when the tile picker opens, not per-keystroke.
 *
 * The Rust producer lands in Task 4 — this frontend shim is the T8b consumer
 * side. Tests mock `invoke` to return the map directly.
 */
export async function keyStatusForOrigins(origins: string[]): Promise<KeyStatusByOrigin> {
  const map = await invoke<KeyStatusByOrigin>('elmer_key_status_for_origins', { origins });
  // Fail-closed: if the backend is unavailable and the IPC boundary yields a
  // nullish value, return an empty map (no "key saved" badges) rather than
  // letting undefined reach the picker, which would crash on a per-origin read.
  return map ?? {};
}

let _nextId = 0;
function nextId(): string {
  return `elmer-item-${_nextId++}`;
}

export function useElmer(): UseElmer {
  const [items, setItems] = useState<ElmerItem[]>([]);
  const [phase, setPhase] = useState<ElmerPhase>('idle');
  const [lastOutcome, setLastOutcome] = useState<ElmerOutcome | null>(null);
  // Guard against launching a second send while one is running.
  const running = useRef(false);

  // Phase 2b — transient streaming buffers for the in-flight turn. Held in
  // state (so the streaming bubble re-renders on each chunk) AND mirrored in
  // refs so the once-registered EV_TURN finalize listener can read the latest
  // accumulated reasoning at commit time without re-registering on every chunk.
  const [streamingAnswer, setStreamingAnswer] = useState('');
  const [streamingReasoning, setStreamingReasoning] = useState('');
  const streamingReasoningRef = useRef('');

  // G2: Model-config state.
  const [modelConfig, setModelConfig] = useState<ConfigReadDto | null>(null);
  const [modelConfigState, setModelConfigState] = useState<ModelConfigLoadState>('idle');
  const [detectState, setDetectState] = useState<DetectState>({ status: 'idle' });

  // G3: Track the last-used model name so configSet can detect a model change
  // and insert an attribution marker before the next turn renders.
  const activeModelRef = useRef<string | null>(null);
  const [activeModel, setActiveModel] = useState<string | null>(null);

  // T7: Latest context-usage snapshot, from either the native Ollama path or
  // the OpenAI-compat path. Null until the first EV_CONTEXT event arrives;
  // persists across turns so the meter stays visible.
  const [context, setContext] = useState<{ promptTokens: number; numCtx: number | null } | null>(
    null,
  );

  // Subscribe to all three Elmer event channels for the lifetime of the hook.
  // The listeners are set up once on mount and torn down on unmount. Tauri's
  // `listen` returns an `UnlistenFn`; we collect them and call all on cleanup.
  useEffect(() => {
    // `cancelled` guards the async listener setup against the effect being torn
    // down before the `listen()` promises resolve (React StrictMode's
    // mount→unmount→mount, a Vite HMR re-run, or a fast unmount). Without it the
    // cleanup runs while `unlisteners` is still empty, the handlers register
    // AFTER cleanup, and a listener set leaks every cycle — so each EV_TURN /
    // EV_CHIP fires N times and the response renders N times (tuxlink-hn5k6).
    let cancelled = false;
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // Phase 2b — EV_DELTA: incremental streamed chunk. Route by deltaKind to
      // the matching transient buffer. Reasoning chunks mirror into the ref so
      // the finalize listener (registered once) can carry them onto the
      // committed item. Non-streaming providers never emit this — the EV_TURN
      // path below still works with both buffers empty.
      const unDelta = await listen<ElmerDeltaPayload>(EV_DELTA, (event) => {
        const payload = event.payload;
        if (payload.deltaKind === 'reasoning') {
          streamingReasoningRef.current += payload.chunk;
          setStreamingReasoning((prev) => prev + payload.chunk);
        } else {
          setStreamingAnswer((prev) => prev + payload.chunk);
        }
      });

      const unTurn = await listen<ElmerTurnPayload>(EV_TURN, (event) => {
        const payload = event.payload;
        // Finalize: commit the full turn to `items`, carrying any accumulated
        // streamed reasoning, then CLEAR the transient buffers so the live
        // streaming bubble is replaced by the committed (markdown) item — no
        // double render. A non-streamed turn has empty buffers → reasoning
        // undefined, behaving exactly as before phase 2b (no regression).
        const reasoning = streamingReasoningRef.current || undefined;
        setItems((prev) => [
          ...prev,
          { kind: 'turn', id: nextId(), role: payload.role, text: payload.text, reasoning },
        ]);
        streamingReasoningRef.current = '';
        setStreamingAnswer('');
        setStreamingReasoning('');
      });

      const unChip = await listen<ElmerChipPayload>(EV_CHIP, (event) => {
        const payload = event.payload;
        setItems((prev) => {
          // pf6re: a 'denied' chip flips the most-recent still-'calling' chip for
          // the SAME tool to 'denied' (a durable, structured "that transmit was
          // refused" marker in the persisted transcript) rather than appending a
          // duplicate. Falls back to appending if no matching in-flight chip.
          if (payload.status === 'denied') {
            for (let i = prev.length - 1; i >= 0; i--) {
              const it = prev[i];
              if (it.kind === 'chip' && it.tool === payload.tool && it.status === 'calling') {
                const next = prev.slice();
                next[i] = { ...it, status: 'denied' };
                return next;
              }
            }
          }
          return [
            ...prev,
            { kind: 'chip', id: nextId(), tool: payload.tool, status: payload.status },
          ];
        });
      });

      const unOutcome = await listen<ElmerOutcomePayload>(EV_OUTCOME, (event) => {
        const payload = event.payload;
        const outcome: ElmerOutcome = {
          outcomeKind: payload.outcomeKind,
          detail: payload.detail,
        };
        setLastOutcome(outcome);
        const outcomePhase = outcomeKindToPhase(payload.outcomeKind);
        setPhase(outcomePhase);
        // tuxlink-pgbox + tuxlink-6ompo: persist EVERY failed attempt (error /
        // offline / rateLimited) into the transcript so it survives the next run
        // (the single-slot lastOutcome would otherwise be overwritten — the
        // reported "previous errors swallowed" bug). A failed run emits no
        // EV_TURN, so this is the only durable, copyable record of the failure,
        // and failures now accumulate in the scrollback. The actionable callouts
        // (offline / rateLimited recovery, needsOperator / toolDenied gates)
        // still render for the CURRENT outcome; this ALSO drops a history entry.
        if (PERSISTED_FAILURE_PHASES.has(outcomePhase)) {
          setItems((prev) => [
            ...prev,
            {
              kind: 'error',
              id: nextId(),
              outcomeKind: payload.outcomeKind,
              detail: payload.detail,
            },
          ]);
        }
        running.current = false;
        // A streamed turn that is cancelled, times out, or errors emits NO
        // finalizing EV_TURN, so the EV_TURN clear above never runs and a partial
        // live bubble would linger after the run ended. Clear the transient
        // streaming buffers on every terminal outcome too. On a clean 'done' the
        // EV_TURN handler already cleared them, so this is a harmless no-op.
        streamingReasoningRef.current = '';
        setStreamingAnswer('');
        setStreamingReasoning('');
      });

      // T7: EV_CONTEXT — context-usage snapshot from a completed turn, emitted
      // by either the native Ollama path or the OpenAI-compat path (numCtx is
      // null on compat when the probe couldn't determine a window). Store the
      // latest promptTokens + numCtx so <ContextMeter> can render a fill meter.
      // Once non-null, context persists across turns; the meter never disappears.
      const unContext = await listen<ElmerContextPayload>(EV_CONTEXT, (event) => {
        const { promptTokens, numCtx } = event.payload;
        setContext({ promptTokens, numCtx });
      });

      if (cancelled) {
        // Cleanup already ran (or is about to) — tear these down now so they
        // don't outlive the effect and double-handle events on the next mount.
        unDelta();
        unTurn();
        unChip();
        unOutcome();
        unContext();
        return;
      }
      unlisteners.push(unDelta, unTurn, unChip, unOutcome, unContext);
    };

    void setupListeners();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const send = useCallback((msg: string) => {
    if (running.current) return;
    running.current = true;
    setPhase('running');
    // Phase 2b — reset transient streaming buffers at the start of each send so
    // a prior turn's residue can't bleed into the next live bubble.
    streamingReasoningRef.current = '';
    setStreamingAnswer('');
    setStreamingReasoning('');
    // Append the user's message immediately so the pane feels responsive
    // before the first EV_TURN event arrives (the model takes 70-117 s).
    setItems((prev) => [
      ...prev,
      { kind: 'turn', id: nextId(), role: 'user', text: msg },
    ]);
    void invoke('elmer_send', { msg }).catch((err: unknown) => {
      // elmer_send rejects on NeedsOperator (the backend also emits EV_OUTCOME
      // in that case, so the phase will already be 'needsOperator'). Swallow the
      // rejection here — the EV_OUTCOME listener is the authoritative phase setter.
      // Log for debugging only.
      console.error('[useElmer] elmer_send rejected:', err);
    });
  }, []);

  const stop = useCallback(() => {
    void invoke('elmer_stop');
  }, []);

  // tuxlink-vbv2k: newConversation — start fresh. Clears the transcript +
  // streaming buffers + context meter immediately (responsive), resets phase and
  // the last-outcome callout, and tells the backend to reset ITS conversation so
  // the next turn begins with empty context (the load-bearing part for local
  // models). Model/endpoint config is kept. Cancels any in-flight run backend-side.
  const newConversation = useCallback(() => {
    setItems([]);
    streamingReasoningRef.current = '';
    setStreamingAnswer('');
    setStreamingReasoning('');
    // Reset the context meter to empty (keep the last numCtx so it renders 0%,
    // rather than disappearing) — the fresh conversation has used no context yet.
    setContext((prev) => (prev ? { promptTokens: 0, numCtx: prev.numCtx } : null));
    setLastOutcome(null);
    setPhase('idle');
    running.current = false;
    // Backend conversation reset (cancels any in-flight run; infallible there).
    // Log any transport error for debugging only.
    void invoke('elmer_new_conversation').catch((err: unknown) => {
      console.error('[useElmer] elmer_new_conversation failed:', err);
    });
  }, []);

  // G2: configRead — load model config from backend.
  // G3: Initialises activeModel on first load so configSet can detect changes.
  const configRead = useCallback(async () => {
    setModelConfigState('loading');
    try {
      const dto = await invoke<ConfigReadDto>('elmer_config_read');
      setModelConfig(dto);
      setModelConfigState('loaded');
      // G3: seed activeModel from the loaded config (first read only — don't
      // overwrite if the operator has already done a mid-conversation model save).
      if (activeModelRef.current === null && dto.agentModel) {
        activeModelRef.current = dto.agentModel;
        setActiveModel(dto.agentModel);
      }
    } catch {
      setModelConfigState('error');
    }
  }, []);

  // G2+G3: configSet — save model config to backend.
  // On a model change mid-conversation, inserts an attribution marker into the
  // transcript so the operator can tell which model produced the next turn (G3).
  // T8: optional advanced fields forwarded to the Tauri command.
  const configSet = useCallback(async (args: { agentEndpoint: string; agentModel: string; key: SetKey; agentTurnTimeoutSecs: number; numCtx?: number | null; temperature?: number | null; systemPromptOverride?: string | null }) => {
    await invoke('elmer_config_set', args);

    // G3: If the model changed from the last active model, drop a marker.
    const prev = activeModelRef.current;
    const next = args.agentModel;
    if (next && prev !== null && next !== prev) {
      setItems((prevItems) => [
        ...prevItems,
        { kind: 'attribution', id: nextId(), model: next },
      ]);
    }

    // Always advance the active model after a successful save.
    if (next) {
      activeModelRef.current = next;
      setActiveModel(next);
    }

    // Refresh modelConfig so the Model form — which re-initialises from these
    // props every time the disclosure is collapsed and re-expanded (the form is
    // unmounted on collapse, ElmerPane.tsx) — reflects the just-saved
    // endpoint/model/keyStatus instead of the stale initial load. Silent (no
    // loading state) to avoid a flicker on save.
    try {
      const refreshed = await invoke<ConfigReadDto>('elmer_config_read');
      setModelConfig(refreshed);
    } catch {
      // The save itself succeeded; keep the prior modelConfig if the refresh
      // read fails (the form stays usable with the last-known config).
    }
  }, []);

  // G2: detectModels — detect available models for the given endpoint.
  const detectModels = useCallback(async (args: { agentEndpoint: string; keySource: KeySource }) => {
    setDetectState({ status: 'detecting' });
    try {
      const models = await invoke<string[]>('elmer_detect_models', args);
      setDetectState({ status: 'success', models });
    } catch (err: unknown) {
      const reason = err instanceof Error ? err.message : String(err);
      setDetectState({ status: 'error', reason });
    }
  }, []);

  return {
    items,
    streamingAnswer,
    streamingReasoning,
    phase,
    lastOutcome,
    send,
    stop,
    newConversation,
    modelConfig,
    modelConfigState,
    configRead,
    configSet,
    detectModels,
    detectState,
    activeModel,
    onboarded: modelConfig !== null ? modelConfig.onboarded : null,
    context,
  };
}
