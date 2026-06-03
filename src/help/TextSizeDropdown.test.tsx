import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { TextSizeDropdown } from './TextSizeDropdown';

describe('TextSizeDropdown', () => {
  it('renders the button labeled with the current preset', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    expect(screen.getByRole('button', { name: /Text size:/ })).toBeInTheDocument();
    expect(screen.getByText('Normal')).toBeInTheDocument();
  });

  it('opens the menu and lists all four presets', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    const items = screen.getAllByRole('menuitem');
    const labels = items.map((i) => i.textContent?.replace('✓', '').trim());
    expect(labels).toEqual(['Normal', 'Large', 'X-Large', 'Huge']);
  });

  it('marks the active preset with aria-checked', () => {
    render(<TextSizeDropdown value="Large" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    const items = screen.getAllByRole('menuitem');
    const active = items.find((i) => i.getAttribute('aria-checked') === 'true');
    expect(active?.textContent).toContain('Large');
  });

  it('calls onChange with the selected preset and closes', () => {
    const onChange = vi.fn();
    render(<TextSizeDropdown value="Normal" onChange={onChange} />);
    fireEvent.click(screen.getByRole('button'));
    // Click the menuitem whose textContent starts with "X-Large".
    const items = screen.getAllByRole('menuitem');
    const xlarge = items.find((i) => i.textContent?.trim().startsWith('X-Large'));
    expect(xlarge, 'X-Large menuitem present').not.toBeUndefined();
    fireEvent.click(xlarge!);
    expect(onChange).toHaveBeenCalledWith('X-Large');
    // Menu should close.
    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument();
  });

  it('closes the menu on Escape', () => {
    render(<TextSizeDropdown value="Normal" onChange={() => {}} />);
    fireEvent.click(screen.getByRole('button'));
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menuitem')).not.toBeInTheDocument();
  });
});
