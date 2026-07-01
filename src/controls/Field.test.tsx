import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Field } from './Field';

describe('<Field>', () => {
  it('renders a .tux-field input', () => {
    render(<Field aria-label="Cmd port" defaultValue="8515" />);
    expect(screen.getByLabelText('Cmd port').className).toContain('tux-field');
  });
  it('associates a visible label with the input', () => {
    render(<Field label="Cmd port" id="cmd" defaultValue="8515" />);
    expect(screen.getByLabelText('Cmd port')).toHaveValue('8515');
  });
});
