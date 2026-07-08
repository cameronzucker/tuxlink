import { useEffect, useRef } from 'react';
import { useStreamAutoFollow } from './useStreamAutoFollow';

export interface StreamingStatusCardProps {
  /** Current radio verb (used while not yet responding). */
  verb: string;
  /** True once answer tokens are streaming (shows a stable "responding"). */
  isResponding: boolean;
  /** Live answer buffer (plain text). */
  answer: string;
  /** Live reasoning buffer (plain text). */
  reasoning: string;
  /** Estimated tokens so far; a counter shows only when > 0. */
  tokensEstimate: number;
  /** Seconds since the in-flight window began. */
  elapsedSecs: number;
  /** Whether the bounded stream body is expanded. */
  expanded: boolean;
  /** Toggle the expanded body. */
  onToggleExpand: () => void;
}

function formatElapsed(secs: number): string {
  return secs < 60
    ? `${secs}s`
    : `${Math.floor(secs / 60)}m ${String(secs % 60).padStart(2, '0')}s`;
}

/**
 * The single in-flight surface for an Elmer turn (tuxlink-h5azu / 06v9s / d5zns).
 * Collapsed by default to a live counter row; expands to a bounded (~10-line)
 * scrolling box that shows the reasoning trace + streaming answer. Owns the whole
 * running->done window, so there is no bubble<->indicator handoff to glitch.
 */
export function StreamingStatusCard({
  verb,
  isResponding,
  answer,
  reasoning,
  tokensEstimate,
  elapsedSecs,
  expanded,
  onToggleExpand,
}: StreamingStatusCardProps) {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const follow = useStreamAutoFollow(bodyRef);

  // Follow the growing stream inside the box, but only while expanded + pinned.
  useEffect(() => {
    if (expanded) follow.followIfPinned();
  }, [answer, reasoning, expanded, follow]);

  const label = isResponding ? 'responding' : verb;

  return (
    <div className="elmer-stream-card" data-testid="elmer-stream-card" data-expanded={expanded}>
      <button
        type="button"
        className="elmer-stream-head"
        data-testid="elmer-stream-card-toggle"
        aria-expanded={expanded}
        aria-label={expanded ? 'Collapse live output' : 'Expand live output'}
        onClick={onToggleExpand}
      >
        <span className="elmer-stream-chev" aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        <span className="elmer-stream-pulse" aria-hidden="true" />
        <span className="elmer-stream-verb" data-testid="elmer-stream-verb">
          Elmer is{' '}
          <span className={isResponding ? 'elmer-stream-verb-em' : undefined}>{label}</span>…
        </span>
        <span className="elmer-stream-spacer" />
        <span className="elmer-stream-metrics">
          {tokensEstimate > 0 && (
            <>
              <span className="elmer-stream-tokens" data-testid="elmer-stream-tokens">
                ~{tokensEstimate.toLocaleString()} tok
              </span>
              <span aria-hidden="true"> · </span>
            </>
          )}
          <span className="elmer-stream-elapsed" data-testid="elmer-stream-elapsed">
            {formatElapsed(elapsedSecs)}
          </span>
        </span>
      </button>

      {expanded && (
        <div
          className="elmer-stream-body"
          data-testid="elmer-stream-body"
          ref={bodyRef}
          onScroll={follow.onScroll}
        >
          {reasoning.length > 0 && (
            <div className="elmer-stream-reasoning" data-testid="elmer-stream-reasoning">
              {reasoning}
            </div>
          )}
          {answer.length > 0 && (
            <span className="elmer-stream-answer">
              {answer}
              <span
                className="elmer-stream-cursor"
                data-testid="elmer-stream-cursor"
                aria-hidden="true"
              />
            </span>
          )}
          {!follow.atBottom && (
            <button
              type="button"
              className="elmer-stream-jump-live"
              data-testid="elmer-stream-jump-live"
              onClick={follow.jumpToLive}
            >
              ↓ Jump to live
            </button>
          )}
        </div>
      )}
    </div>
  );
}
