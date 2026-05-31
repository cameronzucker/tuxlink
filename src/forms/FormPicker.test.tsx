import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FormPicker } from './FormPicker';

describe('FormPicker', () => {
  it('lists all registered forms', () => {
    const forms = [
      { id: 'ICS213_Initial', name: 'ICS-213 General Message' },
      { id: 'ICS309_Initial', name: 'ICS-309 Communications Log' },
    ];
    render(<FormPicker forms={forms} onPick={() => {}} onCancel={() => {}} />);
    expect(screen.getByText('ICS-213 General Message')).toBeInTheDocument();
    expect(screen.getByText('ICS-309 Communications Log')).toBeInTheDocument();
  });

  it('calls onPick with the selected form id', () => {
    const onPick = vi.fn();
    const forms = [{ id: 'ICS213_Initial', name: 'ICS-213 General Message' }];
    render(<FormPicker forms={forms} onPick={onPick} onCancel={() => {}} />);
    fireEvent.click(screen.getByText('ICS-213 General Message'));
    fireEvent.click(screen.getByTestId('form-picker-confirm'));
    expect(onPick).toHaveBeenCalledWith('ICS213_Initial');
  });

  it('calls onCancel when cancel button clicked', () => {
    const onCancel = vi.fn();
    render(<FormPicker forms={[]} onPick={() => {}} onCancel={onCancel} />);
    fireEvent.click(screen.getByTestId('form-picker-cancel'));
    expect(onCancel).toHaveBeenCalled();
  });
});
