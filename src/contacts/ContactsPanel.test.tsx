// ContactsPanel + ContactEditor tests (Task A8).
//
// Covers the inline list/detail management surface: Groups-above-People list,
// search filtering, detail pane (name/callsign/email/tactical/notes + actions),
// the suggest-from-history "+ Add" cards, the New-message → Compose-To route,
// and the ContactEditor New/Edit form. The M8 (no MessageList) + Codex#11
// (no mailbox_list for 'contacts') invariants are asserted at the App-level in
// AppShell.test.tsx (A9); here we assert ContactsPanel renders standalone.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

// react-virtuoso renders into a zero-height scroller under jsdom (M10); replace
// it with a plain map so the People rows are actually in the DOM.
vi.mock('react-virtuoso', () => ({
  Virtuoso: ({
    data,
    itemContent,
  }: {
    data: unknown[];
    itemContent: (i: number, m: unknown) => unknown;
  }) => (
    <div data-testid="virtuoso-mock">
      {data.map((m, i) => (
        <div key={i}>{itemContent(i, m) as ReactNode}</div>
      ))}
    </div>
  ),
}));

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

import { ContactsPanel } from './ContactsPanel';
import type { Contact, Group, Suggestion } from './types';

const NOW = '2026-06-07T00:00:00Z';

const ALICE: Contact = {
  id: 'c-alice',
  name: 'Alice Operator',
  callsign: 'W6ABC',
  email: 'w6abc@winlink.org',
  tactical: 'NET-CONTROL',
  notes: 'Primary net control.',
  created_at: NOW,
  updated_at: NOW,
};
const BOB: Contact = {
  id: 'c-bob',
  name: 'Bob Relay',
  callsign: 'KE7XYZ',
  created_at: NOW,
  updated_at: NOW,
};
const TEAM: Group = {
  id: 'g-team',
  name: 'Team Alpha',
  members: [{ type: 'contact', contact_id: 'c-alice' }],
  created_at: NOW,
  updated_at: NOW,
};

function routeInvoke(opts: {
  contacts?: Contact[];
  groups?: Group[];
  suggestions?: Suggestion[];
}) {
  const contacts = opts.contacts ?? [];
  const groups = opts.groups ?? [];
  const suggestions = opts.suggestions ?? [];
  vi.mocked(invoke).mockImplementation((async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === 'contacts_read') return { schema_version: 1, contacts, groups };
    if (cmd === 'contacts_suggestions') return suggestions;
    if (cmd === 'contact_upsert') return args?.contact as Contact;
    if (cmd === 'group_upsert') return args?.group as Group;
    if (cmd === 'group_delete') return undefined;
    return undefined;
  }) as typeof invoke);
}

function renderPanel() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ContactsPanel />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  globalThis.localStorage?.clear?.();
  vi.mocked(invoke).mockReset();
});

describe('<ContactsPanel> — list', () => {
  it('renders the Groups section above the People section', async () => {
    routeInvoke({ contacts: [ALICE, BOB], groups: [TEAM] });
    renderPanel();

    const groupsHeading = await screen.findByTestId('contacts-groups-heading');
    const peopleHeading = await screen.findByTestId('contacts-people-heading');

    // Both present, and Groups precedes People in document order (A.4).
    expect(groupsHeading).toBeInTheDocument();
    expect(peopleHeading).toBeInTheDocument();
    expect(
      groupsHeading.compareDocumentPosition(peopleHeading) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    expect(screen.getByText('Team Alpha')).toBeInTheDocument();
    expect(screen.getByText('Alice Operator')).toBeInTheDocument();
    expect(screen.getByText('Bob Relay')).toBeInTheDocument();
  });

  it('filters both groups and people by the search input', async () => {
    routeInvoke({ contacts: [ALICE, BOB], groups: [TEAM] });
    renderPanel();
    await screen.findByText('Alice Operator');

    const search = screen.getByTestId('contacts-search');
    fireEvent.change(search, { target: { value: 'ke7' } });

    expect(screen.getByText('Bob Relay')).toBeInTheDocument();
    expect(screen.queryByText('Alice Operator')).not.toBeInTheDocument();
    // The group (Team Alpha) doesn't match 'ke7' → hidden.
    expect(screen.queryByText('Team Alpha')).not.toBeInTheDocument();
  });

  it('matches a contact by callsign as well as by name', async () => {
    routeInvoke({ contacts: [ALICE, BOB] });
    renderPanel();
    await screen.findByText('Alice Operator');

    fireEvent.change(screen.getByTestId('contacts-search'), { target: { value: 'w6abc' } });
    expect(screen.getByText('Alice Operator')).toBeInTheDocument();
    expect(screen.queryByText('Bob Relay')).not.toBeInTheDocument();
  });
});

describe('<ContactsPanel> — detail pane', () => {
  it('shows the selected contact detail with all multi-address fields + actions', async () => {
    routeInvoke({ contacts: [ALICE] });
    renderPanel();

    fireEvent.click(await screen.findByText('Alice Operator'));

    const detail = await screen.findByTestId('contact-detail');
    expect(within(detail).getByText('Alice Operator')).toBeInTheDocument();
    expect(within(detail).getByText('W6ABC')).toBeInTheDocument();
    expect(within(detail).getByText('w6abc@winlink.org')).toBeInTheDocument();
    expect(within(detail).getByText('NET-CONTROL')).toBeInTheDocument();
    expect(within(detail).getByText('Primary net control.')).toBeInTheDocument();
    expect(within(detail).getByTestId('contact-new-message')).toBeInTheDocument();
    expect(within(detail).getByTestId('contact-edit')).toBeInTheDocument();
  });

  it('New message routes the primary callsign into a Compose To draft + opens the window', async () => {
    routeInvoke({ contacts: [ALICE] });
    renderPanel();
    fireEvent.click(await screen.findByText('Alice Operator'));

    fireEvent.click(await screen.findByTestId('contact-new-message'));

    await waitFor(() => {
      const call = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === 'compose_window_open');
      expect(call).toBeTruthy();
    });
    // The seeded draft carries the primary callsign in To.
    const ids = JSON.parse(globalThis.localStorage!.getItem('tuxlink.drafts.index') ?? '[]');
    expect(ids.length).toBeGreaterThan(0);
    const draft = JSON.parse(globalThis.localStorage!.getItem(`tuxlink.drafts.${ids[0]}`)!);
    expect(draft.to).toContain('W6ABC');
  });
});

describe('<ContactsPanel> — suggestions', () => {
  it('lists "+ Add" cards from contacts_suggestions annotated with the message count', async () => {
    routeInvoke({
      contacts: [],
      suggestions: [{ callsign: 'KE7XYZ', message_count: 4 }],
    });
    renderPanel();

    const card = await screen.findByTestId('suggestion-KE7XYZ');
    expect(within(card).getByText('KE7XYZ')).toBeInTheDocument();
    // count annotation: "exchanged 4 messages with KE7XYZ"
    expect(within(card).getByText(/4 messages/)).toBeInTheDocument();
  });

  it('"+ Add" calls contact_upsert with the suggested callsign prefilled', async () => {
    routeInvoke({
      contacts: [],
      suggestions: [{ callsign: 'KE7XYZ', message_count: 4 }],
    });
    renderPanel();

    const card = await screen.findByTestId('suggestion-KE7XYZ');
    fireEvent.click(within(card).getByTestId('suggestion-add-KE7XYZ'));

    await waitFor(() => {
      const call = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === 'contact_upsert');
      expect(call).toBeTruthy();
      const contact = (call?.[1] as { contact: Contact }).contact;
      expect(contact.callsign).toBe('KE7XYZ');
    });
  });

  it('removes the "+ Add" card after adding (re-derived suggestions exclude it) — no duplicate create', async () => {
    // Simulate the backend `derive_suggestions` re-derivation: the suggestion is
    // present on the FIRST `contacts_suggestions` call, then excluded (the
    // contact now exists) on every subsequent call. The card must invalidate +
    // refetch away after the "+ Add" click, so a second click is impossible.
    let suggestionsCalls = 0;
    vi.mocked(invoke).mockImplementation((async (
      cmd: string,
      args?: Record<string, unknown>,
    ) => {
      if (cmd === 'contacts_read') return { schema_version: 1, contacts: [], groups: [] };
      if (cmd === 'contacts_suggestions') {
        suggestionsCalls += 1;
        return suggestionsCalls === 1 ? [{ callsign: 'KE7XYZ', message_count: 4 }] : [];
      }
      if (cmd === 'contact_upsert') return args?.contact as Contact;
      return undefined;
    }) as typeof invoke);

    renderPanel();

    const card = await screen.findByTestId('suggestion-KE7XYZ');
    fireEvent.click(within(card).getByTestId('suggestion-add-KE7XYZ'));

    // After the upsert + suggestions invalidation settles, the card is gone.
    await waitFor(() => {
      expect(screen.queryByTestId('suggestion-KE7XYZ')).not.toBeInTheDocument();
    });
    // The whole Suggested section collapses once there are no suggestions left.
    expect(screen.queryByTestId('contacts-suggested')).not.toBeInTheDocument();

    // contact_upsert fired exactly once — no duplicate create from a stale card.
    const upsertCalls = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === 'contact_upsert');
    expect(upsertCalls).toHaveLength(1);
  });
});

describe('<ContactsPanel> — editor', () => {
  it('+ New opens the editor; saving an entered callsign calls contact_upsert', async () => {
    routeInvoke({ contacts: [] });
    renderPanel();

    fireEvent.click(await screen.findByTestId('contacts-new'));
    const editor = await screen.findByTestId('contact-editor');

    fireEvent.change(within(editor).getByTestId('editor-callsign'), {
      target: { value: 'N0CALL' },
    });
    fireEvent.change(within(editor).getByTestId('editor-name'), {
      target: { value: 'New Person' },
    });
    fireEvent.click(within(editor).getByTestId('editor-save'));

    await waitFor(() => {
      const call = vi
        .mocked(invoke)
        .mock.calls.find(([cmd]) => cmd === 'contact_upsert');
      expect(call).toBeTruthy();
      const contact = (call?.[1] as { contact: Contact }).contact;
      expect(contact.callsign).toBe('N0CALL');
      expect(contact.name).toBe('New Person');
    });
  });

  it('Save is disabled until a callsign is entered (callsign required)', async () => {
    routeInvoke({ contacts: [] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('contacts-new'));
    const editor = await screen.findByTestId('contact-editor');
    expect(within(editor).getByTestId('editor-save')).toBeDisabled();
  });

  it('Edit opens the editor prefilled with the existing contact fields', async () => {
    routeInvoke({ contacts: [ALICE] });
    renderPanel();
    fireEvent.click(await screen.findByText('Alice Operator'));
    fireEvent.click(await screen.findByTestId('contact-edit'));

    const editor = await screen.findByTestId('contact-editor');
    expect((within(editor).getByTestId('editor-callsign') as HTMLInputElement).value).toBe('W6ABC');
    expect((within(editor).getByTestId('editor-name') as HTMLInputElement).value).toBe('Alice Operator');
    expect((within(editor).getByTestId('editor-email') as HTMLInputElement).value).toBe('w6abc@winlink.org');
  });
});

describe('<ContactsPanel> — group editor (A8b)', () => {
  it('+ New group opens an empty GroupEditor; saving routes through group_upsert', async () => {
    routeInvoke({ contacts: [ALICE], groups: [] });
    renderPanel();

    fireEvent.click(await screen.findByTestId('contacts-new-group'));
    const editor = await screen.findByTestId('group-editor');
    // Empty (new) group: blank name, Delete absent.
    expect((within(editor).getByTestId('group-editor-name') as HTMLInputElement).value).toBe('');
    expect(within(editor).queryByTestId('group-editor-delete')).not.toBeInTheDocument();

    fireEvent.change(within(editor).getByTestId('group-editor-name'), {
      target: { value: 'Field Day Crew' },
    });
    fireEvent.click(within(editor).getByTestId('group-editor-save'));

    await waitFor(() => {
      const call = vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === 'group_upsert');
      expect(call).toBeTruthy();
      const group = (call?.[1] as { group: Group }).group;
      expect(group.name).toBe('Field Day Crew');
      expect(group.id).toBe(''); // new → backend stamps
    });
  });

  it('clicking an existing group opens the GroupEditor prefilled (name + members)', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();

    fireEvent.click(await screen.findByTestId('group-row-g-team'));
    const editor = await screen.findByTestId('group-editor');
    expect((within(editor).getByTestId('group-editor-name') as HTMLInputElement).value).toBe(
      'Team Alpha',
    );
    // Edit mode → Delete present.
    expect(within(editor).getByTestId('group-editor-delete')).toBeInTheDocument();
    // The contact member resolves to Alice's display form (not silently absent).
    const list = within(editor).getByTestId('group-member-list');
    expect(within(list).getByText(/Alice Operator/)).toBeInTheDocument();
  });

  it('Delete in the GroupEditor routes through group_delete', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();

    fireEvent.click(await screen.findByTestId('group-row-g-team'));
    const editor = await screen.findByTestId('group-editor');
    fireEvent.click(within(editor).getByTestId('group-editor-delete'));

    await waitFor(() => {
      const call = vi.mocked(invoke).mock.calls.find(([cmd]) => cmd === 'group_delete');
      expect(call).toBeTruthy();
      expect((call?.[1] as { id: string }).id).toBe('g-team');
    });
  });
});
