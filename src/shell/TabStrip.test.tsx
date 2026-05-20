import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TabStrip, SHELL_TABS } from './TabStrip';

describe('<TabStrip>', () => {
  it('renders the four functional folder tabs', () => {
    render(<TabStrip selectedFolder="inbox" onSelectFolder={() => {}} />);
    for (const t of SHELL_TABS) {
      expect(screen.getByTestId(`tab-${t.id}`)).toBeInTheDocument();
    }
    expect(SHELL_TABS.map((t) => t.id)).toEqual(['inbox', 'outbox', 'sent', 'drafts']);
  });

  it('marks the selected folder active', () => {
    render(<TabStrip selectedFolder="sent" onSelectFolder={() => {}} />);
    expect(screen.getByTestId('tab-sent').className).toContain('active');
    expect(screen.getByTestId('tab-inbox').className).not.toContain('active');
  });

  it('shows a count badge only for non-zero counts', () => {
    render(<TabStrip selectedFolder="inbox" onSelectFolder={() => {}} counts={{ inbox: 3, sent: 0 }} />);
    expect(screen.getByTestId('tab-count-inbox')).toHaveTextContent('3');
    expect(screen.queryByTestId('tab-count-sent')).toBeNull();
  });

  it('fires onSelectFolder with the tab id on click', () => {
    const onSelect = vi.fn();
    render(<TabStrip selectedFolder="inbox" onSelectFolder={onSelect} />);
    fireEvent.click(screen.getByTestId('tab-sent'));
    expect(onSelect).toHaveBeenCalledWith('sent');
  });
});
