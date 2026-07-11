// ContactsPanel tests — the unified outline (tuxlink-je5d).
//
// Covers the reshaped surface: one tree (collapsible groups holding members
// inline + an Ungrouped section), callsign-first rows, the polymorphic detail
// (member → contact detail w/ connection record · group header → inline group
// management · raw → save), suggested-as-rows, multi-select → add-to-group /
// remove, search, and the explicit absence of "Message all". The pure tree model
// is unit-tested separately in contactTree.test.ts.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

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
// Callsign-only contact (no name) — Winlink reality.
const NONAME: Contact = {
  id: 'c-noname',
  name: '',
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
    if (cmd === 'contact_delete') return undefined;
    if (cmd === 'group_upsert') return args?.group as Group;
    if (cmd === 'group_delete') return undefined;
    // The carried-over connection record: honest empty state by default.
    if (cmd === 'contacts_connection_record') return { attempts: [], hint: null };
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

// ─────────────────────────────────────────────────────────────────────────────
describe('<ContactsPanel> — outline tree', () => {
  it('renders groups as collapsible sections holding members, plus an Ungrouped section', async () => {
    routeInvoke({ contacts: [ALICE, NONAME], groups: [TEAM] });
    renderPanel();

    // Group section + its member (Alice is in Team Alpha; groups start expanded).
    expect(await screen.findByTestId('group-section-g-team')).toBeInTheDocument();
    expect(await screen.findByTestId('contact-row-c-alice')).toBeInTheDocument();
    // NONAME is in no group → Ungrouped.
    const ungrouped = await screen.findByTestId('contacts-ungrouped');
    expect(within(ungrouped).getByTestId('contact-row-c-noname')).toBeInTheDocument();
  });

  it('collapsing a group hides its members', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();
    expect(await screen.findByTestId('contact-row-c-alice')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('group-caret-g-team'));
    await waitFor(() => expect(screen.queryByTestId('contact-row-c-alice')).not.toBeInTheDocument());
  });

  it('renders a callsign-only contact without a name (callsign is the identity)', async () => {
    routeInvoke({ contacts: [NONAME] });
    renderPanel();
    const row = await screen.findByTestId('contact-row-c-noname');
    expect(row).toHaveTextContent('KE7XYZ');
    expect(row).not.toHaveTextContent('Alice');
  });

  it('search filters the tree', async () => {
    routeInvoke({ contacts: [ALICE, NONAME] });
    renderPanel();
    await screen.findByTestId('contact-row-c-alice');
    fireEvent.change(screen.getByTestId('contacts-search'), { target: { value: 'KE7' } });
    await waitFor(() => expect(screen.queryByTestId('contact-row-c-alice')).not.toBeInTheDocument());
    expect(screen.getByTestId('contact-row-c-noname')).toBeInTheDocument();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
describe('<ContactsPanel> — polymorphic detail', () => {
  it('selecting a member shows the contact detail with the connection-record card', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('contact-row-c-alice'));
    expect(await screen.findByTestId('contact-detail')).toBeInTheDocument();
    expect(screen.getByTestId('contact-detail-callsign')).toHaveTextContent('W6ABC');
    expect(screen.getByTestId('contact-detail-tactical')).toHaveTextContent('NET-CONTROL');
    // The carried-over favorites connection record (empty state here).
    expect(screen.getByTestId('contact-record-card')).toBeInTheDocument();
  });

  it('New message seeds a draft and opens a compose window', async () => {
    routeInvoke({ contacts: [ALICE] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('contact-row-c-alice'));
    fireEvent.click(await screen.findByTestId('contact-new-message'));
    // openComposeTo persists a To=callsign draft (localStorage) then opens the window.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'compose_window_open',
        expect.objectContaining({ draftId: expect.any(String) }),
      ),
    );
  });

  it('selecting a group header opens inline group management', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('group-name-g-team'));
    expect(await screen.findByTestId('group-management')).toBeInTheDocument();
    // The member is listed for management.
    expect(screen.getByTestId('group-management-members')).toHaveTextContent('W6ABC');
  });

  it('Delete in group management routes through group_delete', async () => {
    routeInvoke({ contacts: [ALICE], groups: [TEAM] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('group-name-g-team'));
    fireEvent.click(await screen.findByTestId('group-management-delete'));
    fireEvent.click(await screen.findByTestId('group-management-delete-confirm'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('group_delete', expect.objectContaining({ id: 'g-team' })),
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
describe('<ContactsPanel> — suggestions dissolve into Ungrouped', () => {
  it('a suggested-from-traffic callsign renders as a row with one-click Save → contact_upsert', async () => {
    routeInvoke({ suggestions: [{ callsign: 'AE7PT', message_count: 3 }] });
    renderPanel();
    expect(await screen.findByTestId('suggestion-AE7PT')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('suggestion-add-AE7PT'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'contact_upsert',
        expect.objectContaining({ contact: expect.objectContaining({ callsign: 'AE7PT' }) }),
      ),
    );
  });
});

// ─────────────────────────────────────────────────────────────────────────────
describe('<ContactsPanel> — multi-select', () => {
  it('Ctrl+click selects rows and shows the bulk bar; Add-to-new-group routes through group_upsert', async () => {
    routeInvoke({ contacts: [ALICE, NONAME] });
    renderPanel();
    const alice = await screen.findByTestId('contact-row-c-alice');
    const noname = await screen.findByTestId('contact-row-c-noname');
    fireEvent.click(alice, { ctrlKey: true });
    fireEvent.click(noname, { ctrlKey: true });

    const bar = await screen.findByTestId('contacts-bulk-bar');
    expect(within(bar).getByTestId('contacts-bulk-count')).toHaveTextContent('2');

    fireEvent.click(screen.getByTestId('contacts-bulk-add-to-group'));
    fireEvent.change(await screen.findByTestId('contacts-bulk-newgroup-input'), {
      target: { value: 'Field Team' },
    });
    fireEvent.click(screen.getByTestId('contacts-bulk-newgroup-add'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'group_upsert',
        expect.objectContaining({ group: expect.objectContaining({ name: 'Field Team' }) }),
      ),
    );
  });

  it('does NOT offer a "Message all" action (messaging belongs to Compose / groups)', async () => {
    routeInvoke({ contacts: [ALICE, NONAME] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('contact-row-c-alice'), { ctrlKey: true });
    await screen.findByTestId('contacts-bulk-bar');
    expect(screen.queryByText(/message all/i)).not.toBeInTheDocument();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// Task T-F — the Recent section + contact-detail reachability. Unconfirmed
// contacts live in "Recent" (never the curated list); each row makes its OWN
// honest RF claim; promote flips tier via contact_confirm; the detail Connect
// dispatches the Task-23a p2p seam (never a CMS fallback); the operator UI shows
// telnet host:port.
const RECENT_VARA: Contact = {
  id: 'c-recent',
  name: '',
  callsign: 'W7XYZ-5',
  tier: 'unconfirmed',
  origin: 'incoming',
  channels: [
    {
      transport: 'vara-hf', target_callsign: 'W7XYZ-5', via: ['RELAY1'],
      freq_hz: 7_101_000, bandwidth: null, direction: 'incoming',
      counts: { ok: 1, fail: 0 }, last_seen: '2026-07-11T09:00:00-07:00',
      last_ok: '2026-07-11T09:00:00-07:00',
    },
  ],
  endpoints: [],
  created_at: NOW,
  updated_at: NOW,
};
const RECENT_DIALED: Contact = {
  id: 'c-dialed',
  name: '',
  callsign: 'K1DIAL',
  tier: 'unconfirmed',
  origin: 'outgoing',
  channels: [
    {
      transport: 'ardop', target_callsign: 'K1DIAL', via: [],
      freq_hz: 7_105_000, bandwidth: null, direction: 'outgoing',
      counts: { ok: 0, fail: 2 }, last_seen: '2026-07-11T09:00:00-07:00',
      last_ok: null,
    },
  ],
  endpoints: [],
  created_at: NOW,
  updated_at: NOW,
};
const RECENT_TELNET: Contact = {
  id: 'c-telnet',
  name: '',
  callsign: 'W7TEL',
  tier: 'unconfirmed',
  origin: 'outgoing',
  channels: [],
  endpoints: [
    {
      id: 'ep-9', host: '10.0.0.5', port: 8774, provenance: 'operator',
      last_seen: '2026-07-11T09:00:00-07:00', last_ok: null,
    },
  ],
  created_at: NOW,
  updated_at: NOW,
};

describe('<ContactsPanel> — Recent section + reachability (Task T-F)', () => {
  it('routes unconfirmed contacts to Recent and keeps confirmed ones in the curated tree', async () => {
    routeInvoke({ contacts: [ALICE, RECENT_VARA] });
    renderPanel();
    // Confirmed ALICE → curated row; never a Recent row.
    expect(await screen.findByTestId('contact-row-c-alice')).toBeInTheDocument();
    // Unconfirmed RECENT_VARA → Recent section; never the curated tree.
    const recent = await screen.findByTestId('contacts-recent');
    expect(within(recent).getByTestId('recent-row-c-recent')).toBeInTheDocument();
    expect(screen.queryByTestId('contact-row-c-recent')).not.toBeInTheDocument();
    expect(screen.queryByTestId('recent-row-c-alice')).not.toBeInTheDocument();
  });

  it('hides the Recent section entirely when there are no unconfirmed contacts', async () => {
    routeInvoke({ contacts: [ALICE] });
    renderPanel();
    await screen.findByTestId('contact-row-c-alice');
    expect(screen.queryByTestId('contacts-recent')).not.toBeInTheDocument();
  });

  it('a completed (last_ok) row carries the Heard distinction; a fail-only row reads dialed-not-reached', async () => {
    routeInvoke({ contacts: [RECENT_VARA, RECENT_DIALED] });
    renderPanel();
    expect(await screen.findByTestId('recent-status-c-recent')).toHaveTextContent(/^heard/);
    expect(screen.getByTestId('recent-status-c-dialed')).toHaveTextContent('dialed · not reached yet');
  });

  it('the "+ Add" promote calls contact_confirm', async () => {
    routeInvoke({ contacts: [RECENT_VARA] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('recent-add-c-recent'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('contact_confirm', expect.objectContaining({ id: 'c-recent' })),
    );
  });

  it('selecting a Recent row opens its detail with the reachability block + promote', async () => {
    routeInvoke({ contacts: [RECENT_VARA] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('recent-row-c-recent'));
    expect(await screen.findByTestId('contact-detail')).toBeInTheDocument();
    expect(screen.getByTestId('contact-reachability')).toBeInTheDocument();
    expect(screen.getByTestId('contact-promote')).toBeInTheDocument();
  });

  it('detail RF Connect dispatches the p2p seam with exact target/via/freq (never CMS)', async () => {
    routeInvoke({ contacts: [RECENT_VARA] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('recent-row-c-recent'));
    fireEvent.click(await screen.findByTestId('reach-channel-connect-c-recent-0'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'modem_vara_b2f_exchange',
        expect.objectContaining({
          target: 'W7XYZ-5', intent: 'p2p', transportKind: 'vara-hf',
          via: ['RELAY1'], freqHz: 7_101_000,
        }),
      ),
    );
    expect(invoke).not.toHaveBeenCalledWith('cms_connect', expect.anything());
  });

  it('detail telnet endpoint shows host:port to the operator and Connect dials telnet_p2p_connect', async () => {
    routeInvoke({ contacts: [RECENT_TELNET] });
    renderPanel();
    fireEvent.click(await screen.findByTestId('recent-row-c-telnet'));
    const epRow = await screen.findByTestId('reach-endpoint-ep-9');
    expect(epRow).toHaveTextContent('10.0.0.5:8774'); // operator UI shows the address
    fireEvent.click(screen.getByTestId('reach-endpoint-connect-ep-9'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'telnet_p2p_connect',
        expect.objectContaining({
          req: expect.objectContaining({ host: '10.0.0.5', port: 8774, peer_callsign: 'W7TEL' }),
        }),
      ),
    );
    expect(invoke).not.toHaveBeenCalledWith('cms_connect', expect.anything());
  });
});

// ─────────────────────────────────────────────────────────────────────────────
describe('<ContactsPanel> — editor', () => {
  it('+ New opens the editor; saving an entered callsign calls contact_upsert', async () => {
    routeInvoke({});
    renderPanel();
    fireEvent.click(await screen.findByTestId('contacts-new'));
    const editor = await screen.findByTestId('contact-editor');
    fireEvent.change(within(editor).getByTestId('editor-callsign'), {
      target: { value: 'N0DXE' },
    });
    fireEvent.click(within(editor).getByTestId('editor-save'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'contact_upsert',
        expect.objectContaining({ contact: expect.objectContaining({ callsign: 'N0DXE' }) }),
      ),
    );
  });
});
