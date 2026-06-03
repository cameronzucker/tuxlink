import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FolderSidebar } from './FolderSidebar';
import { SESSION_TYPES } from '../connections/sessionTypes';

describe('<FolderSidebar> (Mock B)', () => {
  it('renders the Mailbox + Connections sections with their items', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-sidebar')).toBeInTheDocument();
    expect(screen.getByText('Mailbox')).toBeInTheDocument();
    expect(screen.getByText('Connections')).toBeInTheDocument();
    expect(screen.getByTestId('folder-inbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-sent')).toBeInTheDocument();
    expect(screen.getByTestId('folder-outbox')).toBeInTheDocument();
    expect(screen.getByTestId('folder-archive')).toBeInTheDocument();
    // Connections accordion: session-type headers replace old static items
    expect(screen.getByTestId('sess-cms')).toBeInTheDocument();
    expect(screen.getByTestId('sess-radio-only')).toBeInTheDocument();
  });

  it('Inbox + Sent + Outbox + Archive are all enabled (tuxlink-ca5x)', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-inbox')).not.toBeDisabled();
    expect(screen.getByTestId('folder-sent')).not.toBeDisabled();
    expect(screen.getByTestId('folder-outbox')).not.toBeDisabled();
    expect(screen.getByTestId('folder-archive')).not.toBeDisabled();
  });

  it('clicking Outbox fires onSelectFolder with the outbox id', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-outbox'));
    expect(onSelect).toHaveBeenCalledWith('outbox');
  });

  it('marks the selected folder with aria-current', () => {
    render(<FolderSidebar selectedFolder="sent" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-sent')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('folder-inbox')).not.toHaveAttribute('aria-current');
  });

  it('clicking a functional folder fires onSelectFolder', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-sent'));
    expect(onSelect).toHaveBeenCalledWith('sent');
  });

  it('clicking Archive fires onSelectFolder with the archive id (tuxlink-ca5x)', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-archive'));
    expect(onSelect).toHaveBeenCalledWith('archive');
  });

  it('shows counts for Inbox + Sent (suppresses zero/missing)', () => {
    render(
      <FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} counts={{ inbox: 3, sent: 87 }} />,
    );
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('3');
    expect(screen.getByTestId('folder-count-sent')).toHaveTextContent('87');
  });
});

// ============================================================================
// User folders — Phase 2 (tuxlink-f62f). The "Folders" section appears below
// the system folders; the `+` button opens NewFolderDialog (via the parent's
// `onCreateFolder` callback).
// ============================================================================

describe('<FolderSidebar> — user folders (tuxlink-f62f)', () => {
  const sampleFolders = [
    { slug: 'ares-drills', displayName: 'ARES Drills', createdAt: '2026-06-02T12:00:00Z' },
    { slug: 'disaster-prep', displayName: 'Disaster Prep', createdAt: '2026-06-02T13:00:00Z' },
  ];

  it('renders the Folders section heading', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    expect(screen.getByText('Folders')).toBeInTheDocument();
  });

  it('shows the empty-hint copy when no user folders exist + create is wired', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        onCreateFolder={vi.fn()}
      />,
    );
    expect(screen.getByTestId('folders-empty-hint')).toHaveTextContent('Click + to create one');
  });

  it('does NOT show the + button when onCreateFolder is absent', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    expect(screen.queryByTestId('folder-create-btn')).toBeNull();
  });

  it('clicking the + button fires onCreateFolder', () => {
    const onCreateFolder = vi.fn();
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        onCreateFolder={onCreateFolder}
      />,
    );
    fireEvent.click(screen.getByTestId('folder-create-btn'));
    expect(onCreateFolder).toHaveBeenCalledOnce();
  });

  it('renders one row per user folder, with display name + testid', () => {
    render(
      <FolderSidebar
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
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        userFolders={sampleFolders}
      />,
    );
    expect(screen.queryByTestId('folders-empty-hint')).toBeNull();
  });
});

describe('FolderSidebar — Packet connection entry (accordion)', () => {
  // tuxlink-bcgj: the transport-state dot (off/listening/connected) was
  // removed for visual cohesion — the same info already renders in the
  // DashboardRibbon connection chip. The Packet row now mirrors every
  // other transport row (label + selection only).
  it('renders a selectable Packet (AX.25) item (under cms accordion)', () => {
    render(
      <FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />,
    );
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
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onSelectConnection={onSelectConnection}
      />,
    );
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-packet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'packet' });
  });

  it('marks CMS Packet active when selectedConnection matches', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        selectedConnection={{ sessionType: 'cms', protocol: 'packet' }}
      />,
    );
    // The accordion auto-expands because selectedConnection is pre-set — no manual click needed.
    expect(screen.getByTestId('proto-cms-packet')).toHaveClass('active');
  });
});

describe('FolderSidebar — Connections accordion', () => {
  it('renders a header per session type, collapsed by default', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    for (const s of SESSION_TYPES) {
      expect(screen.getByTestId(`sess-${s.id}`)).toHaveAttribute('aria-expanded', 'false');
    }
  });
  it('expands a session type to reveal its protocols', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByTestId('proto-cms-telnet')).toBeInTheDocument();
    expect(screen.getByTestId('proto-cms-packet')).toBeInTheDocument();
  });
  it('selecting a built protocol calls onSelectConnection with the key', () => {
    const onSelectConnection = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    fireEvent.click(screen.getByTestId('proto-cms-telnet'));
    expect(onSelectConnection).toHaveBeenCalledWith({ sessionType: 'cms', protocol: 'telnet' });
  });
  it('a "soon" protocol is disabled and does not fire selection', () => {
    // tuxlink-dfmf flipped cms.vara-hf/vara-fm to built:true (Phase 2 UI).
    // p2p.vara-hf remains unbuilt, so it's the test target for the
    // disabled-protocol behavior. Test target is intent-agnostic — any
    // built parent with at least one unbuilt protocol works.
    const onSelectConnection = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('sess-p2p'));
    const vara = screen.getByTestId('proto-p2p-vara-hf');
    expect(vara).toBeDisabled();
    fireEvent.click(vara);
    expect(onSelectConnection).not.toHaveBeenCalled();
  });

  it('auto-expands the session type of a pre-set selectedConnection (selection always visible)', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={vi.fn()}
        selectedConnection={{ sessionType: 'cms', protocol: 'telnet' }}
        onSelectConnection={vi.fn()}
      />,
    );
    // No manual click on sess-cms — the row must already be present + active:
    const row = screen.getByTestId('proto-cms-telnet');
    expect(row).toBeInTheDocument();
    expect(row).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('sess-cms')).toHaveAttribute('aria-expanded', 'true');
  });
});
