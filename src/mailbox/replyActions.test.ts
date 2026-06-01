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

import { buildReplyDraft, openReplyWindow, hasReplyWithFormSupport } from './replyActions';
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

// Codex P2 (2026-05-20): the reader hides Winlink form payloads behind a
// placeholder and never silently drops user data. Reply/Forward must honor
// both — no rendered form content in the quote, and a visible note when a
// forward cannot carry the original attachments (compose can't attach files
// yet).
describe('buildReplyDraft — form + attachment safety', () => {
  const FORM_BODY_RENDERED =
    'GENERAL MESSAGE (ICS 213)\n1. Incident Name: WALDO\n4. Subject: REQUEST SUPPLIES';
  const FORM_PAYLOAD = {
    formId: 'ICS213_Initial',
    formParameters: {
      xmlFileVersion: '1.0',
      rmsExpressVersion: 'Tuxlink/0.3.0',
      submissionDatetime: '20260530143000',
      sendersCallsign: 'N0CALL',
      gridSquare: 'FM18',
      displayForm: 'ICS213_Initial_Viewer.html',
      replyTemplate: 'ICS213_SendReply.0',
    },
    fields: [
      ['inc_name', 'WALDO'],
      ['to_name', 'JOHN'],
      ['fm_name', 'JANE'],
      ['subjectline', 'REQUEST SUPPLIES'],
      ['mdate', '2026-05-30'],
      ['mtime', '14:30Z'],
      ['message', 'Need bandages.'],
    ] as [string, string][],
  };

  it('reply on a form does not quote the rendered body; substitutes a safe placeholder', () => {
    const msg = parsed({
      isForm: true,
      body: FORM_BODY_RENDERED,
      formId: 'ICS213_Initial',
      formPayload: FORM_PAYLOAD,
    });
    const body = buildReplyDraft(msg, 'reply').body;
    expect(body).not.toContain('GENERAL MESSAGE');
    expect(body).not.toContain('WALDO');
    expect(body.toLowerCase()).toContain('form');
  });

  it('forward on a form does not include the rendered body', () => {
    const msg = parsed({
      isForm: true,
      body: FORM_BODY_RENDERED,
      formId: 'ICS213_Initial',
      formPayload: FORM_PAYLOAD,
    });
    const body = buildReplyDraft(msg, 'forward').body;
    expect(body).not.toContain('GENERAL MESSAGE');
    expect(body).not.toContain('WALDO');
  });

  it('forward of a message WITH attachments adds a visible omitted-attachments note', () => {
    const body = buildReplyDraft(
      parsed({ attachments: [{ filename: 'ics213.txt', size: 100 }, { filename: 'map.jpg', size: 2000 }] }),
      'forward',
    ).body;
    expect(body).toMatch(/attachment/i);
    expect(body).toContain('ics213.txt');
    expect(body).toContain('map.jpg');
  });

  it('forward of a message WITHOUT attachments adds no omitted-attachments note', () => {
    const body = buildReplyDraft(parsed({ attachments: [] }), 'forward').body;
    expect(body).not.toMatch(/not carried/i);
  });

  it('reply does not add an attachment note (replies do not re-send attachments)', () => {
    const body = buildReplyDraft(parsed({ attachments: [{ filename: 'x.txt', size: 1 }] }), 'reply').body;
    expect(body).not.toMatch(/not carried/i);
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

describe("buildReplyDraft 'replyWithForm' mode", () => {
  const FORM_PAYLOAD = {
    formId: 'ICS213_Initial',
    formParameters: {
      xmlFileVersion: '1.0',
      rmsExpressVersion: 'Tuxlink/0.3.0',
      submissionDatetime: '20260530143000',
      sendersCallsign: 'N5VSU',
      gridSquare: 'EM15',
      displayForm: 'ICS213_Initial_Viewer.html',
      replyTemplate: 'ICS213_SendReply.0',
    },
    fields: [
      ['inc_name', 'WALDO'],
      ['to_name', 'James / WX4MTL'],
      ['fm_name', 'Maria / N5VSU'],
      ['subjectline', 'REQUEST SUPPLIES'],
      ['mdate', '2026-05-30'],
      ['mtime', '14:30Z'],
      ['message', 'Need bandages.'],
      ['isexercise', ''],
    ] as [string, string][],
  };

  it('seeds the draft with the same formId for round-trip form composition', () => {
    const msg = parsed({ isForm: true, formId: 'ICS213_Initial', formPayload: FORM_PAYLOAD });
    const draft = buildReplyDraft(msg, 'replyWithForm');
    expect(draft.formId).toBe('ICS213_Initial');
  });

  it('swaps fm_name into to_name and preserves inc_name + subjectline (with Re: prefix)', () => {
    const msg = parsed({
      isForm: true,
      formId: 'ICS213_Initial',
      formPayload: FORM_PAYLOAD,
      subject: 'ICS-213 REQUEST',
    });
    const draft = buildReplyDraft(msg, 'replyWithForm');
    expect(draft.formFields?.to_name).toBe('Maria / N5VSU');
    expect(draft.formFields?.inc_name).toBe('WALDO');
    expect(draft.formFields?.subjectline).toBe('Re: REQUEST SUPPLIES');
  });

  it('does not pre-populate message / approved_name fields (response-specific)', () => {
    const msg = parsed({ isForm: true, formId: 'ICS213_Initial', formPayload: FORM_PAYLOAD });
    const draft = buildReplyDraft(msg, 'replyWithForm');
    expect(draft.formFields?.message).toBeUndefined();
    expect(draft.formFields?.approved_name).toBeUndefined();
  });

  it("falls back to a plain reply when the source isn't a parseable form", () => {
    const msg = parsed({ isForm: false });
    const draft = buildReplyDraft(msg, 'replyWithForm');
    // No formId set → plain reply path
    expect(draft.formId).toBeUndefined();
    expect(draft.formFields).toBeUndefined();
  });

  // Codex r2 P2 #1: forms WITHOUT explicit field-mapping logic
  // (Bulletin/Position/ICS-309/Damage Assessment) fall back to plain reply
  // rather than producing a half-populated form draft.
  it('falls back to a plain reply for non-ICS-213 forms (no per-form mapping)', () => {
    const bulletinPayload = {
      formId: 'Bulletin_Initial',
      formParameters: {
        xmlFileVersion: '1.0', rmsExpressVersion: 'Tuxlink/0.3.0',
        submissionDatetime: '', sendersCallsign: '', gridSquare: '',
        displayForm: 'Bulletin Viewer.html', replyTemplate: '',
      },
      fields: [['title', 'Test'], ['message', 'Body text']] as [string, string][],
    };
    const msg = parsed({ isForm: true, formId: 'Bulletin_Initial', formPayload: bulletinPayload });
    const draft = buildReplyDraft(msg, 'replyWithForm');
    // Plain-reply fallback: no formId/formFields on the draft.
    expect(draft.formId).toBeUndefined();
    expect(draft.formFields).toBeUndefined();
  });
});

// Codex r2 P2 #1 helper — gates the MessageView "Reply with form…" button.
describe('hasReplyWithFormSupport', () => {
  it('returns true for ICS213_Initial (the only currently mapped form)', () => {
    expect(hasReplyWithFormSupport('ICS213_Initial')).toBe(true);
  });

  it('returns false for Phase 9 forms (Bulletin / Position / ICS-309 / DA)', () => {
    expect(hasReplyWithFormSupport('Bulletin_Initial')).toBe(false);
    expect(hasReplyWithFormSupport('Position_Report')).toBe(false);
    expect(hasReplyWithFormSupport('Form-309_Initial')).toBe(false);
    expect(hasReplyWithFormSupport('Damage_Assessment_Initial')).toBe(false);
  });

  it('returns false for null / undefined / unknown formIds', () => {
    expect(hasReplyWithFormSupport(null)).toBe(false);
    expect(hasReplyWithFormSupport(undefined)).toBe(false);
    expect(hasReplyWithFormSupport('Made_Up_Form_v999')).toBe(false);
  });
});
