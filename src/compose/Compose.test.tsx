// Compose — focused unit tests for the pieces extracted from the
// component scope. The full <Compose /> mount-and-interact tests live
// in the PR's manual smoke (Tauri-runtime-dependent: invoke,
// onCloseRequested, getCurrentWindow), not here. This suite covers
// pure helpers: the ParsedBody → fieldValues conversion that
// handleWebviewSubmit uses to feed `send_webview_form`.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 10.

import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Contact, Group } from '../contacts/types';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  win: {
    onCloseRequested: vi.fn(),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
  },
  // Mutable contacts/groups the mocked useContacts returns. Tests assign these
  // before render so send-time group expansion has fixtures to resolve against.
  contacts: [] as Contact[],
  groups: [] as Group[],
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => mocks.win }));
// Defensive global stubs. Compose's close-handler effect does a dynamic
// `import('@tauri-apps/api/window')` inside an async `.then()`; when a mounted
// <Compose> outlives a fast test (the A6 send tests await an invoke, then
// cleanup unmounts), the late promise + unlisten can momentarily resolve
// against the REAL Tauri module during teardown, which reads these globals.
// Stubbing them makes that path a harmless no-op instead of an
// unhandled-rejection that pollutes the run (mirrors App.test.tsx, which also
// mounts <Compose>). The in-test path still uses the mocked getCurrentWindow.
const g = globalThis as unknown as Record<string, unknown>;
g.__TAURI_INTERNALS__ = {
  metadata: { currentWindow: { label: 'compose-test' }, currentWebview: { label: 'compose-test' } },
  transformCallback: (cb: unknown) => cb,
  invoke: async () => undefined,
};
g.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: async () => undefined,
};
// Compose builds its own useContacts() instance (separate window). Mock it so
// the send-time group expansion sees fixture contacts/groups without a Tauri
// runtime or a QueryClientProvider wrapper.
vi.mock('../contacts/useContacts', () => ({
  useContacts: () => ({
    contacts: mocks.contacts,
    groups: mocks.groups,
    isLoading: false,
    upsertContact: vi.fn(),
    deleteContact: vi.fn(),
    upsertGroup: vi.fn(),
    deleteGroup: vi.fn(),
  }),
}));

import {
  Compose,
  closePromptShape,
  isSaveDraftAvailable,
  parsedBodyToFieldValues,
  persistedFormDraft,
} from './Compose';

const DEFAULT_INVOKE = async (cmd: string) => {
  if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
  return null;
};

beforeEach(() => {
  localStorage.clear();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  mocks.win.onCloseRequested.mockReset();
  mocks.win.onCloseRequested.mockResolvedValue(vi.fn());
  mocks.win.minimize.mockClear();
  mocks.win.toggleMaximize.mockClear();
  mocks.contacts = [];
  mocks.groups = [];
});

afterEach(() => {
  cleanup();
});

describe('<Compose> sender identity', () => {
  it('shows the configured callsign in the read-only From field', async () => {
    render(<Compose draftId="from-identity-test" />);
    const from = screen.getByLabelText(/^From$/i) as HTMLInputElement;

    await waitFor(() => expect(from).toHaveValue('N0CALL'));
    expect(from).toBeDisabled();
    expect(screen.getByText(/Multi-callsign.*coming soon/i)).toBeInTheDocument();
  });

  it('falls back to the configured identifier for offline-path installs', async () => {
    // Offline-path operators have no callsign (config.rs forbids it on that
    // path) — their station identity lives in `identifier`. The From field must
    // surface it instead of rendering blank (smoke-walk item 39 gap).
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read')
        return { connect_to_cms: false, callsign: null, identifier: 'FIELD-1', grid: 'CN87' };
      return null;
    });
    render(<Compose draftId="from-identity-offline-test" />);
    const from = screen.getByLabelText(/^From$/i) as HTMLInputElement;

    await waitFor(() => expect(from).toHaveValue('FIELD-1'));
    expect(from).toBeDisabled();
  });
});

describe('parsedBodyToFieldValues', () => {
  it('collapses single-value fields to bare strings', () => {
    const out = parsedBodyToFieldValues({
      fields: {
        callsign: ['W6ABC'],
        subject: ['Test'],
      },
      submitter: null,
    });
    expect(out).toEqual({ callsign: 'W6ABC', subject: 'Test' });
  });

  it('joins multi-value fields with newlines', () => {
    // WLE forms use repeated names + checkbox groups; collapsing
    // multi-values via newline preserves the convention forms::parse
    // expects.
    const out = parsedBodyToFieldValues({
      fields: {
        checked_items: ['food', 'water', 'shelter'],
      },
      submitter: null,
    });
    expect(out.checked_items).toBe('food\nwater\nshelter');
  });

  it("strips the synthetic 'Submit' button name", () => {
    // WLE templates POST the submit button's value back as a field
    // named 'Submit'. The backend serializer would just emit it as a
    // <Submit> element in the XML, but it's clearer to strip it at the
    // boundary so the wire format doesn't carry an obviously-meaningless
    // pseudo-field.
    const out = parsedBodyToFieldValues({
      fields: {
        Submit: ['Send'],
        callsign: ['W6ABC'],
      },
      submitter: 'Submit',
    });
    expect(out).not.toHaveProperty('Submit');
    expect(out).toHaveProperty('callsign', 'W6ABC');
  });

  it('returns an empty object for an empty ParsedBody', () => {
    expect(parsedBodyToFieldValues({ fields: {}, submitter: null })).toEqual({});
  });

  it('preserves field order from Object.entries (insertion order for plain objects)', () => {
    // Stability isn't strictly required by the serializer (XML key order
    // is sorted alphabetically inside serialize_catalog_form_xml), but
    // we want consistent test output for the snapshot expectations.
    const out = parsedBodyToFieldValues({
      fields: {
        bravo: ['B'],
        alpha: ['A'],
      },
      submitter: null,
    });
    expect(Object.keys(out)).toEqual(['bravo', 'alpha']);
  });
});

// ============================================================================
// P1.1 (2026-06-04 Codex adrev): Save Draft must NOT silently lose webview
// form contents. closePromptShape + isSaveDraftAvailable encode the dialog
// + toolbar conditions; the rendering side reads from these helpers.
// ============================================================================

describe('isSaveDraftAvailable', () => {
  it('is true for plain, pick, and form modes', () => {
    expect(isSaveDraftAvailable('plain')).toBe(true);
    expect(isSaveDraftAvailable('pick')).toBe(true);
    expect(isSaveDraftAvailable('form')).toBe(true);
  });

  it('is false for webview-form mode (Codex adrev P1.1)', () => {
    // In webview-form mode the field values live inside the embedded
    // child webview; Compose has no IPC introspection into them. Save
    // Draft would persist only the formId metadata while silently
    // losing every typed field value — the exact UX trap Codex
    // flagged. Hide the affordance entirely.
    expect(isSaveDraftAvailable('webview-form')).toBe(false);
  });
});

describe('closePromptShape', () => {
  it('returns the Save / Discard / Cancel triad for plain mode', () => {
    const shape = closePromptShape('plain', 'close');
    expect(shape.primary).toBe('This draft has unsaved changes.');
    expect(shape.sub).toBeUndefined();
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('returns the switch-to-form variant when transitioning from plain to form picker', () => {
    const shape = closePromptShape('plain', 'switch-to-form');
    expect(shape.primary).toBe('Save changes before switching to a form?');
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('returns the Save / Discard / Cancel triad for native form mode', () => {
    // Native React forms own their field values via setFormMode; Save
    // Draft can capture them. The full triad applies.
    const shape = closePromptShape('form', 'close');
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('omits Save and surfaces an explainer in webview-form mode (Codex adrev P1.1)', () => {
    // The key regression test for P1.1: in webview-form mode the
    // close-dialog must NOT offer Save Draft, must explain why, and
    // must offer Discard + Cancel only. The operator can Cancel back
    // to the form and press its Send button — that's the only path
    // that preserves the form contents.
    const shape = closePromptShape('webview-form', 'close');
    expect(shape.buttons).toEqual(['discard', 'cancel']);
    expect(shape.buttons).not.toContain('save');
    expect(shape.primary).toMatch(/can't be saved as a draft/i);
    expect(shape.sub).toMatch(/embedded form window/i);
    expect(shape.sub).toMatch(/Cancel.*Send button/i);
  });

  it('webview-form mode ignores the action — same shape for close + switch-to-form', () => {
    const closeShape = closePromptShape('webview-form', 'close');
    const switchShape = closePromptShape('webview-form', 'switch-to-form');
    expect(closeShape).toEqual(switchShape);
  });
});

// ============================================================================
// Task A6 — send-path group expansion (CORRECTNESS-CRITICAL: recipients on the
// wire). These mount the real <Compose> with a mocked invoke + useContacts so
// we can assert the EXACT message_send payload.
// ============================================================================

const ts = '2026-06-07T00:00:00Z';
const mkContact = (id: string, callsign: string): Contact => ({
  id,
  name: callsign,
  callsign,
  created_at: ts,
  updated_at: ts,
});

/** Seed a localStorage draft so Compose restores `to`/`cc` on mount. */
function seedDraft(draftId: string, to: string, cc = ''): void {
  localStorage.setItem(
    `tuxlink.drafts.${draftId}`,
    JSON.stringify({ draftId, to, cc, subject: 'S', body: 'B', requestAck: false, savedAt: ts }),
  );
}

/** Pull the `message_send` draft DTO out of the invoke mock's calls. */
function lastMessageSendDraft(): { to: string[]; cc: string[] } | undefined {
  const call = mocks.invoke.mock.calls.find(([cmd]) => cmd === 'message_send');
  return call?.[1]?.draft;
}

describe('<Compose> send-path group expansion (Task A6)', () => {
  it('expands a group:<id> in To to member callsigns — NO group: token reaches message_send (H5)', async () => {
    mocks.contacts = [mkContact('c-w6abc', 'W6ABC'), mkContact('c-w7def', 'W7DEF')];
    mocks.groups = [
      {
        id: 'g-ares',
        name: 'ARES',
        members: [
          { type: 'contact', contact_id: 'c-w6abc' },
          { type: 'contact', contact_id: 'c-w7def' },
          { type: 'raw', callsign: 'W9XYZ' },
        ],
        created_at: ts,
        updated_at: ts,
      },
    ];
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'message_send') return 'MID-1';
      return null;
    });
    seedDraft('a6-h5', 'group:g-ares');

    render(<Compose draftId="a6-h5" />);
    // Wait for the draft restore to populate the To chips.
    await screen.findByTestId('recipient-chip-group:g-ares');

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    await waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(true),
    );
    const draft = lastMessageSendDraft();
    expect(draft?.to).toEqual(['W6ABC', 'W7DEF', 'W9XYZ']);
    // H5 — the sentinel must NOT survive to the wire.
    expect(draft?.to.some((t) => t.startsWith('group:'))).toBe(false);
  });

  it('dedups To against the @winlink.org email form on the wire (H6)', async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'message_send') return 'MID-1';
      return null;
    });
    seedDraft('a6-dedup', 'W6ABC; w6abc@winlink.org; W6ABC-7');

    render(<Compose draftId="a6-dedup" />);
    await screen.findByTestId('recipient-chip-W6ABC');

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    await waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(true),
    );
    const draft = lastMessageSendDraft();
    // W6ABC + w6abc@winlink.org collapse to one; W6ABC-7 is a distinct SSID.
    expect(draft?.to).toEqual(['W6ABC', 'W6ABC-7']);
  });

  it('seeds Cc from the expanded To so a shared recipient is not double-sent (Codex#6)', async () => {
    mocks.contacts = [mkContact('c-w6abc', 'W6ABC')];
    mocks.groups = [
      {
        id: 'g-one',
        name: 'One',
        members: [{ type: 'contact', contact_id: 'c-w6abc' }],
        created_at: ts,
        updated_at: ts,
      },
    ];
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'message_send') return 'MID-1';
      return null;
    });
    // To = group expanding to W6ABC; Cc = W6ABC (email form) + a fresh callsign.
    seedDraft('a6-cc', 'group:g-one', 'w6abc@winlink.org; W7DEF');

    render(<Compose draftId="a6-cc" />);
    await screen.findByTestId('recipient-chip-group:g-one');

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    await waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(true),
    );
    const draft = lastMessageSendDraft();
    expect(draft?.to).toEqual(['W6ABC']);
    expect(draft?.cc).toEqual(['W7DEF']); // W6ABC dropped — already in To
  });

  it('BLOCKS send and shows compose-error when a group in To was deleted (unknown group token)', async () => {
    // The group is NOT in mocks.groups — simulates deletion mid-compose.
    mocks.contacts = [];
    mocks.groups = [];
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      // message_send must NOT be called — this line would fail the test if hit.
      if (cmd === 'message_send') return 'MID-SHOULD-NOT-HAPPEN';
      return null;
    });
    seedDraft('a6-block-deleted-group', 'group:g-deleted-uuid');

    render(<Compose draftId="a6-block-deleted-group" />);
    await screen.findByTestId('recipient-chip-group:g-deleted-uuid');

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    // The error banner must appear with the block message.
    await waitFor(() =>
      expect(screen.getByTestId('compose-error')).toBeInTheDocument(),
    );
    expect(screen.getByTestId('compose-error')).toHaveTextContent(
      'A distribution group in your recipients no longer exists. Remove the group and re-add its members before sending.',
    );

    // message_send must NOT have been called.
    expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(false);
  });

  it('sends normally when a group in To IS known (existing A6 behavior stays green)', async () => {
    mocks.contacts = [mkContact('c-w6abc', 'W6ABC')];
    mocks.groups = [
      {
        id: 'g-known',
        name: 'Known',
        members: [{ type: 'contact', contact_id: 'c-w6abc' }],
        created_at: ts,
        updated_at: ts,
      },
    ];
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'message_send') return 'MID-OK';
      return null;
    });
    seedDraft('a6-known-group-sends', 'group:g-known');

    render(<Compose draftId="a6-known-group-sends" />);
    await screen.findByTestId('recipient-chip-group:g-known');

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    await waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(true),
    );
    const draft = lastMessageSendDraft();
    expect(draft?.to).toEqual(['W6ABC']);
    // No error banner.
    expect(screen.queryByTestId('compose-error')).not.toBeInTheDocument();
  });

  it('does NOT expand groups on autosave — saved draft keeps the group: sentinel', async () => {
    vi.useFakeTimers();
    try {
      mocks.contacts = [mkContact('c-w6abc', 'W6ABC')];
      mocks.groups = [
        {
          id: 'g-keep',
          name: 'Keep',
          members: [{ type: 'contact', contact_id: 'c-w6abc' }],
          created_at: ts,
          updated_at: ts,
        },
      ];
      mocks.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
        return null;
      });
      seedDraft('a6-autosave', 'group:g-keep');

      render(<Compose draftId="a6-autosave" />);
      // Let the restore-on-mount effect run, then fire the 2s autosave tick.
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(2000);

      const saved = JSON.parse(localStorage.getItem('tuxlink.drafts.a6-autosave')!);
      // Autosave persists the RAW sentinel string, never the expanded members.
      expect(saved.to).toBe('group:g-keep');
      expect(saved.to).not.toContain('W6ABC');
    } finally {
      vi.useRealTimers();
    }
  });

  // ============================================================================
  // tuxlink-n3hw — autosave must not re-stamp savedAt on an unedited open
  // ============================================================================
  //
  // savedAt feeds draftToMessageMeta's `date`, which drives the Drafts-list
  // sort. The 2s autosave previously called saveDraft UNCONDITIONALLY, so just
  // opening a draft for reading re-stamped savedAt within 2s and bumped the
  // draft to the top of the list. Recency must track EDITS, not reads.

  it('does NOT re-stamp savedAt when a draft is opened for reading without edits (tuxlink-n3hw)', async () => {
    vi.useFakeTimers();
    try {
      mocks.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
        return null;
      });
      seedDraft('n3hw-read', 'W6ABC');

      render(<Compose draftId="n3hw-read" />);
      // Restore-on-mount, then fire several 2s autosave ticks WITHOUT editing.
      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(6000);

      const saved = JSON.parse(localStorage.getItem('tuxlink.drafts.n3hw-read')!);
      // Opening for reading must not touch recency — savedAt stays seeded.
      expect(saved.savedAt).toBe(ts);
    } finally {
      vi.useRealTimers();
    }
  });

  it('re-stamps savedAt once the draft is actually edited (tuxlink-n3hw)', async () => {
    vi.useFakeTimers();
    try {
      mocks.invoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
        return null;
      });
      seedDraft('n3hw-edit', 'W6ABC');

      render(<Compose draftId="n3hw-edit" />);
      await vi.advanceTimersByTimeAsync(0);

      // A genuine edit must persist AND bump recency.
      const subject = screen.getByTestId('compose-subject') as HTMLInputElement;
      fireEvent.change(subject, { target: { value: 'Edited subject' } });
      await vi.advanceTimersByTimeAsync(2000);

      const saved = JSON.parse(localStorage.getItem('tuxlink.drafts.n3hw-edit')!);
      expect(saved.subject).toBe('Edited subject');
      expect(saved.savedAt).not.toBe(ts);
    } finally {
      vi.useRealTimers();
    }
  });

  // ============================================================================
  // C2-P1 regression — fresh contacts_read at send (Codex#5 proper fix)
  // ============================================================================
  //
  // The stale-group-expansion race: the cached `useContacts` hook value can lag
  // a `contacts:changed` refetch that was triggered by a main-window edit AFTER
  // the Compose window mounted. buildRecipients must perform a LIVE contacts_read
  // at send so expansion always uses the most-recent membership — not the value
  // frozen into the hook at mount time.
  //
  // Arrange: cached hook state (set A) vs fresh contacts_read response (set B).
  // Assert: the message is sent with set B, proving the send path re-reads.
  it('C2-P1: uses the fresh contacts_read result, NOT the stale cached hook value, when expanding a group at send', async () => {
    // Set A — stale cache: group g-stale contains only W6ABC (mounted state)
    const staleContact = mkContact('c-w6abc', 'W6ABC');
    mocks.contacts = [staleContact];
    mocks.groups = [
      {
        id: 'g-stale',
        name: 'Stale',
        members: [{ type: 'contact', contact_id: 'c-w6abc' }],
        created_at: ts,
        updated_at: ts,
      },
    ];

    // Set B — fresh contacts_read: same group now also includes W7DEF (simulating
    // a main-window edit that invalidated the query but whose refetch is still
    // in-flight inside the hook's cache).
    const freshMember = mkContact('c-w7def', 'W7DEF');
    const freshContactsFile = {
      schema_version: 1,
      contacts: [staleContact, freshMember],
      groups: [
        {
          id: 'g-stale',
          name: 'Stale',
          members: [
            { type: 'contact' as const, contact_id: 'c-w6abc' },
            { type: 'contact' as const, contact_id: 'c-w7def' },
          ],
          created_at: ts,
          updated_at: ts,
        },
      ],
    };

    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return { callsign: 'N0CALL', grid: 'CN87' };
      if (cmd === 'contacts_read') return freshContactsFile;
      if (cmd === 'message_send') return 'MID-C2P1';
      return null;
    });
    seedDraft('c2-p1-fresh-fetch', 'group:g-stale');

    render(<Compose draftId="c2-p1-fresh-fetch" />);
    await screen.findByTestId('recipient-chip-group:g-stale');

    const contactsReadBefore = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'contacts_read').length;

    fireEvent.click(screen.getByTestId('compose-send-btn'));

    await waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([cmd]) => cmd === 'message_send')).toBe(true),
    );

    // contacts_read must have been called at least once during the send flow
    // (call count must have increased from the pre-send baseline).
    const contactsReadAfter = mocks.invoke.mock.calls.filter(([cmd]) => cmd === 'contacts_read').length;
    expect(contactsReadAfter).toBeGreaterThan(contactsReadBefore);

    // The wire payload must reflect set B (fresh), NOT set A (stale cache).
    const draft = lastMessageSendDraft();
    // Fresh contacts_read gives g-stale two members: W6ABC + W7DEF.
    expect(draft?.to).toEqual(['W6ABC', 'W7DEF']);
    // W7DEF must be present — it was NOT in the stale cached hook state.
    expect(draft?.to).toContain('W7DEF');
  });
});

// ── tuxlink-hhfx / G10 — persistedFormDraft (shared draft-field derivation) ──
describe('persistedFormDraft', () => {
  it('native form persists formId + live values', () => {
    expect(persistedFormDraft({ kind: 'form', formId: 'ICS213_Initial', values: { a: '1' } })).toEqual({
      formId: 'ICS213_Initial',
      formFields: { a: '1' },
    });
  });

  it('webview-form persists only the formId (values live in the webview)', () => {
    expect(persistedFormDraft({ kind: 'webview-form', formId: 'USGS_DYFI' })).toEqual({
      formId: 'USGS_DYFI',
    });
  });

  it('webview-reply persists the original values + reply markers', () => {
    expect(
      persistedFormDraft({
        kind: 'webview-reply',
        formId: 'ICS213_Initial',
        values: { Message: 'orig' },
        msgOriginalBody: 'body',
      }),
    ).toEqual({
      formId: 'ICS213_Initial',
      formFields: { Message: 'orig' },
      formReply: true,
      msgOriginalBody: 'body',
    });
  });

  it('plain / pick modes persist no form fields', () => {
    expect(persistedFormDraft({ kind: 'plain' })).toEqual({});
    expect(persistedFormDraft({ kind: 'pick' })).toEqual({});
  });
});

describe('isSaveDraftAvailable — webview-reply', () => {
  it('is unavailable for webview-reply (edits live in the embedded webview)', () => {
    expect(isSaveDraftAvailable('webview-reply')).toBe(false);
  });
});
