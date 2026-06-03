// Empty-state for the reading pane. Lives in its own module (separate from
// MessageView.tsx) so AppShell can import it eagerly without pulling
// MessageView's full dependency graph — most importantly the forms registry,
// which side-effect-imports every ICS-213 / ICS-309 / bulletin / position /
// damage-assessment renderer at module load.
//
// AppShell uses this both as the no-selection render AND as the Suspense
// fallback while the lazy MessageView chunk loads on first message open.
// tuxlink-djnl follow-up (Codex adrev item: "Split MessageView — keep a tiny
// eager empty pane and lazy-load the real reader on first selection").
//
// MessageView.tsx re-exports `MessageViewEmpty` from here so existing tests
// that import from `./MessageView` keep working unchanged.

import './MessageView.css';

export const SELECT_MESSAGE_COPY = 'Select a message to read.';

export function MessageViewEmpty() {
  return (
    <div
      className="reading-pane reading-pane--center"
      data-testid="message-view-empty"
    >
      {SELECT_MESSAGE_COPY}
    </div>
  );
}

/**
 * Suspense fallback for the lazy MessageView chunk. tuxlink-268k (Codex P3):
 * when `selectedMessage` is set but the lazy chunk is still loading, showing
 * `MessageViewEmpty` flashes the wrong copy ('Select a message to read')
 * paired with a highlighted row. This pane is the loading-specific
 * placeholder: matches `.reading-pane` shape, neutral copy, no misleading
 * "no selection" cue.
 *
 * On a warmly-cached chunk this is visible for ~one frame; on a cold first
 * open (forms registry parse) it can show for ~100ms on Pi5. Either way,
 * better than the empty-pane copy lie.
 */
export function MessageViewLoading() {
  return (
    <div
      className="reading-pane reading-pane--center"
      data-testid="message-view-loading"
      aria-busy="true"
    >
      Loading message…
    </div>
  );
}
