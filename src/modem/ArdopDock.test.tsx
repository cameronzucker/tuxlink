import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ArdopDock } from './ArdopDock';
import { STOPPED } from './types';

vi.mock('./useModemStatus', () => ({
  useModemStatus: () => ({ status: STOPPED, loading: false }),
}));

describe('<ArdopDock> stopped', () => {
  it('renders the Connect form when status.state === stopped', () => {
    render(<ArdopDock />);
    expect(screen.getByTestId('ardop-dock-root')).toBeInTheDocument();
    expect(screen.getByLabelText(/target callsign/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /connect/i })).toBeInTheDocument();
  });
});
