import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HelpView } from './HelpView';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

beforeEach(() => {
  localStorage.clear();
  document.documentElement.style.removeProperty('--help-font-size');
});

describe('HelpView', () => {
  it('renders the layout skeleton', () => {
    render(<HelpView />);
    expect(screen.getByTestId('tux-help-root')).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: /help topics/i })).toBeInTheDocument();
    expect(screen.getByRole('main')).toBeInTheDocument();
  });

  it('opens to the first topic by default', () => {
    render(<HelpView />);
    expect(screen.getByRole('heading', { level: 1, name: /getting started/i })).toBeInTheDocument();
  });

  it('renders the header strip with the User Guide title', () => {
    render(<HelpView />);
    expect(screen.getByText(/User Guide/)).toBeInTheDocument();
  });

  it('renders the text-size dropdown in the header', () => {
    render(<HelpView />);
    expect(screen.getByRole('button', { name: /Text size:/ })).toBeInTheDocument();
  });
});

describe('HelpView text-size keyboard shortcuts', () => {
  it('Ctrl+= steps the size up', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });
    // The button label updates to reflect the new preset.
    expect(screen.getByText('Large')).toBeInTheDocument();
  });

  it('Ctrl+- steps the size down (saturates at Normal)', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '-', ctrlKey: true });
    // Saturates: still Normal.
    expect(screen.getByText('Normal')).toBeInTheDocument();
  });

  it('Ctrl+0 resets to Normal from any tier', () => {
    render(<HelpView />);
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → Large
    fireEvent.keyDown(window, { key: '=', ctrlKey: true });  // → X-Large
    fireEvent.keyDown(window, { key: '0', ctrlKey: true });  // → Normal
    expect(screen.getByText('Normal')).toBeInTheDocument();
  });
});
