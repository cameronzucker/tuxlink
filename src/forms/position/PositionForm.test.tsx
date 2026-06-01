import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { PositionForm } from './PositionForm';

describe('PositionForm', () => {
  const noop = () => {};

  it('renders all position fields', () => {
    render(<PositionForm onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Latitude/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Longitude/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Comment/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields are empty', () => {
    const onSubmit = vi.fn();
    render(<PositionForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('position-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with coordinates when required fields are filled', () => {
    const onSubmit = vi.fn();
    render(<PositionForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Time/i), { target: { value: '14:00Z' } });
    fireEvent.change(screen.getByLabelText(/Latitude/i), { target: { value: '38.889484' } });
    fireEvent.change(screen.getByLabelText(/Longitude/i), { target: { value: '-77.035278' } });
    fireEvent.click(screen.getByTestId('position-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const vals = onSubmit.mock.calls[0][0];
    expect(vals.lat).toBe('38.889484');
    expect(vals.lon).toBe('-77.035278');
  });
});
