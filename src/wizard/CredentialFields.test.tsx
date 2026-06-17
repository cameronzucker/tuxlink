// CredentialFields tests (tuxlink-vfb3). The shared callsign + password input
// pair (with show/hide toggle) extracted from Step2Credentials so the wizard and
// the Settings "Winlink Account" re-enter form render identical fields. Controlled
// component: callsign/password come from props; only the mask toggle is internal.

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';

import { CredentialFields } from './CredentialFields';

describe('<CredentialFields>', () => {
  it('renders labelled callsign and password inputs', () => {
    render(
      <CredentialFields
        callsign=""
        password=""
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/callsign/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/cms password/i)).toBeInTheDocument();
  });

  it('masks the password by default and the toggle flips its type', () => {
    render(
      <CredentialFields
        callsign=""
        password="secret"
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
      />,
    );
    const pw = screen.getByLabelText(/cms password/i) as HTMLInputElement;
    expect(pw.type).toBe('password');
    const toggle = screen.getByRole('button', { name: /conceal|reveal/i });
    fireEvent.click(toggle);
    expect((screen.getByLabelText(/cms password/i) as HTMLInputElement).type).toBe('text');
    fireEvent.click(toggle);
    expect((screen.getByLabelText(/cms password/i) as HTMLInputElement).type).toBe('password');
  });

  it('reflects controlled values from props', () => {
    render(
      <CredentialFields
        callsign="W4PHS"
        password="hunter2x"
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
      />,
    );
    expect((screen.getByLabelText(/callsign/i) as HTMLInputElement).value).toBe('W4PHS');
    expect((screen.getByLabelText(/cms password/i) as HTMLInputElement).value).toBe('hunter2x');
  });

  it('fires onChange callbacks with the typed value', () => {
    const onCallsignChange = vi.fn();
    const onPasswordChange = vi.fn();
    render(
      <CredentialFields
        callsign=""
        password=""
        onCallsignChange={onCallsignChange}
        onPasswordChange={onPasswordChange}
      />,
    );
    fireEvent.change(screen.getByLabelText(/callsign/i), { target: { value: 'K7ABC' } });
    fireEvent.change(screen.getByLabelText(/cms password/i), { target: { value: 'pw' } });
    expect(onCallsignChange).toHaveBeenCalledWith('K7ABC');
    expect(onPasswordChange).toHaveBeenCalledWith('pw');
  });

  it('fires onCallsignBlur when the callsign loses focus', () => {
    const onCallsignBlur = vi.fn();
    render(
      <CredentialFields
        callsign="W4PHS"
        password=""
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
        onCallsignBlur={onCallsignBlur}
      />,
    );
    fireEvent.blur(screen.getByLabelText(/callsign/i));
    expect(onCallsignBlur).toHaveBeenCalledOnce();
  });

  it('shows the callsign field error with an alert role when provided', () => {
    render(
      <CredentialFields
        callsign="bad call"
        password=""
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
        callsignError="Callsign must contain no internal whitespace."
      />,
    );
    const err = screen.getByRole('alert');
    expect(err).toHaveTextContent(/internal whitespace/i);
  });

  it('disables both inputs and the toggle when disabled', () => {
    render(
      <CredentialFields
        callsign=""
        password=""
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
        disabled
      />,
    );
    expect(screen.getByLabelText(/callsign/i)).toBeDisabled();
    expect(screen.getByLabelText(/cms password/i)).toBeDisabled();
    expect(screen.getByRole('button', { name: /conceal|reveal/i })).toBeDisabled();
  });

  it('namespaces input ids by idPrefix so two instances can coexist', () => {
    const { container } = render(
      <CredentialFields
        idPrefix="reenter"
        callsign=""
        password=""
        onCallsignChange={vi.fn()}
        onPasswordChange={vi.fn()}
      />,
    );
    expect(container.querySelector('#reenter-callsign')).not.toBeNull();
    expect(container.querySelector('#reenter-password')).not.toBeNull();
  });
});
