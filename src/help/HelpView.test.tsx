import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { HelpView } from './HelpView';

vi.mock('@tauri-apps/plugin-shell', () => ({
  open: vi.fn(),
}));

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
});
