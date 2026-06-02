import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MessageListSortControl } from './MessageListSortControl';
import type { SortState } from './messageSort';

const DATE_DESC: SortState = { key: 'date', direction: 'desc' };

describe('<MessageListSortControl>', () => {
  it('renders only the icon trigger when closed (popup is portaled lazily)', () => {
    render(<MessageListSortControl value={DATE_DESC} onChange={() => {}} />);
    const trigger = screen.getByTestId('message-list-sort-trigger');
    expect(trigger).toBeInTheDocument();
    expect(trigger).toHaveAttribute('aria-label', 'Sort messages');
    // Menu not yet open → not in the DOM.
    expect(screen.queryByTestId('message-list-sort-menu')).toBeNull();
  });

  it('opens the popup on trigger click', () => {
    render(<MessageListSortControl value={DATE_DESC} onChange={() => {}} />);
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-menu')).toBeInTheDocument();
    // Both radio groups render (one item per sort key, two for direction).
    expect(screen.getByTestId('message-list-sort-key-date')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-key-sender')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-key-recipient')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-key-subject')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-key-size')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-direction-desc')).toBeInTheDocument();
    expect(screen.getByTestId('message-list-sort-direction-asc')).toBeInTheDocument();
  });

  it('reflects the active sort key + direction as the checked radio items', () => {
    render(
      <MessageListSortControl value={{ key: 'size', direction: 'asc' }} onChange={() => {}} />,
    );
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-key-size')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByTestId('message-list-sort-direction-asc')).toHaveAttribute('aria-checked', 'true');
    // Other items not checked.
    expect(screen.getByTestId('message-list-sort-key-date')).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByTestId('message-list-sort-direction-desc')).toHaveAttribute('aria-checked', 'false');
  });

  it('direction labels adapt to the active key (date: Newest/Oldest)', () => {
    render(<MessageListSortControl value={DATE_DESC} onChange={() => {}} />);
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-direction-desc')).toHaveTextContent('Newest first');
    expect(screen.getByTestId('message-list-sort-direction-asc')).toHaveTextContent('Oldest first');
  });

  it('direction labels adapt to the active key (size: Largest/Smallest)', () => {
    render(
      <MessageListSortControl value={{ key: 'size', direction: 'desc' }} onChange={() => {}} />,
    );
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-direction-desc')).toHaveTextContent('Largest first');
    expect(screen.getByTestId('message-list-sort-direction-asc')).toHaveTextContent('Smallest first');
  });

  it('direction labels adapt to the active key (sender: A→Z / Z→A)', () => {
    render(
      <MessageListSortControl value={{ key: 'sender', direction: 'desc' }} onChange={() => {}} />,
    );
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    expect(screen.getByTestId('message-list-sort-direction-asc')).toHaveTextContent('A → Z');
    expect(screen.getByTestId('message-list-sort-direction-desc')).toHaveTextContent('Z → A');
  });

  it('picking a new sort key fires onChange with the new key + preserved direction', () => {
    const onChange = vi.fn();
    render(
      <MessageListSortControl value={{ key: 'date', direction: 'asc' }} onChange={onChange} />,
    );
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    fireEvent.click(screen.getByTestId('message-list-sort-key-subject'));
    expect(onChange).toHaveBeenCalledWith({ key: 'subject', direction: 'asc' });
  });

  it('picking a new direction fires onChange with the preserved key + new direction', () => {
    const onChange = vi.fn();
    render(
      <MessageListSortControl value={{ key: 'sender', direction: 'desc' }} onChange={onChange} />,
    );
    // Radix DropdownMenu opens on pointerDown (button 0), not click.
    fireEvent.pointerDown(screen.getByTestId('message-list-sort-trigger'), { button: 0 });
    fireEvent.click(screen.getByTestId('message-list-sort-direction-asc'));
    expect(onChange).toHaveBeenCalledWith({ key: 'sender', direction: 'asc' });
  });
});
