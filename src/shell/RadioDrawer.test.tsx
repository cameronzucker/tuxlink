import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RadioDrawer } from './RadioDrawer';

// F9 — CSS scoping guard: a display:flex/block leak OUT of the @media block in
// RadioDrawer.css would make the panel a flex box at desktop. jsdom can't apply
// media queries, so assert the source structure: the three desktop rules are
// display:contents/none and live BEFORE the @media; flex/block only appear
// inside it.
const RADIO_DRAWER_CSS = import.meta.glob('./RadioDrawer.css', {
  eager: true,
  query: '?raw',
  import: 'default',
}) as Record<string, string>;
const drawerCss = RADIO_DRAWER_CSS['./RadioDrawer.css'];

describe('RadioDrawer', () => {
  it('renders its children (the radio panel) and a grip handle', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div data-testid="panel-child">panel</div>
      </RadioDrawer>,
    );
    expect(screen.getByTestId('panel-child')).toBeInTheDocument();
    expect(screen.getByTestId('radio-drawer-grip')).toBeInTheDocument();
  });

  it('reflects open state via the is-open class for CSS to animate', () => {
    const { container, rerender } = render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    expect(container.querySelector('.radio-drawer')?.classList.contains('is-open')).toBe(false);
    rerender(
      <RadioDrawer open={true} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    expect(container.querySelector('.radio-drawer')?.classList.contains('is-open')).toBe(true);
  });

  it('fires onToggle when the grip is tapped', () => {
    const onToggle = vi.fn();
    render(
      <RadioDrawer open={false} onToggle={onToggle} sessionState="connecting">
        <div />
      </RadioDrawer>,
    );
    fireEvent.click(screen.getByTestId('radio-drawer-grip'));
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it('surfaces session state on the grip (data attribute) for the tick styling', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="connected">
        <div />
      </RadioDrawer>,
    );
    expect(screen.getByTestId('radio-drawer-grip').getAttribute('data-session-state')).toBe(
      'connected',
    );
  });

  it('grip is an accessible toggle button', () => {
    render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <div />
      </RadioDrawer>,
    );
    const grip = screen.getByTestId('radio-drawer-grip');
    expect(grip.tagName).toBe('BUTTON');
    expect(grip).toHaveAttribute('aria-expanded', 'false');
    expect(grip).toHaveAttribute('aria-label');
  });

  it('moves focus into the panel body on open and back to the grip on close (F10)', () => {
    const { rerender } = render(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <button data-testid="panel-btn">dial</button>
      </RadioDrawer>,
    );
    rerender(
      <RadioDrawer open={true} onToggle={() => {}} sessionState="disconnected">
        <button data-testid="panel-btn">dial</button>
      </RadioDrawer>,
    );
    // body (tabindex -1) receives focus on open
    expect(document.activeElement?.classList.contains('radio-drawer-body')).toBe(true);
    rerender(
      <RadioDrawer open={false} onToggle={() => {}} sessionState="disconnected">
        <button data-testid="panel-btn">dial</button>
      </RadioDrawer>,
    );
    expect(document.activeElement).toBe(screen.getByTestId('radio-drawer-grip'));
  });
});

describe('RadioDrawer.css desktop scoping guard (F9)', () => {
  it('keeps the three desktop rules as display:contents/none before the @media', () => {
    const head = drawerCss.slice(0, drawerCss.indexOf('@media (max-width: 1365px)'));
    expect(head).toContain('.radio-drawer {\n  display: contents;');
    expect(head).toContain('.radio-drawer-body {\n  display: contents;');
    expect(head).toContain('.radio-drawer-grip {\n  display: none;');
    // No flex/block layout escapes to desktop.
    expect(head).not.toContain('display: flex');
    expect(head).not.toContain('display: block');
  });
});
