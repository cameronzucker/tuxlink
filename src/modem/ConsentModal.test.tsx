import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ConsentModal } from './ConsentModal';

describe('ConsentModal', () => {
  it('renders the RADIO-1 warning + Cancel/Connect buttons', () => {
    render(<ConsentModal target="W7RMS-10" onCancel={() => {}} onConfirm={() => {}} />);
    expect(screen.getByText(/About to transmit on amateur radio/i)).toBeInTheDocument();
    expect(screen.getByText(/W7RMS-10/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /cancel/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^connect$/i })).toBeInTheDocument();
  });

  it('Connect button is disabled until the acknowledgement checkbox is ticked', () => {
    const onConfirm = vi.fn();
    render(<ConsentModal target="W7RMS-10" onCancel={() => {}} onConfirm={onConfirm} />);
    const connect = screen.getByRole('button', { name: /^connect$/i });
    expect(connect).toBeDisabled();
    fireEvent.click(screen.getByRole('checkbox'));
    expect(connect).not.toBeDisabled();
    fireEvent.click(connect);
    expect(onConfirm).toHaveBeenCalled();
  });

  it('Cancel button calls onCancel without confirming', () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(<ConsentModal target="W7RMS-10" onCancel={onCancel} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalledOnce();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
