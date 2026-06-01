import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Ics309Form } from './Ics309Form';

describe('Ics309Form', () => {
  const noop = () => {};

  it('renders all required header fields', () => {
    render(<Ics309Form onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Title/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date\/Time Prepared/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Radio Operator Name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Station ID/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields are empty', () => {
    const onSubmit = vi.fn();
    render(<Ics309Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('ics309-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with values when required fields are filled', () => {
    const onSubmit = vi.fn();
    render(<Ics309Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Title/i), { target: { value: 'Alpha Net' } });
    fireEvent.change(screen.getByLabelText(/Date\/Time Prepared/i), { target: { value: '2026-05-31 09:00Z' } });
    fireEvent.change(screen.getByLabelText(/Radio Operator Name/i), { target: { value: 'W1AW' } });
    fireEvent.change(screen.getByLabelText(/Station ID/i), { target: { value: 'W1AW-1' } });
    fireEvent.click(screen.getByTestId('ics309-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const vals = onSubmit.mock.calls[0][0];
    expect(vals.title).toBe('Alpha Net');
    expect(vals.opname).toBe('W1AW');
  });

  it('calls onChange when a field changes', () => {
    const onChange = vi.fn();
    render(<Ics309Form onChange={onChange} onSubmit={noop} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Title/i), { target: { value: 'Bravo Net' } });
    expect(onChange).toHaveBeenCalled();
    const last = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(last.title).toBe('Bravo Net');
  });
});
