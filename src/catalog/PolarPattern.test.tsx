import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { PolarPattern } from './PolarPattern';

describe('PolarPattern', () => {
  it('renders an svg with a lobe and a peak marker for a shaped pattern', () => {
    const gains = Array.from({ length: 91 }, (_, i) => (i >= 45 ? 3 : -10));
    const { container } = render(<PolarPattern gainsDbi={gains} peakElevationDeg={90} />);
    expect(container.querySelector('svg')).toBeTruthy();
    expect(container.querySelector('[data-testid="lobe"]')).toBeTruthy();
    expect(container.querySelector('[data-testid="polar-peak"]')).toBeTruthy();
    // A shaped pattern is NOT flagged "not modeled".
    expect(container.querySelector('[data-testid="polar-flat"]')).toBeNull();
  });

  it('shows the "not modeled" state for a flat (neutral) pattern', () => {
    const gains = Array.from({ length: 91 }, () => 0);
    const { container, getByText } = render(<PolarPattern gainsDbi={gains} peakElevationDeg={0} />);
    expect(container.querySelector('[data-testid="polar-flat"]')).toBeTruthy();
    expect(getByText(/not modeled/i)).toBeTruthy();
    // No peak marker on a flat pattern (there is no meaningful main lobe).
    expect(container.querySelector('[data-testid="polar-peak"]')).toBeNull();
  });
});
