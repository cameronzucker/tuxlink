import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

const win = vi.hoisted(() => ({
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));

import { TitleBar } from './TitleBar';

describe('TitleBar', () => {
  beforeEach(() => { win.minimize.mockClear(); win.toggleMaximize.mockClear(); win.close.mockClear(); });

  it('renders the app name and active folder', () => {
    render(<TitleBar folderLabel="Inbox" />);
    expect(screen.getByText('Tuxlink')).toBeInTheDocument();
    expect(screen.getByText('— Inbox')).toBeInTheDocument();
  });

  it('has a drag region', () => {
    const { container } = render(<TitleBar folderLabel="Inbox" />);
    expect(container.querySelector('[data-tauri-drag-region]')).not.toBeNull();
  });

  it('wires the window controls', () => {
    render(<TitleBar folderLabel="Inbox" />);
    fireEvent.click(screen.getByRole('button', { name: /minimize/i }));
    fireEvent.click(screen.getByRole('button', { name: /maximize/i }));
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    expect(win.minimize).toHaveBeenCalledOnce();
    expect(win.toggleMaximize).toHaveBeenCalledOnce();
    expect(win.close).toHaveBeenCalledOnce();
  });
});
