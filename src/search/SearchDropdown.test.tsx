import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SearchDropdown, type SearchDropdownProps } from './SearchDropdown';
import { EMPTY_SPEC, type RecentSearch, type SavedSearch } from './types';

const saved: SavedSearch[] = [
  { id: '1', name: 'Storm Net 5/30', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 0 },
  { id: '2', name: 'ICS-213 last 24h', spec: EMPTY_SPEC, created_at: 0, last_used_at: null, order: 1 },
];
const recent: RecentSearch[] = [
  { spec: { ...EMPTY_SPEC, free_text: 'outage' }, ran_at: 100 },
  { spec: { ...EMPTY_SPEC, free_text: 'weather' }, ran_at: 50 },
];

function defaultProps(overrides: Partial<SearchDropdownProps> = {}): SearchDropdownProps {
  return {
    saved, recent, activeSavedId: null,
    onRunSaved: () => {}, onRunRecent: () => {}, onPromoteRecent: () => {},
    onUnsaveActive: () => {}, onManage: () => {}, onClose: () => {},
    ...overrides,
  };
}

describe('SearchDropdown', () => {
  it('renders saved section above recent section', () => {
    render(<SearchDropdown {...defaultProps()} />);
    const labels = screen.getAllByTestId(/section-label/);
    expect(labels[0]).toHaveTextContent(/Saved/);
    expect(labels[1]).toHaveTextContent(/Recent/);
  });

  it('clicking a saved row calls onRunSaved with that saved-search', () => {
    const onRunSaved = vi.fn();
    render(<SearchDropdown {...defaultProps({ onRunSaved })} />);
    fireEvent.click(screen.getByTestId('dropdown-saved-row-1'));
    expect(onRunSaved).toHaveBeenCalledWith(saved[0]);
  });

  it('clicking ☆ on a recent row opens inline rename, Enter saves with that name', () => {
    const onPromote = vi.fn();
    render(<SearchDropdown {...defaultProps({ onPromoteRecent: onPromote })} />);
    fireEvent.click(screen.getByTestId('dropdown-recent-star-0'));
    const input = screen.getByTestId('dropdown-name-recent-0').querySelector('input')!;
    fireEvent.change(input, { target: { value: 'My pick' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onPromote).toHaveBeenCalledWith(recent[0], 'My pick');
  });

  it('arrow-down then enter runs the focused row', () => {
    const onRunSaved = vi.fn();
    render(<SearchDropdown {...defaultProps({ onRunSaved })} />);
    fireEvent.keyDown(window, { key: 'ArrowDown' });
    fireEvent.keyDown(window, { key: 'Enter' });
    expect(onRunSaved).toHaveBeenCalled();
  });

  it('clicking Manage calls onManage', () => {
    const onManage = vi.fn();
    render(<SearchDropdown {...defaultProps({ onManage })} />);
    fireEvent.click(screen.getByTestId('dropdown-manage'));
    expect(onManage).toHaveBeenCalled();
  });

  it('does NOT render a save-current row (save flow is star-from-recent only)', () => {
    render(<SearchDropdown {...defaultProps()} />);
    expect(screen.queryByTestId('dropdown-save-current')).not.toBeInTheDocument();
    expect(screen.queryByTestId('dropdown-name-input-current')).not.toBeInTheDocument();
  });
});
