import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Ics213Form } from './Ics213Form';

describe('Ics213Form', () => {
  const noop = () => {};

  it('renders all ICS-213 input fields', () => {
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Incident Name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/To.*Name and Position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/From.*Name and Position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Subject/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Message/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Approved by/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Is exercise/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields empty', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with field values when required fields filled', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/To.*Name and Position/i), { target: { value: 'JOHN' } });
    fireEvent.change(screen.getByLabelText(/From.*Name and Position/i), { target: { value: 'JANE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'TEST' } });
    fireEvent.change(screen.getByLabelText(/Date/i), { target: { value: '2026-05-30' } });
    fireEvent.change(screen.getByLabelText(/Time/i), { target: { value: '14:30Z' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'hello' } });
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const values = onSubmit.mock.calls[0][0];
    expect(values.to_name).toBe('JOHN');
    expect(values.fm_name).toBe('JANE');
    expect(values.subjectline).toBe('TEST');
    expect(values.message).toBe('hello');
  });

  it('initialValues pre-populates fields', () => {
    render(<Ics213Form initialValues={{ inc_name: 'WALDO' }} onSubmit={noop} onCancel={noop} />);
    const incName = screen.getByLabelText(/Incident Name/i) as HTMLInputElement;
    expect(incName.value).toBe('WALDO');
  });

  it('calls onChange when a field is edited (controlled host pattern)', () => {
    const onChange = vi.fn();
    render(<Ics213Form onChange={onChange} onSubmit={noop} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Incident Name/i), { target: { value: 'WALDO' } });
    expect(onChange).toHaveBeenCalled();
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(lastCall.inc_name).toBe('WALDO');
  });
});
