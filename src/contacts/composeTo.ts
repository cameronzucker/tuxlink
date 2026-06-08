// "New message" → Compose-To route for the Contacts surface (Task A8).
//
// Mirrors `src/mailbox/replyActions.ts::openReplyWindow`: there is no dedicated
// "compose to X" IPC. We reuse the established draft seam — seed a prefilled
// draft (To = the contact's primary callsign) into the localStorage draft store
// under a fresh id, then open a compose window for that id via
// `compose_window_open` (gated to the MAIN window; the Contacts surface lives in
// the main window, so this is authorized).

import { invoke } from '@tauri-apps/api/core';
import { saveDraft } from '../compose/useDraft';
import { newDraftId } from '../routing';

/// Seed a To-only draft for `to` (a callsign/email/group token) and open a
/// compose window for it. Returns the window-open IPC promise so callers can
/// surface failures; the draft is already persisted, so a window-open reject
/// still leaves the message recoverable from Drafts.
export async function openComposeTo(to: string): Promise<void> {
  const draftId = newDraftId();
  saveDraft({
    draftId,
    to,
    subject: '',
    body: '',
    requestAck: false,
  });
  await invoke('compose_window_open', { draftId });
}
