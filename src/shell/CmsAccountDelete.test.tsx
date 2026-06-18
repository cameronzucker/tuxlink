// CmsAccountDelete tests (tuxlink-vfb3 sub-project 3) — wired delete behind a
// typed-confirmation gate.
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CmsAccountDelete } from './CmsAccountDelete';

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** Route cms_password_change_available + cms_account_remove. */
function route(opts: { available?: boolean; remove?: () => Promise<unknown> } = {}) {
  const available = opts.available ?? true;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === 'cms_password_change_available') return Promise.resolve(available);
    if (cmd === 'cms_account_remove') return opts.remove ? opts.remove() : Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => invokeMock.mockReset());

describe('<CmsAccountDelete>', () => {
  it('renders nothing when the account API is unavailable (no access key)', async () => {
    route({ available: false });
    const { container } = render(<CmsAccountDelete callsign="KK7ABC" />);
    // availability resolves async; give the effect a tick, then assert empty.
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('cms_password_change_available'));
    expect(container.querySelector('[data-testid="account-delete"]')).toBeNull();
  });

  it('keeps Delete disabled until the exact callsign is typed (case-insensitive)', async () => {
    route();
    render(<CmsAccountDelete callsign="KK7ABC" />);
    const submit = (await screen.findByTestId('account-delete-submit')) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-delete-confirm'), { target: { value: 'KK7AB' } });
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId('account-delete-confirm'), { target: { value: 'kk7abc' } });
    expect(submit.disabled).toBe(false);
  });

  it('on confirm invokes cms_account_remove, fires onDeleted, and shows success', async () => {
    route();
    const onDeleted = vi.fn();
    render(<CmsAccountDelete callsign="KK7ABC" onDeleted={onDeleted} />);
    fireEvent.change(await screen.findByTestId('account-delete-confirm'), { target: { value: 'KK7ABC' } });
    fireEvent.click(screen.getByTestId('account-delete-submit'));
    await waitFor(() => expect(screen.getByTestId('account-delete-success')).toBeInTheDocument());
    expect(invokeMock).toHaveBeenCalledWith('cms_account_remove', { rawCallsign: 'KK7ABC' });
    expect(onDeleted).toHaveBeenCalledTimes(1);
  });

  it('surfaces a privileged/refused rejection plainly without claiming success', async () => {
    route({ remove: () => Promise.reject({ kind: 'Rejected', code: 'Forbidden', message: 'not permitted' }) });
    const onDeleted = vi.fn();
    render(<CmsAccountDelete callsign="KK7ABC" onDeleted={onDeleted} />);
    fireEvent.change(await screen.findByTestId('account-delete-confirm'), { target: { value: 'KK7ABC' } });
    fireEvent.click(screen.getByTestId('account-delete-submit'));
    await waitFor(() => expect(screen.getByTestId('account-delete-error').textContent).toMatch(/not permitted/i));
    expect(screen.queryByTestId('account-delete-success')).not.toBeInTheDocument();
    expect(onDeleted).not.toHaveBeenCalled();
  });

  it('maps a post-send timeout to an indeterminate-outcome warning', async () => {
    route({ remove: () => Promise.reject({ kind: 'UnknownOutcome' }) });
    render(<CmsAccountDelete callsign="KK7ABC" />);
    fireEvent.change(await screen.findByTestId('account-delete-confirm'), { target: { value: 'KK7ABC' } });
    fireEvent.click(screen.getByTestId('account-delete-submit'));
    await waitFor(() =>
      expect(screen.getByTestId('account-delete-error').textContent).toMatch(/may or may not/i)
    );
  });
});
