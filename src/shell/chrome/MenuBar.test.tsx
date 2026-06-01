import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MenuBar } from './MenuBar';

describe('MenuBar', () => {
  it('renders all seven top menus', () => {
    render(<MenuBar onAction={vi.fn()} />);
    for (const label of ['File', 'Message', 'Session', 'Mailbox', 'View', 'Tools', 'Help']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
  });

  it('opens a dropdown on click and fires onAction for a leaf', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'Message' }));
    fireEvent.click(screen.getByRole('button', { name: /New Message/ }));
    expect(onAction).toHaveBeenCalledWith('menu:message:new');
  });

  it('reveals a submenu leaf (View → Color scheme → Night)', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('button', { name: /Night . tactical/ }));
    expect(onAction).toHaveBeenCalledWith('menu:view:scheme:night-red');
  });

  // tuxlink-39b: not-yet-wired items render disabled + badged (not dead clickables).
  it('renders a not-yet-wired item disabled with a "soon" badge and does not fire onAction', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    const templates = screen.getByRole('button', { name: /Templates/ });
    expect(templates).toBeDisabled();
    fireEvent.click(templates);
    expect(onAction).not.toHaveBeenCalled();
  });

  // tuxlink-39b: "Preferences" removed as a duplicate of "Settings".
  it('no longer offers a Preferences item', () => {
    render(<MenuBar onAction={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    expect(screen.queryByRole('button', { name: /Preferences/ })).not.toBeInTheDocument();
  });
});
