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
  it('renders placeholder when value is empty', () => {
    render(<SearchBar value="" activeSaved={null} onValueChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByPlaceholderText(/Search messages/i)).toBeInTheDocument();
  });

  it('shows saved-search name + ★ when activeSaved set', () => {
    render(<SearchBar value="damage" activeSaved={STORM} onValueChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    expect(screen.getByTestId('searchbar-saved-name')).toHaveTextContent('Storm Net 5/30');
    expect(screen.getByTestId('searchbar-saved-star')).toBeInTheDocument();
  });

  it('clicking ★ on an active saved search calls onUnsave', () => {
    const onUnsave = vi.fn();
    render(<SearchBar value="damage" activeSaved={STORM} onValueChange={noop} onUnsave={onUnsave} onToggleDropdown={noop} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-saved-star'));
    expect(onUnsave).toHaveBeenCalled();
  });

  it('clicking chevron toggles dropdown', () => {
    const onToggle = vi.fn();
    render(<SearchBar value="" activeSaved={null} onValueChange={noop} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={false} />);
    fireEvent.click(screen.getByTestId('searchbar-chevron'));
    expect(onToggle).toHaveBeenCalled();
  });

  it('typing fires onValueChange with the typed text', () => {
    const onValueChange = vi.fn();
    render(<SearchBar value="" activeSaved={null} onValueChange={onValueChange} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} />);
    const input = screen.getByTestId('searchbar-input');
    fireEvent.change(input, { target: { value: 'from:KX5DD damage' } });
    expect(onValueChange).toHaveBeenCalledWith('from:KX5DD damage');
  });

  it('Escape clears the value and closes dropdown', () => {
    const onValueChange = vi.fn();
    const onToggle = vi.fn();
    render(<SearchBar value="x" activeSaved={null} onValueChange={onValueChange} onUnsave={noop} onToggleDropdown={onToggle} dropdownOpen={true} />);
    fireEvent.keyDown(screen.getByTestId('searchbar-input'), { key: 'Escape' });
    expect(onValueChange).toHaveBeenCalledWith('');
  });

  it('renders meta text when provided', () => {
    render(<SearchBar value="damage" activeSaved={null} onValueChange={noop} onUnsave={noop} onToggleDropdown={noop} dropdownOpen={false} metaText="3 matches · 47 ms" />);
    expect(screen.getByTestId('searchbar-meta')).toHaveTextContent('3 matches · 47 ms');
  });
});
