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

  it('Inbox + Sent are enabled; Outbox + Archive are disabled (v0.1)', () => {
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('folder-inbox')).not.toBeDisabled();
    expect(screen.getByTestId('folder-sent')).not.toBeDisabled();
    expect(screen.getByTestId('folder-outbox')).toBeDisabled();
    expect(screen.getByTestId('folder-archive')).toBeDisabled();
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

  it('clicking a disabled folder does NOT fire onSelectFolder', () => {
    const onSelect = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('folder-outbox'));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('shows counts for Inbox + Sent (suppresses zero/missing)', () => {
    render(
      <FolderSidebar selectedFolder="inbox" onSelectFolder={() => {}} counts={{ inbox: 3, sent: 87 }} />,
    );
    expect(screen.getByTestId('folder-count-inbox')).toHaveTextContent('3');
    expect(screen.getByTestId('folder-count-sent')).toHaveTextContent('87');
  });
});

describe('FolderSidebar — Packet connection entry (accordion)', () => {
  it('renders a selectable Packet (AX.25) item with a state dot (under cms accordion)', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        packetState="listening"
      />,
    );
    // Expand the CMS accordion to reveal protocols
    fireEvent.click(screen.getByTestId('sess-cms'));
    const item = screen.getByTestId('proto-cms-packet');
    expect(item).toHaveTextContent('Packet (AX.25)');
    expect(screen.getByTestId('conn-packet-dot').className).toContain('listening');
  });

  it('clicking CMS Packet calls onSelectConnection with the full key', () => {
    const onSelectConnection = vi.fn();
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onSelectConnection={onSelectConnection}
        packetState="off"
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
        packetState="connected"
      />,
    );
    fireEvent.click(screen.getByTestId('sess-cms'));
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
    const onSelectConnection = vi.fn();
    render(<FolderSidebar selectedFolder="inbox" onSelectFolder={vi.fn()} onSelectConnection={onSelectConnection} />);
    fireEvent.click(screen.getByTestId('sess-cms'));
    const vara = screen.getByTestId('proto-cms-vara-hf');
    expect(vara).toBeDisabled();
    fireEvent.click(vara);
    expect(onSelectConnection).not.toHaveBeenCalled();
  });
});
