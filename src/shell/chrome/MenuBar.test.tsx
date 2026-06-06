import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MenuBar } from './MenuBar';

const CHROME_CSS_MODULES = import.meta.glob('./chrome.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const APP_SHELL_CSS_MODULES = import.meta.glob('../AppShell.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const chromeCss = CHROME_CSS_MODULES['./chrome.css'];
const appShellCss = APP_SHELL_CSS_MODULES['../AppShell.css'];

function requiredZIndex(css: string, selector: RegExp) {
  const match = css.match(selector);
  expect(match).not.toBeNull();
  return Number(match?.[1]);
}

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

  // tuxlink-dpf: the not-yet-wired dispatchMenuAction targets used to render
  // as ordinary enabled buttons that silently no-op'd on click. They now
  // match the Tools/Templates convention (disabled + "soon" badge).
  // Print was on this list until tuxlink-j0m3 wired the handler.
  it.each([
    { menu: 'Session', item: 'Disconnect' },
    { menu: 'Session', item: 'Session Log' },
    { menu: 'Session', item: 'Verify CMS Connection' },
    { menu: 'Session', item: 'Show transport' },
  ])('$menu → $item renders disabled with no action firing', ({ menu, item }) => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: menu }));
    const button = screen.getByRole('button', { name: new RegExp(item) });
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(onAction).not.toHaveBeenCalled();
  });

  // tuxlink-j0m3: Print is wired — enabled, fires the action id. The
  // open-message gate lives in the handler (AppShell), not the menu.
  it('Message → Print is enabled and fires menu:message:print', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'Message' }));
    const print = screen.getByRole('button', { name: /Print/ });
    expect(print).not.toBeDisabled();
    fireEvent.click(print);
    expect(onAction).toHaveBeenCalledWith('menu:message:print');
  });

  it('keeps top-app dropdown layers above message-list scroll content', () => {
    const panesZ = requiredZIndex(
      appShellCss,
      /\.layout-b \.panes\s*\{[^}]*z-index:\s*(\d+);/,
    );
    const ribbonZ = requiredZIndex(
      appShellCss,
      /\.layout-b \.ribbon-with-search\s*\{[^}]*z-index:\s*(\d+);/,
    );
    const menubarZ = requiredZIndex(
      chromeCss,
      /\.tux-menubar\s*\{[^}]*z-index:\s*(\d+);/,
    );

    expect(appShellCss).toMatch(/\.layout-b \.panes\s*\{[^}]*isolation:\s*isolate;/);
    expect(ribbonZ).toBeGreaterThan(panesZ);
    expect(menubarZ).toBeGreaterThan(ribbonZ);
    expect(menubarZ).toBeLessThan(100);
  });
});
