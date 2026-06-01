import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BulletinForm } from './BulletinForm';

describe('BulletinForm', () => {
  const noop = () => {};

  it('renders all bulletin fields', () => {
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Precedence Level/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Subject/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Bulletin #/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/For.*Recipient/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Bulletin From/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date\/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Message/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields are empty', () => {
    const onSubmit = vi.fn();
    render(<BulletinForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('bulletin-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with values when required fields filled', () => {
    const onSubmit = vi.fn();
    render(<BulletinForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Precedence Level/i), { target: { value: 'ROUTINE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'Net schedule update' } });
    fireEvent.change(screen.getByLabelText(/Bulletin #/i), { target: { value: '42' } });
    fireEvent.change(screen.getByLabelText(/For.*Recipient/i), { target: { value: 'ALL' } });
    fireEvent.change(screen.getByLabelText(/Bulletin From/i), { target: { value: 'W1AW' } });
    fireEvent.change(screen.getByLabelText(/Date\/Time/i), { target: { value: '2026-05-31 09:00Z' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'Net moved to 0930 local.' } });
    fireEvent.click(screen.getByTestId('bulletin-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const vals = onSubmit.mock.calls[0][0];
    expect(vals.level).toBe('ROUTINE');
    expect(vals.bullnr).toBe('42');
    expect(vals.message).toBe('Net moved to 0930 local.');
  });
});
