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
