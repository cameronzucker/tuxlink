import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SearchBar } from './SearchBar';
import { EMPTY_SPEC, type SavedSearch } from './types';

const noop = () => {};
const STORM: SavedSearch = {
  id: '1', name: 'Storm Net 5/30', spec: { ...EMPTY_SPEC, free_text: 'damage' },
  created_at: 0, last_used_at: null, order: 0,
};

describe('SearchBar', () => {
  it('renders placeholder when no spec', () => {
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByPlaceholderText(/Search messages/i)).toBeInTheDocument();
  });

  it('shows saved-search name + ★ when activeSaved set', () => {
    render(<SearchBar spec={STORM.spec} activeSaved={STORM} onSpecChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByTestId('searchbar-saved-name')).toHaveTextContent('Storm Net 5/30');
    expect(screen.getByTestId('searchbar-saved-star')).toBeInTheDocument();
  });

  it('clicking ★ on an active saved search calls onUnsave', () => {
    const onUnsave = vi.fn();
    render(<SearchBar spec={STORM.spec} activeSaved={STORM} onSpecChange={noop} onUnsave={onUnsave} onToggleDropdown={noop} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-saved-star'));
    expect(onUnsave).toHaveBeenCalled();
  });

  it('clicking chevron toggles dropdown', () => {
    const onToggle = vi.fn();
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={noop} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-chevron'));
    expect(onToggle).toHaveBeenCalled();
  });

  it('typing fires onSpecChange with updated free_text', () => {
    const onSpecChange = vi.fn();
    render(<SearchBar spec={EMPTY_SPEC} activeSaved={null} onSpecChange={onSpecChange} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    const input = screen.getByTestId('searchbar-input');
    fireEvent.change(input, { target: { value: 'damage' } });
    expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({ free_text: 'damage' }));
  });

  it('Escape clears spec and closes dropdown', () => {
    const onSpecChange = vi.fn();
    const onToggle = vi.fn();
    render(<SearchBar spec={{ ...EMPTY_SPEC, free_text: 'x' }} activeSaved={null} onSpecChange={onSpecChange} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={true} />);
    fireEvent.keyDown(screen.getByTestId('searchbar-input'), { key: 'Escape' });
    expect(onSpecChange).toHaveBeenCalledWith(EMPTY_SPEC);
  });
});
