import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

const win = vi.hoisted(() => ({
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));

import { ComposeTitleBar } from './ComposeTitleBar';

describe('ComposeTitleBar', () => {
  beforeEach(() => { win.minimize.mockClear(); win.toggleMaximize.mockClear(); });

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

  it('wires minimize + maximize directly to the window API', () => {
    render(<ComposeTitleBar onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: /minimize/i }));
    fireEvent.click(screen.getByRole('button', { name: /maximize/i }));
    expect(win.minimize).toHaveBeenCalledOnce();
    expect(win.toggleMaximize).toHaveBeenCalledOnce();
  });
});
