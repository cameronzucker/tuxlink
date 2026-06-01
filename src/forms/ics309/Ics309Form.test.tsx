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

  // Codex r2 P2 #2: when restoring a saved draft, scan all 4 entry fields
  // across all 30 rows so populated entries 11-30 (and rows with from/to/sub
  // but no time) are visible to the operator before submit.
  it('restores entryCount to the highest populated row across all 4 fields (1..30)', () => {
    render(
      <Ics309Form
        initialValues={{ title: 't', from17: 'KK4OBN', to17: 'KK4XYZ' }}
        onSubmit={noop}
        onCancel={noop}
      />,
    );
    // Row 17 should be visible (the "from17" / "to17" inputs are rendered).
    expect(screen.getByLabelText(/From #17/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/To #17/i)).toBeInTheDocument();
  });

  it('restores entryCount to 30 when the last row carries any data', () => {
    render(
      <Ics309Form
        initialValues={{ title: 't', sub30: 'final log entry' }}
        onSubmit={noop}
        onCancel={noop}
      />,
    );
    expect(screen.getByLabelText(/Subject #30/i)).toBeInTheDocument();
  });

  it('starts with 5 visible rows when no entry fields are populated', () => {
    render(<Ics309Form initialValues={{ title: 't' }} onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Time #5/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/Time #6/i)).toBeNull();
  });
});
