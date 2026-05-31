import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChipStrip } from './ChipStrip';
import { EMPTY_SPEC } from './types';

describe('ChipStrip', () => {
  it('renders the empty-state placeholder + all ghost chips when no filters', () => {
    render(<ChipStrip spec={EMPTY_SPEC} onSpecChange={() => {}} metaText={null} />);
    expect(screen.getByTestId('chipstrip-empty')).toBeInTheDocument();
    expect(screen.getAllByTestId(/^chip-ghost-/)).toHaveLength(9);
  });

  it('renders an active chip with an × that removes it', () => {
    const onSpecChange = vi.fn();
    render(<ChipStrip
      spec={{ ...EMPTY_SPEC, filters: { from: { kind: 'addr', value: 'KX5DD' } } }}
      onSpecChange={onSpecChange}
      metaText={null}
    />);
    expect(screen.getByTestId('chip-active-from')).toHaveTextContent('FROM:KX5DD');
    fireEvent.click(screen.getByTestId('chip-x-from'));
    expect(onSpecChange).toHaveBeenCalledWith(expect.objectContaining({ filters: {} }));
  });

  it('renders meta-text on the far right', () => {
    render(<ChipStrip spec={EMPTY_SPEC} onSpecChange={() => {}} metaText="3 matches · 47 ms · ★ Storm Net" />);
    expect(screen.getByTestId('chipstrip-meta')).toHaveTextContent('3 matches · 47 ms · ★ Storm Net');
  });
});
