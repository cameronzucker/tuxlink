import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MessageListSortControl } from './MessageListSortControl';
import { SORT_OPTIONS, type SortMode } from './messageSort';

describe('<MessageListSortControl>', () => {
  it('renders a labeled select with every sort option', () => {
    render(<MessageListSortControl value="date-desc" onChange={() => {}} />);
    const select = screen.getByTestId('message-list-sort-select') as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    // every defined SORT_OPTIONS.id appears as an <option>
    for (const opt of SORT_OPTIONS) {
      expect(select.querySelector(`option[value="${opt.id}"]`)).toBeInTheDocument();
    }
    // the visible label is associated with the select (a11y)
    const label = screen.getByText('Sort');
    expect(label).toBeInTheDocument();
    expect(label.tagName).toBe('LABEL');
    expect(label.getAttribute('for')).toBe(select.id);
  });

  it('reflects the current value as the selected option', () => {
    const { rerender } = render(<MessageListSortControl value="subject-asc" onChange={() => {}} />);
    const select = screen.getByTestId('message-list-sort-select') as HTMLSelectElement;
    expect(select.value).toBe('subject-asc');
    rerender(<MessageListSortControl value="sender-desc" onChange={() => {}} />);
    expect(select.value).toBe('sender-desc');
  });

  it('calls onChange with the new SortMode when the user picks one', () => {
    const onChange = vi.fn();
    render(<MessageListSortControl value="date-desc" onChange={onChange} />);
    const select = screen.getByTestId('message-list-sort-select') as HTMLSelectElement;
    fireEvent.change(select, { target: { value: 'sender-asc' satisfies SortMode } });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith('sender-asc');
  });

  it('renders option labels from SORT_OPTIONS (not internal ids)', () => {
    render(<MessageListSortControl value="date-desc" onChange={() => {}} />);
    // Sanity-check a couple labels — the dropdown is what users see, not the enum
    expect(screen.getByRole('option', { name: 'Newest first' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Sender A→Z' })).toBeInTheDocument();
  });
});
