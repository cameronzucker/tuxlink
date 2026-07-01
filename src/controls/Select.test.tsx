import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Select } from './Select';

describe('<Select>', () => {
  it('renders a .tux-select with its options', () => {
    render(
      <Select aria-label="Mode" defaultValue="PKTUSB">
        <option value="PKTUSB">PKTUSB</option>
      </Select>,
    );
    const sel = screen.getByLabelText('Mode');
    expect(sel.className).toContain('tux-select');
    expect(sel).toHaveValue('PKTUSB');
  });
});
