import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { DamageAssessmentForm } from './DamageAssessmentForm';

describe('DamageAssessmentForm', () => {
  const noop = () => {};

  it('renders required header fields and at least one category group', () => {
    render(<DamageAssessmentForm onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Status/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Jurisdiction/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Survey Area/i)).toBeInTheDocument();
    // At least one category input is present (Houses Affected is first)
    expect(screen.getAllByLabelText(/Affected/i).length).toBeGreaterThan(0);
  });

  it('blocks submit when required fields are empty', () => {
    const onSubmit = vi.fn();
    render(<DamageAssessmentForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('damage-assessment-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with values when required header fields are filled', () => {
    const onSubmit = vi.fn();
    render(<DamageAssessmentForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Status/i), { target: { value: 'FINAL' } });
    fireEvent.change(screen.getByLabelText(/Jurisdiction/i), { target: { value: 'Springfield County' } });
    fireEvent.change(screen.getByLabelText(/Survey Area/i), { target: { value: 'North District' } });
    fireEvent.click(screen.getByTestId('damage-assessment-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const vals = onSubmit.mock.calls[0][0];
    expect(vals.status).toBe('FINAL');
    expect(vals.jur).toBe('Springfield County');
    expect(vals.surarea).toBe('North District');
  });

  it('calls onChange when a field changes', () => {
    const onChange = vi.fn();
    render(<DamageAssessmentForm onChange={onChange} onSubmit={noop} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Status/i), { target: { value: 'PRELIMINARY' } });
    expect(onChange).toHaveBeenCalled();
    const last = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(last.status).toBe('PRELIMINARY');
  });
});
