// src/radio/charts/Sparkline.test.tsx
//
// Spec §5.3 — pure-SVG / div-based sparkline used by both the Live
// section's throughput trace and the Signal section's S/N trace.
// These tests cover the structural contract (one bar per sample,
// height scales to range, threshold classes apply correctly).

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Sparkline } from './Sparkline';

describe('<Sparkline>', () => {
  it('renders one bar per sample', () => {
    render(<Sparkline samples={[1, 2, 3, 4, 5]} />);
    const container = screen.getByTestId('sparkline');
    expect(container.children).toHaveLength(5);
  });

  it('applies warn class above warnAbove threshold', () => {
    render(<Sparkline samples={[1, 5, 10]} max={10} warnAbove={4} />);
    const container = screen.getByTestId('sparkline');
    expect(container.children[0].className).not.toMatch(/warn/);
    expect(container.children[1].className).toMatch(/warn/);
    expect(container.children[2].className).toMatch(/warn/);
  });

  it('applies bad class above badAbove threshold (takes priority over warn)', () => {
    render(
      <Sparkline samples={[1, 5, 9]} max={10} warnAbove={4} badAbove={7} />,
    );
    const container = screen.getByTestId('sparkline');
    expect(container.children[0].className).not.toMatch(/(warn|bad)/);
    expect(container.children[1].className).toMatch(/warn/);
    expect(container.children[1].className).not.toMatch(/bad/);
    expect(container.children[2].className).toMatch(/bad/);
  });

  it('applies warn class below warnBelow threshold (for low-is-bad cases)', () => {
    render(<Sparkline samples={[10, 4, 1]} max={10} warnBelow={5} badBelow={2} />);
    const container = screen.getByTestId('sparkline');
    expect(container.children[0].className).not.toMatch(/(warn|bad)/);
    expect(container.children[1].className).toMatch(/warn/);
    expect(container.children[2].className).toMatch(/bad/);
  });

  it('scales bar heights to fit the sample range', () => {
    render(<Sparkline samples={[0, 50, 100]} min={0} max={100} />);
    const container = screen.getByTestId('sparkline');
    // Smallest sample → minimum visible height (2%); largest → 100%.
    const last = container.children[2] as HTMLElement;
    const first = container.children[0] as HTMLElement;
    expect(last.style.height).toBe('100%');
    // 0 maps to the floor (`Math.max(2, 0)` = 2%).
    expect(first.style.height).toBe('2%');
  });

  it('uses provided height prop on the container', () => {
    render(<Sparkline samples={[1, 2]} height={56} />);
    const container = screen.getByTestId('sparkline');
    expect(container.style.height).toBe('56px');
  });

  it('handles an empty samples array without crashing', () => {
    render(<Sparkline samples={[]} />);
    const container = screen.getByTestId('sparkline');
    expect(container.children).toHaveLength(0);
  });
});
