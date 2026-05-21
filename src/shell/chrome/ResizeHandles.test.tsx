import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/react';

const win = vi.hoisted(() => ({ startResizeDragging: vi.fn(async () => {}) }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => win,
  ResizeDirection: {
    North: 'North', South: 'South', East: 'East', West: 'West',
    NorthEast: 'NorthEast', NorthWest: 'NorthWest', SouthEast: 'SouthEast', SouthWest: 'SouthWest',
  },
}));

import { ResizeHandles } from './ResizeHandles';

describe('ResizeHandles', () => {
  beforeEach(() => win.startResizeDragging.mockClear());

  it('renders eight handles', () => {
    const { container } = render(<ResizeHandles />);
    expect(container.querySelectorAll('.tux-resize').length).toBe(8);
  });

  it('starts a resize-drag in the handle direction on mousedown', () => {
    const { container } = render(<ResizeHandles />);
    const se = container.querySelector('.tux-resize.se')!;
    fireEvent.mouseDown(se);
    expect(win.startResizeDragging).toHaveBeenCalledWith('SouthEast');
  });
});
