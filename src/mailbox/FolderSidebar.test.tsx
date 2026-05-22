import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FolderSidebar } from './FolderSidebar';
import type { ConnectionKey } from './FolderSidebar';

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
    expect(screen.getByTestId('conn-telnet')).toBeInTheDocument();
    expect(screen.getByTestId('conn-varahf')).toBeInTheDocument();
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

describe('FolderSidebar — Packet connection entry', () => {
  it('renders a selectable Packet (AX.25) item with a state dot', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        packetState="listening"
      />,
    );
    const item = screen.getByTestId('conn-packet');
    expect(item).toHaveTextContent('Packet (AX.25)');
    expect(screen.getByTestId('conn-packet-dot').className).toContain('listening');
  });

  it('clicking Packet (AX.25) calls onSelectConnection("packet")', () => {
    const onSelectConnection = vi.fn();
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        onSelectConnection={onSelectConnection}
        packetState="off"
      />,
    );
    fireEvent.click(screen.getByTestId('conn-packet'));
    expect(onSelectConnection).toHaveBeenCalledWith('packet');
  });

  it('marks Packet active when selectedConnection is "packet"', () => {
    render(
      <FolderSidebar
        selectedFolder="inbox"
        onSelectFolder={() => {}}
        selectedConnection="packet"
        packetState="connected"
      />,
    );
    expect(screen.getByTestId('conn-packet')).toHaveClass('active');
  });
});
