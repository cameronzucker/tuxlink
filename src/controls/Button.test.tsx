import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Button } from './Button';

describe('<Button>', () => {
  it('maps tone/emphasis/size to controls.css classes', () => {
    render(<Button tone="primary" emphasis="soft" size="md">Send</Button>);
    const btn = screen.getByRole('button', { name: 'Send' });
    expect(btn.className).toBe('tux-btn tux-btn--primary tux-btn--soft tux-btn--md');
  });

  it('defaults to neutral / solid / md', () => {
    render(<Button>Go</Button>);
    expect(screen.getByRole('button', { name: 'Go' }).className)
      .toBe('tux-btn tux-btn--neutral tux-btn--solid tux-btn--md');
  });

  it('forwards native attributes and events', () => {
    const onClick = vi.fn();
    render(<Button data-testid="x" disabled onClick={onClick}>Z</Button>);
    const btn = screen.getByTestId('x');
    expect(btn).toBeDisabled();
    fireEvent.click(btn);
    expect(onClick).not.toHaveBeenCalled(); // disabled swallows the click
  });

  it('merges a caller-supplied className', () => {
    render(<Button className="dash-connect">C</Button>);
    expect(screen.getByRole('button', { name: 'C' }).className)
      .toContain('dash-connect');
  });
});
