// CmsPasswordChange tests (tuxlink-vfb3). The in-wizard CMS account
// password-rotation control. Gated on cms_password_change_available() so the
// open build (no access code) never shows a dead form; new+confirm validation;
// verbatim backend error surfacing; success state.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { createElement } from 'react';

import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const invokeMock = invoke as ReturnType<typeof vi.fn>;

import { CmsPasswordChange } from './CmsPasswordChange';

/** Route invoke: availability gate + the change call. */
function route(opts: { available?: boolean; change?: () => Promise<unknown> } = {}) {
  const available = opts.available ?? true;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'cms_password_change_available') return Promise.resolve(available);
    if (cmd === 'cms_password_change') {
      return opts.change ? opts.change() : Promise.resolve(undefined);
    }
    return Promise.resolve(undefined);
  });
}

function renderControl(callsign = 'N7CPZ') {
  return render(createElement(CmsPasswordChange, { callsign }));
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe('<CmsPasswordChange>', () => {
  it('renders nothing when the feature is unavailable (no access code configured)', async () => {
    route({ available: false });
    renderControl();
    // Give the availability effect a tick to resolve.
    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([c]) => c === 'cms_password_change_available')).toBe(true),
    );
    expect(screen.queryByTestId('cms-password-change')).toBeNull();
  });

  it('shows the new + confirm fields when available', async () => {
    route({ available: true });
    renderControl();
    expect(await screen.findByTestId('cms-password-change')).toBeInTheDocument();
    expect(screen.getByTestId('cms-pw-new')).toBeInTheDocument();
    expect(screen.getByTestId('cms-pw-confirm')).toBeInTheDocument();
  });

  it('disables submit until the new password is valid and confirm matches', async () => {
    route({ available: true });
    renderControl();
    await screen.findByTestId('cms-password-change');
    const submit = screen.getByTestId('cms-pw-submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);

    fireEvent.change(screen.getByTestId('cms-pw-current'), { target: { value: 'oldpw' } });
    fireEvent.change(screen.getByTestId('cms-pw-new'), { target: { value: 'short' } }); // <6
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('cms-pw-new'), { target: { value: 'longenough' } });
    fireEvent.change(screen.getByTestId('cms-pw-confirm'), { target: { value: 'mismatch' } });
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('cms-pw-confirm'), { target: { value: 'longenough' } });
    expect(submit.disabled).toBe(false);
    // Clearing the current password re-disables (current is required).
    fireEvent.change(screen.getByTestId('cms-pw-current'), { target: { value: '' } });
    expect(submit.disabled).toBe(true);
  });

  it('submits the new password and shows success', async () => {
    route({ available: true });
    renderControl('n7cpz-10');
    await screen.findByTestId('cms-password-change');
    fireEvent.change(screen.getByTestId('cms-pw-current'), { target: { value: 'oldpw' } });
    fireEvent.change(screen.getByTestId('cms-pw-new'), { target: { value: 'brandnewpw' } });
    fireEvent.change(screen.getByTestId('cms-pw-confirm'), { target: { value: 'brandnewpw' } });
    fireEvent.click(screen.getByTestId('cms-pw-submit'));

    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([c]) => c === 'cms_password_change');
      expect(call).toBeTruthy();
      expect(call?.[1]).toEqual({ rawCallsign: 'n7cpz-10', oldPassword: 'oldpw', newPassword: 'brandnewpw' });
    });
    expect(await screen.findByTestId('cms-pw-success')).toBeInTheDocument();
    // Fields cleared after success (no lingering new secret in the DOM).
    expect((screen.getByTestId('cms-pw-new') as HTMLInputElement).value).toBe('');
  });

  it('surfaces the backend rejection message verbatim and shows no success', async () => {
    route({
      available: true,
      change: () => Promise.reject({ kind: 'Rejected', message: 'Old password is incorrect' }),
    });
    renderControl();
    await screen.findByTestId('cms-password-change');
    fireEvent.change(screen.getByTestId('cms-pw-current'), { target: { value: 'oldpw' } });
    fireEvent.change(screen.getByTestId('cms-pw-new'), { target: { value: 'brandnewpw' } });
    fireEvent.change(screen.getByTestId('cms-pw-confirm'), { target: { value: 'brandnewpw' } });
    fireEvent.click(screen.getByTestId('cms-pw-submit'));

    const err = await screen.findByTestId('cms-pw-error');
    expect(err).toHaveTextContent('Old password is incorrect');
    expect(screen.queryByTestId('cms-pw-success')).toBeNull();
  });
});
