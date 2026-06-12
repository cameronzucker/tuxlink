import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
vi.mock('@tauri-apps/api/event', () => ({ listen: () => Promise.resolve(() => {}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue('A1') }));
import { AprsChatPanel } from './AprsChatPanel';

describe('AprsChatPanel', () => {
  it('renders the composer and a listening indicator', () => {
    render(<AprsChatPanel />);
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /send/i })).toBeInTheDocument();
    expect(screen.getByTestId('aprs-listening-indicator')).toBeInTheDocument();
  });

  it('shows the empty-state guidance when no threads', () => {
    render(<AprsChatPanel />);
    expect(screen.getByText(/no conversations yet/i)).toBeInTheDocument();
  });
});
