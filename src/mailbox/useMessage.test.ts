// Tests for tuxlink-y5c (Task 13) — useMessage hook.
//
// Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §5.3
// Task-13 test (§6): query key [folder, id], enabled: !!selectedMessage,
// folder from selectedMessage.folder (never assumed Inbox).
//
// The Tauri IPC is mocked; we test the query-key construction + enabled flag.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';

// vi.mock is hoisted above imports, so mockInvoke must be declared via vi.hoisted
// so it's available inside the factory closure.
const mockInvoke = vi.hoisted(() =>
  vi.fn(async (_cmd: string) => ({
    id: 'INBOX1',
    subject: 's',
    from: 'f',
    to: [],
    cc: [],
    date: '2026-05-19T00:00:00Z',
    body: 'b',
    attachments: [],
    isForm: false,
    routing: null,
  })),
);

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));

import {
  buildMessageQueryKey,
  buildMessageQueryOptions,
  useMessage,
} from './useMessage';
import { saveDraft } from '../compose/useDraft';

beforeEach(() => {
  mockInvoke.mockClear();
  globalThis.localStorage?.clear?.();
});

function wrapperWith(qc: QueryClient) {
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

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

// ============================================================================
// tuxlink-etxt Task 7: mark-on-open via once-per-transition client effect.
//
// Opening a received-mail message (inbox / archive / user-folder) calls
// message_set_read_state(..., read: true) ONCE per open transition, then
// invalidates the mailbox cache so the unread badge refreshes.
// Re-renders / refetches of the SAME selection must NOT fire it again so
// that an explicit "Mark Unread" on the currently-open message sticks.
// Sent / Outbox / Drafts / Deleted never trigger the mark.
// ============================================================================
describe('useMessage — mark-on-open (Task 7)', () => {
  it('calls message_set_read_state with read:true when an inbox message loads', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    renderHook(() => useMessage({ folder: 'inbox', id: 'M1' }), {
      wrapper: wrapperWith(qc),
    });

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('message_set_read_state', {
        folder: 'inbox',
        id: 'M1',
        read: true,
      }),
    );
  });

  it('invalidates the mailbox cache after marking read', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    renderHook(() => useMessage({ folder: 'inbox', id: 'M2' }), {
      wrapper: wrapperWith(qc),
    });

    await waitFor(() =>
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['mailbox'] }),
    );
  });

  it('does NOT call message_set_read_state again on re-render with the same selection', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    const { rerender } = renderHook(() => useMessage({ folder: 'inbox', id: 'M3' }), {
      wrapper: wrapperWith(qc),
    });

    // Wait for the first successful load + mark.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('message_set_read_state', {
        folder: 'inbox',
        id: 'M3',
        read: true,
      }),
    );

    const callCountAfterFirstLoad = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'message_set_read_state',
    ).length;

    // Re-render (simulates a TanStack refetch updating dataUpdatedAt).
    rerender();
    rerender();

    // Count must not increase — the ref guard prevents re-marking the same message.
    const callCountAfterRerender = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'message_set_read_state',
    ).length;
    expect(callCountAfterRerender).toBe(callCountAfterFirstLoad);
  });

  it('does NOT call message_set_read_state for a sent message', async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    const { result } = renderHook(() => useMessage({ folder: 'sent', id: 'SENT1' }), {
      wrapper: wrapperWith(qc),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    const markCalls = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'message_set_read_state',
    );
    expect(markCalls).toHaveLength(0);
  });

  it('does NOT call message_set_read_state for outbox / drafts / deleted', async () => {
    for (const folder of ['outbox', 'drafts', 'deleted'] as const) {
      mockInvoke.mockClear();
      if (folder === 'drafts') {
        saveDraft({
          draftId: 'X1',
          to: 'KK4XYZ@winlink.org',
          subject: 'Local draft',
          body: 'Local body',
          requestAck: false,
        });
      }
      const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

      const { result } = renderHook(() => useMessage({ folder, id: 'X1' }), {
        wrapper: wrapperWith(qc),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));

      const markCalls = mockInvoke.mock.calls.filter(
        (c) => c[0] === 'message_set_read_state',
      );
      expect(markCalls).toHaveLength(0);
    }
  });

  it('reads Drafts messages from localStorage without invoking message_read (tuxlink-2l66)', async () => {
    saveDraft({
      draftId: 'draft-hook',
      to: 'KK4XYZ@winlink.org; W7SRC@winlink.org',
      cc: 'N0CALL@winlink.org',
      subject: 'Hook draft',
      body: 'Draft body for reading pane.',
      requestAck: true,
    });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    const { result } = renderHook(() => useMessage({ folder: 'drafts', id: 'draft-hook' }), {
      wrapper: wrapperWith(qc),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(result.current.data?.subject).toBe('Hook draft');
    expect(result.current.data?.to).toEqual(['KK4XYZ@winlink.org', 'W7SRC@winlink.org']);
    expect(result.current.data?.cc).toEqual(['N0CALL@winlink.org']);
    expect(result.current.data?.body).toBe('Draft body for reading pane.');
    expect(mockInvoke).not.toHaveBeenCalledWith('message_read', expect.anything());
  });

  it('re-marks when reopening a message after navigating away', async () => {
    // open A → marks A; rerender to B → marks B; rerender back to A → marks A again.
    // The ref tracks the LAST marked key (a string), not a visited-Set — so returning
    // to A after visiting B resets markedRef to 'inbox/B', which differs from
    // 'inbox/A', causing the effect to fire again. A visited-Set alternative would
    // permanently suppress the re-mark for A, violating the spec requirement that
    // "Mark Unread" on A can be re-set by re-opening A after leaving it.
    type Props = { id: string };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    const { rerender } = renderHook(
      ({ id }: Props) => useMessage({ folder: 'inbox', id }),
      { wrapper: wrapperWith(qc), initialProps: { id: 'A' } },
    );

    // Wait for A to be marked.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('message_set_read_state', {
        folder: 'inbox',
        id: 'A',
        read: true,
      }),
    );

    const countAfterA = mockInvoke.mock.calls.filter(
      (c) => c[0] === 'message_set_read_state',
    ).length;

    // Navigate to B — marks B.
    rerender({ id: 'B' });
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('message_set_read_state', {
        folder: 'inbox',
        id: 'B',
        read: true,
      }),
    );

    // Navigate back to A — must re-mark A.
    rerender({ id: 'A' });
    await waitFor(() => {
      // Count all message_set_read_state calls; after returning to A there must be
      // more than the count recorded after the first open of A.
      const total = mockInvoke.mock.calls.filter(
        (c) => c[0] === 'message_set_read_state',
      ).length;
      // countAfterA = 1 (A first open), then B adds 1 = 2; returning to A adds 1 = 3.
      expect(total).toBeGreaterThan(countAfterA + 1);
    });
  });

  // Fix 5 (Codex P2): opening a readless folder (Sent/Outbox/Drafts) must
  // update markedRef so that returning to a previously-marked inbox message
  // re-marks it (the "Mark Unread" stickiness contract on re-open still works).
  // Before the fix, the early-return on !folderBearsReadState skipped the
  // markedRef update, leaving the ref pointing at the prior inbox key; returning
  // to that inbox message looked like a no-change to the ref and suppressed the
  // re-mark.
  //
  // Test strategy: navigate inbox/C → sent/S2 → inbox/C. We must wait for
  // sent/S2's isSuccess to become true (after its query resolves) BEFORE
  // returning to inbox/C, because the ref update for the Sent transition only
  // fires when isSuccess is true. Without that wait, the ref is still 'inbox/C'
  // when we return and the guard incorrectly fires.
  it('re-marks inbox message after navigating through a Sent message (Fix 5)', async () => {
    type Props = { folder: 'inbox' | 'sent'; id: string };
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });

    const { result, rerender } = renderHook(
      ({ folder, id }: Props) => useMessage({ folder, id }),
      { wrapper: wrapperWith(qc), initialProps: { folder: 'inbox', id: 'C' } },
    );

    // Wait for inbox/C to be marked read.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith('message_set_read_state', {
        folder: 'inbox',
        id: 'C',
        read: true,
      }),
    );

    // Open sent/S2 and wait for the sent query to resolve (isSuccess=true),
    // which is when the effect fires and updates markedRef to 'sent/S2'.
    rerender({ folder: 'sent', id: 'S2' });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    // Sent must NOT have triggered a mark invoke.
    const sentMarkCalls = (mockInvoke.mock.calls as unknown[][]).filter(
      (c) => c[0] === 'message_set_read_state' && (c[1] as { folder: string }).folder === 'sent',
    );
    expect(sentMarkCalls).toHaveLength(0);

    // Return to inbox/C — markedRef is now 'sent/S2' so the key 'inbox/C' is new:
    // the guard passes and the mark fires again.
    rerender({ folder: 'inbox', id: 'C' });
    await waitFor(() => {
      const calls = (mockInvoke.mock.calls as unknown[][]).filter(
        (c) => c[0] === 'message_set_read_state' && (c[1] as { id: string }).id === 'C',
      );
      expect(calls.length).toBeGreaterThanOrEqual(2);
    });
  });
});
