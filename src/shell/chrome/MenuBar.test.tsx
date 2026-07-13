import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MenuBar } from './MenuBar';
import { menuAnchorId } from '../../onboarding/menuAnchors';

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
  // tuxlink-lqw2: the Session + Mailbox top menus were removed in the pre-Alpha
  // declutter (connect on the ribbon; folder nav in the FolderSidebar).
  it('renders the five top menus', () => {
    render(<MenuBar onAction={vi.fn()} />);
    for (const label of ['File', 'Message', 'View', 'Tools', 'Help']) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
  });

  it('no longer offers the Session or Mailbox top menus', () => {
    render(<MenuBar onAction={vi.fn()} />);
    expect(screen.queryByRole('button', { name: 'Session' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Mailbox' })).not.toBeInTheDocument();
  });

  // tuxlink-lqw2: Verify CMS Connection relocated into Tools and is now an
  // enabled action (wired to the inline probe overlay), not a "soon" stub.
  it('Tools → Verify CMS Connection is enabled and fires menu:tools:verify_cms', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    const verify = screen.getByRole('button', { name: /Verify CMS Connection/ });
    expect(verify).not.toBeDisabled();
    fireEvent.click(verify);
    expect(onAction).toHaveBeenCalledWith('menu:tools:verify_cms');
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

  it('offers the practical dark color schemes in the View menu', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('button', { name: 'Repository Dark' }));
    expect(onAction).toHaveBeenCalledWith('menu:view:scheme:github-dark');
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    fireEvent.click(screen.getByRole('button', { name: 'Office dark' }));
    expect(onAction).toHaveBeenCalledWith('menu:view:scheme:office-dark');
  });

  // tuxlink-esb65: the "renders a disabled/badged not-yet-wired item" test was
  // removed with the last disabled stub (Tools → Templates). The MenuBar's
  // disabled-item rendering path remains in code; there is simply no live
  // disabled entry to exercise it against now. Re-add coverage if one returns.

  // tuxlink-39b: "Preferences" removed as a duplicate of "Settings".
  it('no longer offers a Preferences item', () => {
    render(<MenuBar onAction={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Tools' }));
    expect(screen.queryByRole('button', { name: /Preferences/ })).not.toBeInTheDocument();
  });

  // tuxlink-d9ry: Print lives in File, still firing the existing message-print
  // action id; the open-message gate lives in the handler (AppShell).
  it('File → Print is enabled and fires menu:message:print', () => {
    const onAction = vi.fn();
    render(<MenuBar onAction={onAction} />);
    fireEvent.click(screen.getByRole('button', { name: 'File' }));
    const print = screen.getByRole('button', { name: /Print/ });
    expect(print).not.toBeDisabled();
    fireEvent.click(print);
    expect(onAction).toHaveBeenCalledWith('menu:message:print');
  });

  it('Message no longer includes Print', () => {
    render(<MenuBar onAction={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Message' }));
    expect(screen.queryByRole('button', { name: /Print/ })).not.toBeInTheDocument();
  });

  // tuxlink-10bkw Task 9: Elmer's point_at tool highlights menu chrome — the
  // top-level button and, once opened, the items inside it — via
  // data-tour-anchor stamped from menuAnchors.ts (menuAnchorId for the
  // top-level button, the item's own MenuActionId verbatim for leaves).
  it('stamps data-tour-anchor on the top-level Tools button and its open menu items', () => {
    render(<MenuBar onAction={vi.fn()} />);
    const toolsButton = screen.getByRole('button', { name: 'Tools' });
    expect(toolsButton).toHaveAttribute('data-tour-anchor', menuAnchorId('Tools'));

    // The item-level anchor only exists in the DOM once the menu is open.
    expect(screen.queryByRole('button', { name: /Settings/ })).not.toBeInTheDocument();
    fireEvent.click(toolsButton);
    const settings = screen.getByRole('button', { name: /Settings/ });
    expect(settings).toHaveAttribute('data-tour-anchor', 'menu:tools:settings');
  });

  it('stamps data-tour-anchor on a nested submenu leaf (View → Color scheme → Night)', () => {
    render(<MenuBar onAction={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'View' }));
    const night = screen.getByRole('button', { name: /Night . tactical/ });
    expect(night).toHaveAttribute('data-tour-anchor', 'menu:view:scheme:night-red');
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
