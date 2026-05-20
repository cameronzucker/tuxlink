// Tests for tuxlink-y5c (Task 13) — useMessage hook.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3
// Task-13 test (§6): query key [folder, id], enabled: !!selectedMessage,
// folder from selectedMessage.folder (never assumed Inbox).
//
// The Tauri IPC is mocked; we test the query-key construction + enabled flag.

import { describe, it, expect } from 'vitest';
import {
  buildMessageQueryKey,
  buildMessageQueryOptions,
} from './useMessage';

// ============================================================================
// Task-13 test: query key carries [folder, id] — spec §4.2 requirement
// that folder is always present (never assumed Inbox).
// ============================================================================
describe('buildMessageQueryKey', () => {
  it('includes both folder and id in the key', () => {
    const key = buildMessageQueryKey('sent', 'SENTMID');
    expect(key).toEqual(['message', 'sent', 'SENTMID']);
  });

  it('inbox and sent produce different keys for same id', () => {
    const inboxKey = buildMessageQueryKey('inbox', 'MID1');
    const sentKey = buildMessageQueryKey('sent', 'MID1');
    expect(inboxKey).not.toEqual(sentKey);
  });
});

// ============================================================================
// Task-13 test: enabled is false when selectedMessage is null or undefined.
// ============================================================================
describe('buildMessageQueryOptions', () => {
  it('enabled is false when no selection', () => {
    const opts = buildMessageQueryOptions(null);
    expect(opts.enabled).toBe(false);
  });

  it('enabled is true when both folder and id are present', () => {
    const opts = buildMessageQueryOptions({ folder: 'inbox' as const, id: 'MID1' });
    expect(opts.enabled).toBe(true);
  });

  it('query key matches buildMessageQueryKey output', () => {
    const selection = { folder: 'sent' as const, id: 'SMID' };
    const opts = buildMessageQueryOptions(selection);
    expect(opts.queryKey).toEqual(buildMessageQueryKey('sent', 'SMID'));
  });
});
