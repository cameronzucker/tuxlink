import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { WeatherGlyph } from './WeatherGlyph';

describe('WeatherGlyph (tuxlink-n6tp)', () => {
  it('renders an accessible SVG labelled with the decoded condition', () => {
    render(<WeatherGlyph code="Ptcldy" />);
    const img = screen.getByRole('img', { name: 'Partly cloudy' });
    expect(img.tagName.toLowerCase()).toBe('svg');
  });

  it('shows the decoded word, never the raw code, on a mapped glyph', () => {
    render(<WeatherGlyph code="Vryhot" />);
    expect(screen.getByRole('img', { name: 'Very hot' })).toBeTruthy();
    expect(screen.queryByText('Vryhot')).toBeNull();
  });

  it('falls back to raw text with the legacy heat class for an unmapped code', () => {
    const { container } = render(<WeatherGlyph code="Funky" />);
    expect(screen.getByText('Funky')).toBeTruthy();
    expect(screen.queryByRole('img')).toBeNull();
    expect(container.querySelector('.cond')).toBeTruthy();
  });

  it('carries the danger heat accent into the Vryhot sun (class on the sun group)', () => {
    const { container } = render(<WeatherGlyph code="Vryhot" />);
    expect(container.querySelector('.wx-danger')).toBeTruthy();
  });
});
