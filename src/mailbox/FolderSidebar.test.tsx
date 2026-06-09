import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FolderSidebar, buildFolderTree } from './FolderSidebar';
import type { UserFolder } from './types';
import { SESSION_TYPES } from '../connections/sessionTypes';

// tuxlink-813d P1 fix: FolderSidebar now branches on the `compact` prop.
//   - DESKTOP (compact omitted / false): the ORIGINAL labeled `.sidebar` nav
//     renders inline — full `.nav-item` rows, the Folders `+`, and the
//     Connections accordion (`sess-*`/`proto-*`) all reachable WITHOUT a `☰`.
//     No `.vtab` rail, no `.sidebar-flyout`, no `rail-expand-btn`.
//   - COMPACT (compact={true}): the collapsed vertical-text `.vtab` rail (owns
//     `folder-<id>`/`user-folder-<slug>`) + the `☰` expand button + the
//     absolutely-positioned `.sidebar-flyout` (section headings, the Folders
//     `+`, the Connections accordion). Tests that touch flyout-only controls
//     click `rail-expand-btn` first to mount the flyout.

// ============================================================================
// DESKTOP — the original labeled sidebar (no rail, no flyout, no ☰).
// ============================================================================

describe('<FolderSidebar> — desktop labeled nav (default / compact=false)', () => {
  it('renders the labeled nav inline: folders, sections, create +, and the Connections accordion — no rail/flyout/☰', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onCreateFolder={() => {}}
      />,
    );
    const nav = screen.getByTestId('folder-sidebar');
    expect(nav).toBeInTheDocument();
    // System folders present inline.
    expect(screen.getByTestId('folder-inbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sent')).toBeInTheDocument();
    expect(screen.getByTestId('folder-outbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-drafts')).toBeInTheDocument();
    expect(screen.getByTestId('folder-archive')).toBeInTheDocument();
    // Section headings + create + connections accordion all reachable inline —
    // no expand click required (there is no ☰ on desktop).
    expect(screen.getByText('Mailbox')).toBeInTheDocument();
    expect(screen.getByText('Folders')).toBeInTheDocument();
    expect(screen.getByText('Connections')).toBeInTheDocument();
    expect(screen.getByTestId('folder-create-btn')).toBeInTheDocument();
    expect(screen.getByTestId('sess-cms')).toBeInTheDocument();
    expect(screen.getByTestId('sess-radio-only')).toBeInTheDocument();
    // The original labeled rows are `.nav-item`, not `.vtab`.
    expect(screen.getByTestId('folder-inbox').className).toContain('nav-item');
    expect(screen.getByTestId('folder-inbox').className).not.toContain('vtab');
    // No compact-rework chrome on desktop.
    expect(screen.queryByTestId('rail-expand-btn')).toBeNull();
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();
    expect(screen.queryByTestId('sidebar-scrim')).toBeNull();
  });

  it('clicking the Connections accordion expands inline (no ☰ first)', () => {
    const onSelectConnection = vi.fn();
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onSelectConnection={onSelectConnection}
      />,
    );
    fireEvent.click(screen.getByTestId('sess-cms'));
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'telnet' });
  });

  it('clicking the inline + button fires onCreateFolder', () => {
    const onCreateFolder = vi.fn();
    render(
      <FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} onCreateFolder={onCreateFolder} />,
    );
    fireEvent.click(screen.getByTestId('folder-create-btn'));
    expect(onCreateFolder).toHaveBeenCalledOnce();
  });

  it('clicking a folder fires onSelectFolder (inline rows)', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(onSelect).toHaveBeenCalledWith('sent');
  });

  it('marks the selected folder with aria-current (inline rows)', () => {
    render(<FolderSidebar selectedFolder="sent" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-sent')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-inbox')).not.toHaveAttribute('aria-current');
  });

  it('renders user folders inline with their slugs', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        userFolders={[
          { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-02T12:00:00Z' },
        ]}
      />,
    );
    expect(screen.getByTestId('user-folder-ares-drills')).toHaveTextContent('ARES Drills');
  });
});

describe('<FolderSidebar> (compact rail + flyout — Mock B)', () => {
  it('renders the rail folder tabs + the flyout sections when expanded', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    // Rail tabs (always present, even collapsed).
    expect(screen.getByTestId('folder-inbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sent')).toBeInTheDocument();
    expect(screen.getByTestId('folder-outbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-drafts')).toBeInTheDocument();
    expect(screen.getByTestId('folder-archive')).toBeInTheDocument();
    // Sections + the Connections accordion live in the flyout — expand first.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    expect(screen.getByText('Mailbox')).toBeInTheDocument();
    expect(screen.getByText('Connections')).toBeInTheDocument();
    expect(screen.getByTestId('sess-cms')).toBeInTheDocument();
    expect(screen.getByTestId('sess-radio-only')).toBeInTheDocument();
  });

  it('Inbox + Sent + Outbox + Drafts + Archive are all enabled (tuxlink-n8gm)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-inbox')).not.toBeDisabled();
    expect(screen.getByTestId('folder-sent')).not.toBeDisabled();
    expect(screen.getByTestId('folder-outbox')).not.toBeDisabled();
    expect(screen.getByTestId('folder-drafts')).not.toBeDisabled();
    expect(screen.getByTestId('folder-archive')).not.toBeDisabled();
  });

  it('clicking Outbox fires onSelectFolder with the outbox id', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-outbox'));
    expect(onSelect).toHaveBeenCalledWith('outbox');
  });

  it('clicking Drafts fires onSelectFolder with the drafts id', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-drafts'));
    expect(onSelect).toHaveBeenCalledWith('drafts');
  });

  it('marks the selected folder with aria-current', () => {
    render(<FolderSidebar compact selectedFolder="sent" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-sent')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-inbox')).not.toHaveAttribute('aria-current');
  });

  it('clicking a functional folder fires onSelectFolder', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(onSelect).toHaveBeenCalledWith('sent');
  });

  it('clicking Archive fires onSelectFolder with the archive id (tuxlink-ca5x)', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-archive'));
    expect(onSelect).toHaveBeenCalledWith('archive');
  });

  it('shows counts for Inbox + Sent (suppresses zero/missing) as vertical chips', () => {
    render(
      <FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} counts={{ inbox: 3, sent: 87 }} />,
    );
    // Rail count chips (`.vcount`) carry the `folder-count-<id>` testid.
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('3');
    expect(screen.getByTestId('folder-count-sent')).toHaveTextContent('87');
    // Outbox has no count → no chip.
    expect(screen.queryByTestId('folder-count-outbox')).toBeNull();
  });

  it('exposes the Contacts pseudo-folder in the rail + flyout (tuxlink-raez / FZ-M1)', () => {
    const onSelect = vi.fn();
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={onSelect}
        contactsCount={5}
      />,
    );
    // Rail: the Contacts tab is present, enabled, shows the contactsCount chip,
    // and selecting it routes to the 'contacts' pseudo-folder.
    const railContacts = screen.getByTestId('folder-contacts');
    expect(railContacts).toBeInTheDocument();
    expect(railContacts).not.toBeDisabled();
    expect(screen.getByTestId('folder-count-contacts')).toHaveTextContent('5');
    fireEvent.click(railContacts);
    expect(onSelect).toHaveBeenCalledWith('contacts');

    // Flyout: expanding surfaces an Address section + the Contacts row, which
    // also routes to 'contacts'.
    onSelect.mockClear();
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByText('Address')).toBeInTheDocument();
    const flyoutContacts = screen.getByTestId('flyout-folder-contacts');
    expect(flyoutContacts).toBeInTheDocument();
    fireEvent.click(flyoutContacts);
    expect(onSelect).toHaveBeenCalledWith('contacts');
  });
});

// ============================================================================
// User folders — Phase 2 (tuxlink-f62f). The "Folders" section appears below
// the system folders; the `+` button opens NewFolderDialog (via the parent's
// `onCreateFolder` callback). In compact, these controls live in the flyout.
// ============================================================================

describe('<FolderSidebar> — user folders (tuxlink-f62f)', () => {
  const sampleFolders = [
    { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-02T12:00:00Z' },
    { slug: 'disaster-prep', displayName: 'Disaster Prep', createdAt: '2026-06-02T13:00:00Z' },
  ];

  it('renders the Folders section heading (in the flyout)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByText('Folders')).toBeInTheDocument();
  });

  it('shows the empty-hint copy when no user folders exist + create is wired (in the flyout)', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        onCreateFolder={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('folders-empty-hint')).toHaveTextContent('Click + to create one');
  });

  it('does NOT show the + button when onCreateFolder is absent (even expanded)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.queryByTestId('folder-create-btn')).toBeNull();
  });

  it('clicking the + button (in the flyout) fires onCreateFolder', () => {
    const onCreateFolder = vi.fn();
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        onCreateFolder={onCreateFolder}
      />,
    );
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    fireEvent.click(screen.getByTestId('folder-create-btn'));
    expect(onCreateFolder).toHaveBeenCalledOnce();
  });

  it('renders one row per user folder, with display name + testid', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        userFolders={sampleFolders}
      />,
    );
    const ares = screen.getByTestId('user-folder-ares-drills');
    const disaster = screen.getByTestId('user-folder-disaster-prep');
    expect(ares).toHaveTextContent('ARES Drills');
    expect(disaster).toHaveTextContent('Disaster Prep');
  });

  it('clicking a user-folder row fires onSelectFolder with the slug', () => {
    const onSelectFolder = vi.fn();
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={onSelectFolder}
        userFolders={sampleFolders}
      />,
    );
    fireEvent.click(screen.getByTestId('user-folder-ares-drills'));
    expect(onSelectFolder).toHaveBeenCalledWith('ares-drills');
  });

  it('marks the selected user folder with aria-current', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="ares-drills"
        onSelectFolder={vi.fn()}
        userFolders={sampleFolders}
      />,
    );
    expect(screen.getByTestId('user-folder-ares-drills')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('user-folder-disaster-prep')).not.toHaveAttribute('aria-current');
  });

  it('hides the empty hint when at least one user folder exists', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        userFolders={sampleFolders}
      />,
    );
    expect(screen.queryByTestId('folders-empty-hint')).toBeNull();
  });
});

// ============================================================================
// Address section — Contacts pseudo-folder (tuxlink-raez, Task A7). The
// "Address" section appears below the system + user folders; its single
// "Contacts" nav-item is NOT a mailbox folder ('contacts' is a pseudo-folder
// string, never added to the MailboxFolder enum). Its count comes from a
// dedicated `contactsCount` prop (sourced from useContacts in AppShell), NOT
// from the mailbox `counts` memo.
// ============================================================================

describe('<FolderSidebar> — Address / Contacts (tuxlink-raez A7)', () => {
  it('renders an "Address" section label', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    expect(screen.getByText('Address')).toBeInTheDocument();
  });

  it('renders a Contacts nav-item', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    const item = screen.getByTestId('folder-contacts');
    expect(item).toBeInTheDocument();
    expect(item).toHaveTextContent('Contacts');
  });

  it('shows the passed contacts count in folder-count-contacts', () => {
    render(
      <FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} contactsCount={12} />,
    );
    expect(screen.getByTestId('folder-count-contacts')).toHaveTextContent('12');
  });

  it('suppresses the count badge when contactsCount is zero/missing', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    expect(screen.queryByTestId('folder-count-contacts')).toBeNull();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} contactsCount={0} />);
    expect(screen.queryByTestId('folder-count-contacts')).toBeNull();
  });

  it('clicking Contacts fires onSelectFolder with the contacts pseudo-folder', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-contacts'));
    expect(onSelect).toHaveBeenCalledWith('contacts');
  });

  it('marks Contacts active (aria-current) when selectedFolder is contacts', () => {
    render(<FolderSidebar selectedFolder="contacts" onSelectFolder={vi.fn()} />);
    expect(screen.getByTestId('folder-contacts')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-inbox')).not.toHaveAttribute('aria-current');
  });

  it('does NOT use the mailbox counts memo for the Contacts badge', () => {
    // Passing a mailbox `counts` object that happens to carry a numeric value
    // must not leak into the Contacts badge — the Contacts count is its own
    // prop. With no contactsCount, no badge renders regardless of `counts`.
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        counts={{ inbox: 3, sent: 87 }}
      />,
    );
    expect(screen.queryByTestId('folder-count-contacts')).toBeNull();
  });
});

describe('FolderSidebar — Packet connection entry (accordion)', () => {
  // tuxlink-bcgj: the transport-state dot (off/listening/connected) was
  // removed for visual cohesion — the same info already renders in the
  // DashboardRibbon connection chip. The Packet row now mirrors every
  // other transport row (label + selection only).
  it('renders a selectable Packet (AX.25) item (under cms accordion, in the flyout)', () => {
    render(
      <FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />,
    );
    // The Connections accordion lives in the flyout — expand the rail first.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    // Expand the CMS accordion to reveal protocols
    fireEvent.click(screen.getByTestId('sess-cms'));
    const item = screen.getByTestId('proto-cms-packet');
    expect(item).toHaveTextContent('Packet (AX.25)');
    // The conn-dot was removed — no transport-state indicator in the sidebar.
    expect(screen.queryByTestId('conn-packet-dot')).toBeNull();
  });

  it('clicking CMS Packet calls onSelectConnection with the full key', () => {
    const onSelectConnection = vi.fn();
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onSelectConnection={onSelectConnection}
      />,
    );
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'packet' });
  });

  it('marks CMS Packet active when selectedConnection matches', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        selectedConnection={{ sessionType: 'cms', protocol: 'packet' }}
      />,
    );
    // Open the flyout to reveal the Connections accordion. The accordion
    // auto-expands because selectedConnection is pre-set — no sess-cms click.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('proto-cms-packet')).toHaveClass('active');
  });
});

describe('FolderSidebar — Connections accordion (compact flyout)', () => {
  // The Connections accordion in compact lives only in the `.sidebar-flyout`;
  // every test opens the flyout (rail-expand-btn) before touching
  // `sess-*`/`proto-*`. (Desktop reaches the accordion inline — see the desktop
  // describe block above.)
  it('renders a header per session type, collapsed by default', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    for (const s of SESSION_TYPES) {
      expect(screen.getByTestId(`sess-${s.id}`)).toHaveAttribute('aria-expanded', 'false');
    }
  });
  it('expands a session type to reveal its protocols', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    fireEvent.click(screen.getByTestId('sess-cms'));
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('proto-cms-telnet')).toBeInTheDocument();
    expect(screen.getByTestId('proto-cms-packet')).toBeInTheDocument();
  });
  it('selecting a built protocol calls onSelectConnection with the key', () => {
    const onSelectConnection = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'telnet' });
  });
  it('a "soon" protocol is disabled and does not fire selection', () => {
    // tuxlink-kb3s also flipped p2p.vara-hf/vara-fm to built:true, so the
    // prior p2p-vara test target is no longer disabled. Radio-only's
    // protocols remain unbuilt (parent intent unbuilt; needs Hybrid-Network
    // routing backend), so radio-only-telnet is the new disabled-protocol
    // test target. Test target is intent-agnostic — any unbuilt protocol
    // row exhibits the disabled-button behavior.
    const onSelectConnection = vi.fn();
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    fireEvent.click(screen.getByTestId('sess-radio-only'));
    const telnet = screen.getByTestId('proto-radio-only-telnet');
    expect(telnet).toBeDisabled();
    fireEvent.click(telnet);
    expect(onSelectConnection).not.toHaveBeenCalled();
  });

  it('auto-expands the session type of a pre-set selectedConnection (selection always visible)', () => {
    render(
      <FolderSidebar
        compact
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        selectedConnection={{ sessionType: 'cms', protocol: 'telnet' }}
        onSelectConnection={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    // No manual click on sess-cms — the row must already be present + active:
    const row = screen.getByTestId('proto-cms-telnet');
    expect(row).toBeInTheDocument();
    expect(row).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
  });
});

describe('<FolderSidebar> — FZ-M1 compact rail (tuxlink-h7q7 / tuxlink-813d)', () => {
  // tuxlink-813d D2: the rail renders vertical-text tabs (`.vtab` with a
  // `.vlabel`), NOT clipped `.nav-label` rows. The full labeled rows (with
  // `.nav-label` + section headings + `+`) moved to the flyout.
  it('renders each folder as a vertical-text tab (.vtab/.vlabel) with no bare text node', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    const inbox = screen.getByTestId('folder-inbox');
    expect(inbox.className).toContain('vtab');
    expect(inbox.querySelector('.vlabel')?.textContent).toBe('Inbox');
    // Reserved count slot present even without a count (keeps labels aligned).
    expect(inbox.querySelector('.vslot')).toBeInTheDocument();
    // No direct text-node child of the button (would show raw text in the rail).
    const directText = Array.from(inbox.childNodes)
      .filter((n) => n.nodeType === Node.TEXT_NODE && (n.textContent ?? '').trim() !== '');
    expect(directText).toHaveLength(0);
  });

  it('keeps section headings + the + button in the flyout (classed, not inline — F4/Task 9)', () => {
    render(
      <FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} onCreateFolder={() => {}} />,
    );
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByText('Folders').className).toContain('section-label-text');
    const createBtn = screen.getByTestId('folder-create-btn');
    expect(createBtn.className).toContain('folder-create-btn');
  });

  it('renders the empty-hint with a class (in the flyout), not inline styles', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} onCreateFolder={() => {}} />);
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('folders-empty-hint').className).toContain('folders-empty-hint');
  });

  it('renders a separate flyout overlay + scrim when expanded, keeping the rail mounted', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} onCreateFolder={() => {}} />);
    // Collapsed: no flyout, no scrim; the rail is present.
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();
    expect(screen.queryByTestId('sidebar-scrim')).toBeNull();
    // Expanded: the rail STAYS mounted (never goes absolute) and the flyout +
    // scrim mount as separate elements.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    expect(screen.getByTestId('sidebar-scrim')).toBeInTheDocument();
  });

  it('marks the rail inert + aria-hidden while the flyout is open (a11y — Codex P2)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    const rail = screen.getByTestId('folder-sidebar');
    // Collapsed: rail is interactive (not inert / not hidden).
    expect(rail).not.toHaveAttribute('inert');
    expect(rail).not.toHaveAttribute('aria-hidden');
    // Expanded: the rail's duplicate controls leave the tab order + a11y tree.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    const railWhileOpen = screen.getByTestId('folder-sidebar');
    expect(railWhileOpen).toHaveAttribute('inert');
    expect(railWhileOpen).toHaveAttribute('aria-hidden', 'true');
  });

  it('moves focus into the flyout on expand and back to the expand button on Escape (F1)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    const expandBtn = screen.getByTestId('rail-expand-btn');

    // Open the flyout — focus should transfer to the flyout nav.
    fireEvent.click(expandBtn);
    const flyout = screen.getByTestId('sidebar-flyout');
    expect(document.activeElement).toBe(flyout);

    // Collapse via Escape — focus should return to the expand button.
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();
    expect(document.activeElement).toBe(expandBtn);
  });

  it('collapses the flyout on folder select, Escape, scrim click, and outside click (F11)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} />);
    const expandBtn = screen.getByTestId('rail-expand-btn');

    // expand → collapse on folder select (rail tab)
    fireEvent.click(expandBtn);
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();

    // expand → collapse on Escape
    fireEvent.click(expandBtn);
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();

    // expand → collapse on scrim click
    fireEvent.click(expandBtn);
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('sidebar-scrim'));
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();

    // expand → collapse on outside pointer-down
    fireEvent.click(expandBtn);
    expect(screen.getByTestId('sidebar-flyout')).toBeInTheDocument();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId('sidebar-flyout')).toBeNull();
  });
});

// ============================================================================
// Nested folders (tuxlink-ka3z) — buildFolderTree + desktop tree render.
// ============================================================================
describe('buildFolderTree', () => {
  const folders: UserFolder[] = [
    { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
    { slug: 'ares', displayName: 'ARES', createdAt: 'b', parentSlug: 'nets' },
    { slug: 'weather', displayName: 'Weather', createdAt: 'c' },
  ];

  it('orders children directly under their parent with depth + hasChildren', () => {
    const rows = buildFolderTree(folders, new Set());
    expect(rows.map((r) => [r.folder.slug, r.depth, r.hasChildren])).toEqual([
      ['nets', 0, true],
      ['ares', 1, false],
      ['weather', 0, false],
    ]);
  });

  it('omits children of a collapsed parent', () => {
    const rows = buildFolderTree(folders, new Set(['nets']));
    expect(rows.map((r) => r.folder.slug)).toEqual(['nets', 'weather']);
  });

  it('treats a folder with a dangling parent as top-level (never vanishes)', () => {
    const orphaned: UserFolder[] = [{ slug: 'lost', displayName: 'Lost', createdAt: 'a', parentSlug: 'ghost' }];
    const rows = buildFolderTree(orphaned, new Set());
    expect(rows.map((r) => [r.folder.slug, r.depth])).toEqual([['lost', 0]]);
  });
});

describe('<FolderSidebar> nested folder rendering (desktop)', () => {
  const folders: UserFolder[] = [
    { slug: 'nets', displayName: 'Nets', createdAt: 'a' },
    { slug: 'ares', displayName: 'ARES', createdAt: 'b', parentSlug: 'nets' },
    { slug: 'weather', displayName: 'Weather', createdAt: 'c' },
  ];

  it('renders subfolders indented under their parent (data-depth)', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} userFolders={folders} />);
    expect(screen.getByTestId('user-folder-nets')).toHaveAttribute('data-depth', '0');
    expect(screen.getByTestId('user-folder-ares')).toHaveAttribute('data-depth', '1');
    expect(screen.getByTestId('user-folder-weather')).toHaveAttribute('data-depth', '0');
  });

  it('collapsing a parent hides its children but not its siblings', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} userFolders={folders} />);
    fireEvent.click(screen.getByTestId('folder-toggle-nets'));
    expect(screen.queryByTestId('user-folder-ares')).toBeNull();
    expect(screen.getByTestId('user-folder-weather')).toBeInTheDocument();
  });

  it('clicking the twisty toggles without selecting the folder', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} userFolders={folders} />);
    fireEvent.click(screen.getByTestId('folder-toggle-nets'));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('the compact rail shows only top-level folders (subfolders via the flyout tree)', () => {
    render(<FolderSidebar compact selectedFolder="inbox" onSelectFolder={() => {}} userFolders={folders} />);
    // Rail: top-level present, subfolder absent.
    expect(screen.getByTestId('user-folder-nets')).toBeInTheDocument();
    expect(screen.queryByTestId('user-folder-ares')).toBeNull();
    // Expand the flyout: the full tree (incl. the subfolder) renders there.
    fireEvent.click(screen.getByTestId('rail-expand-btn'));
    expect(screen.getByTestId('flyout-user-folder-ares')).toHaveAttribute('data-depth', '1');
  });
});
