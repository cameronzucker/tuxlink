// Tests for reply / reply-all / forward prefill + compose-window opening.
//
// bd issue: tuxlink-cbz (reading-pane action bar, operator decision 2026-05-20)
//
// buildReplyDraft is pure (subject prefix, recipient selection, body quoting).
// openReplyWindow seeds a localStorage draft and opens a compose window via the
// existing `compose_window_open` Tauri command (main-window-gated; the reading
// pane lives in the main window so this is authorized).

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the Tauri invoke boundary — openReplyWindow must not need a real runtime.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
import { invoke } from '@tauri-apps/api/core';

import { buildReplyDraft, openReplyWindow } from './replyActions';
import { loadDraft } from '../compose/useDraft';
import type { ParsedMessage } from './types';

function parsed(over: Partial<ParsedMessage> = {}): ParsedMessage {
  return {
    id: 'MID1',
    subject: 'Net check-in',
    from: 'KK4OBN@winlink.org',
    to: ['NET@winlink.org'],
    cc: ['EOC@winlink.org'],
    date: '2026-05-20T20:46:00Z',
    body: 'line one\nline two',
    attachments: [],
    isForm: false,
    routing: 'via CMS-SSL',
    ...over,
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.mocked(invoke).mockClear();
});

describe('buildReplyDraft — subject', () => {
  it('reply prefixes "Re: "', () => {
    expect(buildReplyDraft(parsed({ subject: 'Net check-in' }), 'reply').subject).toBe('Re: Net check-in');
  });

  it('reply does not double-prefix an existing Re: (case-insensitive)', () => {
    expect(buildReplyDraft(parsed({ subject: 'Re: Net check-in' }), 'reply').subject).toBe('Re: Net check-in');
    expect(buildReplyDraft(parsed({ subject: 'RE: shout' }), 'reply').subject).toBe('RE: shout');
  });

  it('forward prefixes "Fwd: " and does not double-prefix', () => {
    expect(buildReplyDraft(parsed({ subject: 'Net check-in' }), 'forward').subject).toBe('Fwd: Net check-in');
    expect(buildReplyDraft(parsed({ subject: 'Fwd: Net check-in' }), 'forward').subject).toBe('Fwd: Net check-in');
  });
});

describe('buildReplyDraft — recipients', () => {
  it('reply addresses only the sender', () => {
    expect(buildReplyDraft(parsed(), 'reply').to).toBe('KK4OBN@winlink.org');
  });

  it('reply-all addresses sender + original To + Cc, deduplicated', () => {
    const to = buildReplyDraft(
      parsed({ from: 'KK4OBN@winlink.org', to: ['NET@winlink.org', 'KK4OBN@winlink.org'], cc: ['EOC@winlink.org'] }),
      'replyAll',
    ).to;
    // sender first, then unique recipients; sender not duplicated
    expect(to).toBe('KK4OBN@winlink.org; NET@winlink.org; EOC@winlink.org');
  });

  it('forward leaves recipients empty (operator picks)', () => {
    expect(buildReplyDraft(parsed(), 'forward').to).toBe('');
  });
});

describe('buildReplyDraft — body', () => {
  it('reply quotes the original with an attribution line and "> " prefixes', () => {
    const body = buildReplyDraft(parsed({ from: 'KK4OBN@winlink.org', body: 'line one\nline two' }), 'reply').body;
    expect(body).toContain('KK4OBN@winlink.org wrote:');
    expect(body).toContain('> line one');
    expect(body).toContain('> line two');
  });

  it('forward includes a forwarded-message header and the original body', () => {
    const body = buildReplyDraft(parsed({ from: 'KK4OBN@winlink.org', subject: 'Net check-in', body: 'line one' }), 'forward').body;
    expect(body).toMatch(/forwarded message/i);
    expect(body).toContain('From: KK4OBN@winlink.org');
    expect(body).toContain('Subject: Net check-in');
    expect(body).toContain('line one');
  });
});

describe('openReplyWindow', () => {
  it('seeds a prefilled draft and opens a compose window for that draft', async () => {
    await openReplyWindow(parsed({ from: 'KK4OBN@winlink.org', subject: 'Net check-in' }), 'reply');

    expect(invoke).toHaveBeenCalledTimes(1);
    const [cmd, args] = vi.mocked(invoke).mock.calls[0];
    expect(cmd).toBe('compose_window_open');
    const draftId = (args as { draftId: string }).draftId;
    expect(typeof draftId).toBe('string');
    expect(draftId.length).toBeGreaterThan(0);

    // The compose window will load this draft by id on mount.
    const draft = loadDraft(draftId);
    expect(draft).not.toBeNull();
    expect(draft!.to).toBe('KK4OBN@winlink.org');
    expect(draft!.subject).toBe('Re: Net check-in');
    expect(draft!.body).toContain('> ');
    expect(draft!.requestAck).toBe(false);
  });
});
