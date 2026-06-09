import type { MailboxFolderRef } from './types';

/// Folders that carry read-state (received mail): Inbox, Archive, and any
/// user-folder slug. Sent/Outbox/Drafts/Deleted are the operator's own or
/// non-received messages and never track unread.
const READLESS = new Set(['sent', 'outbox', 'drafts', 'deleted']);

export function folderBearsReadState(folder: MailboxFolderRef): boolean {
  return !READLESS.has(folder);
}
