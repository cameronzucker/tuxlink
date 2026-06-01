import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { FormPicker } from './FormPicker';

const TWO_FORMS = [
  { id: 'ICS213_Initial', name: 'ICS-213 General Message' },
  { id: 'ICS309_Initial', name: 'ICS-309 Communications Log' },
];

describe('FormPicker', () => {
  it('lists all registered forms', () => {
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={() => {}} />);
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

  it('exposes the list as an ARIA listbox with options', () => {
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={() => {}} />);
    const listbox = screen.getByRole('listbox');
    expect(listbox).toBeInTheDocument();
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(2);
    expect(options[0]).toHaveAttribute('aria-selected', 'true');
    expect(options[1]).toHaveAttribute('aria-selected', 'false');
  });

  it('moves selection with ArrowDown / ArrowUp', () => {
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={() => {}} />);
    const listbox = screen.getByRole('listbox');
    fireEvent.keyDown(listbox, { key: 'ArrowDown' });
    let options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'false');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(listbox, { key: 'ArrowUp' });
    options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'true');
    expect(options[1]).toHaveAttribute('aria-selected', 'false');
  });

  it('clamps selection at the ends with ArrowUp/ArrowDown', () => {
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={() => {}} />);
    const listbox = screen.getByRole('listbox');
    fireEvent.keyDown(listbox, { key: 'ArrowUp' });
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(listbox, { key: 'ArrowDown' });
    fireEvent.keyDown(listbox, { key: 'ArrowDown' });
    expect(screen.getAllByRole('option')[1]).toHaveAttribute('aria-selected', 'true');
  });

  it('jumps to first/last with Home/End', () => {
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={() => {}} />);
    const listbox = screen.getByRole('listbox');
    fireEvent.keyDown(listbox, { key: 'End' });
    expect(screen.getAllByRole('option')[1]).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(listbox, { key: 'Home' });
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');
  });

  it('commits selection on Enter', () => {
    const onPick = vi.fn();
    render(<FormPicker forms={TWO_FORMS} onPick={onPick} onCancel={() => {}} />);
    const listbox = screen.getByRole('listbox');
    fireEvent.keyDown(listbox, { key: 'ArrowDown' });
    fireEvent.keyDown(listbox, { key: 'Enter' });
    expect(onPick).toHaveBeenCalledWith('ICS309_Initial');
  });

  it('cancels on Escape', () => {
    const onCancel = vi.fn();
    render(<FormPicker forms={TWO_FORMS} onPick={() => {}} onCancel={onCancel} />);
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' });
    expect(onCancel).toHaveBeenCalled();
  });

  it('commits selection on double-click', () => {
    const onPick = vi.fn();
    render(<FormPicker forms={TWO_FORMS} onPick={onPick} onCancel={() => {}} />);
    fireEvent.doubleClick(screen.getByText('ICS-309 Communications Log'));
    expect(onPick).toHaveBeenCalledWith('ICS309_Initial');
  });
});
