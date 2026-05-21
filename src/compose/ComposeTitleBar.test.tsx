import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ComposeTitleBar } from './ComposeTitleBar';

describe('ComposeTitleBar', () => {
  it('renders the title and a drag region', () => {
    const { container } = render(<ComposeTitleBar onClose={vi.fn()} />);
    expect(screen.getByText('New Message')).toBeInTheDocument();
    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
  });

  it('calls onClose when the close control is clicked', () => {
    const onClose = vi.fn();
    render(<ComposeTitleBar onClose={onClose} />);
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
