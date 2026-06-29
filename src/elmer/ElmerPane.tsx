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

import { memo, useState, useRef, useEffect, type KeyboardEvent } from 'react';
import { useElmer, type ElmerItem, type ElmerPhase } from './useElmer';
import type { EgressStatusDto } from '../security/egressTypes';
import './ElmerPane.css';

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Renders a single turn or chip item. */
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

  const isUser = item.role === 'user';
  return (
    <div
      className={`elmer-turn elmer-turn--${isUser ? 'user' : 'assistant'}`}
      data-testid={isUser ? 'elmer-turn-user' : 'elmer-turn-assistant'}
      data-role={item.role}
    >
      <span className="elmer-turn-role">{isUser ? 'You' : 'Elmer'}</span>
      <span className="elmer-turn-text">{item.text}</span>
    </div>
  );
}

/** "Elmer is thinking…" indicator shown while a run is in progress. */
function ThinkingIndicator() {
  return (
    <div className="elmer-thinking" data-testid="elmer-thinking" role="status" aria-live="polite">
      Elmer is thinking…
    </div>
  );
}

/** Renders a terminal-outcome callout. */
function OutcomeCallout({ phase, detail }: { phase: ElmerPhase; detail: string }) {
  if (phase === 'idle' || phase === 'running' || phase === 'done') return null;

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
// Main pane
// ---------------------------------------------------------------------------

export interface ElmerPaneProps {
  /** Live egress-grant snapshot (from useEgressArm in AppShell). Read-only here. */
  egressStatus?: EgressStatusDto;
  /** Called when the operator requests a fresh session (clears taint + rearms). */
  onRearm?: (durationSecs: number) => void;
  /** Close the pane (AppShell sets elmerOpen=false). */
  onClose?: () => void;
}

export const ElmerPane = memo(function ElmerPane({
  egressStatus: _egressStatus,
  onRearm: _onRearm,
  onClose,
}: ElmerPaneProps) {
  const { items, phase, lastOutcome, send, stop } = useElmer();
  const [input, setInput] = useState('');
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const listEndRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to the bottom of the message list on each new item.
  // Guard for jsdom (tests) where scrollIntoView is not implemented.
  useEffect(() => {
    if (typeof listEndRef.current?.scrollIntoView === 'function') {
      listEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [items]);

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

      {/* Message list */}
      <div className="elmer-messages" data-testid="elmer-messages" role="log" aria-live="polite">
        {items.map((item) => (
          <MessageItem key={item.id} item={item} />
        ))}
        {isRunning && <ThinkingIndicator />}
        {lastOutcome && (
          <OutcomeCallout phase={phase} detail={lastOutcome.detail} />
        )}
        <div ref={listEndRef} />
      </div>

      {/* Offline-endpoint friendly state (AC-14) */}
      {isOffline && (
        <div className="elmer-offline-banner" data-testid="elmer-offline-banner" role="alert">
          The local Elmer model is not reachable. Verify the endpoint is running.
        </div>
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
          disabled={isRunning}
          aria-label="Message to Elmer"
        />
        <div className="elmer-input-actions">
          <button
            type="button"
            className="elmer-send-button"
            data-testid="elmer-send"
            disabled={isRunning || !input.trim()}
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

      {/* Secondary: endpoint/model picker behind a disclosure (AC-13).
          The primary field path (chat + Stop + arm chip) stays uncluttered;
          the endpoint/model setting is rarely changed after initial setup. */}
      <div className="elmer-advanced" data-testid="elmer-advanced">
        <button
          type="button"
          className="elmer-advanced-toggle"
          data-testid="elmer-advanced-toggle"
          aria-expanded={advancedOpen}
          onClick={() => setAdvancedOpen((o) => !o)}
        >
          {advancedOpen ? '▴' : '▾'} Endpoint / model
        </button>
        {advancedOpen && (
          <div className="elmer-advanced-body" data-testid="elmer-advanced-body">
            {/* Endpoint + model configuration lives here (populated when
                the relevant Tauri commands are wired; placeholder for now). */}
            <p className="elmer-advanced-placeholder">
              Endpoint and model settings are configured in Settings → Elmer.
            </p>
          </div>
        )}
      </div>

      {/* ONE calibrated footer (AC-12) */}
      <div className="elmer-footer" data-testid="elmer-footer">
        Elmer can be wrong or misled by message content — check the actual message before you send.
      </div>
    </aside>
  );
});
