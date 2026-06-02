import { describe, it, expect } from 'vitest';
import { isBackendFolder } from './useMailbox';

describe('isBackendFolder', () => {
  it('treats inbox/outbox/sent/archive as backend folders', () => {
    expect(isBackendFolder('inbox')).toBe(true);
    expect(isBackendFolder('outbox')).toBe(true);
    expect(isBackendFolder('sent')).toBe(true);
    // tuxlink-ca5x: Archive is wired through the same `mailbox_list` Tauri
    // command as the other system folders — it just dispatches with
    // folder="archive".
    expect(isBackendFolder('archive')).toBe(true);
  });

  it('treats drafts/deleted as NON-backend folders (no command dispatch)', () => {
    // Drafts is a local store; Deleted is a disabled placeholder (spec §2.2).
    expect(isBackendFolder('drafts')).toBe(false);
    expect(isBackendFolder('deleted')).toBe(false);
  });
});
