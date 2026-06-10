import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { Icon } from './icons';

describe('<Icon>', () => {
  it('renders an svg element for a known icon name', () => {
    const { container } = render(<Icon name="weather" />);
    const svg = container.querySelector('svg');
    expect(svg).not.toBeNull();
  });

  it('applies the default size of 18 to width and height', () => {
    const { container } = render(<Icon name="weather" />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('width')).toBe('18');
    expect(svg.getAttribute('height')).toBe('18');
  });

  it('honors a custom size prop', () => {
    const { container } = render(<Icon name="pin" size={15} />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('width')).toBe('15');
    expect(svg.getAttribute('height')).toBe('15');
  });

  it('renders at least one path for the weather icon', () => {
    const { container } = render(<Icon name="weather" />);
    const paths = container.querySelectorAll('path, circle');
    expect(paths.length).toBeGreaterThan(0);
  });

  it('passes a className through to the svg element', () => {
    const { container } = render(<Icon name="arrow" className="lead" />);
    const svg = container.querySelector('svg')!;
    expect(svg.classList.contains('lead')).toBe(true);
  });

  it('sets aria-hidden on the svg', () => {
    const { container } = render(<Icon name="close" />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('aria-hidden')).toBe('true');
  });

  it('uses currentColor stroke and fill none', () => {
    const { container } = render(<Icon name="sun" />);
    const svg = container.querySelector('svg')!;
    expect(svg.getAttribute('stroke')).toBe('currentColor');
    expect(svg.getAttribute('fill')).toBe('none');
  });
});
