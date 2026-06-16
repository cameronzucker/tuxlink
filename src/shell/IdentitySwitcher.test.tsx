/**
 * IdentitySwitcher — closed-chip + dropdown + inline-unlock tests.
 *
 * Phase 7 (tuxlink-noa0). Built via strict TDD across Tasks 7 (closed chip),
 * 8 (open dropdown), 9 (inline unlock). The identity list is FLAT — tacticals
 * are derived by `list.tactical.filter(t => t.parent === full.callsign)`.
 *
 * Reconciliation (the plan doc is stale on this): switching identity ALWAYS
 * authenticates the parent FULL via a credential — there is no switch-without-
 * auth path. So every non-active row click reveals the inline unlock field;
 * clicking the already-active row just closes the dropdown.
 *
 * Closed-chip footprint reproduces the DashboardRibbon `.dash-callsign-row`.
 */

import { test, expect, vi } from 'vitest';
import { render, screen, fireEvent, within, waitFor } from '@testing-library/react';
import { IdentitySwitcher } from './IdentitySwitcher';
import type {
  ActiveIdentityDto,
  IdentityListDto,
} from './identityTypes';

const FULL_ACTIVE: ActiveIdentityDto = {
  mycall: 'W7XYZ',
  address_as: 'W7XYZ',
  is_tactical: false,
};

const TACTICAL_ACTIVE: ActiveIdentityDto = {
  mycall: 'W1ABC',
  address_as: 'EOC-3',
  is_tactical: true,
};

const LIST: IdentityListDto = {
  full: [
    { callsign: 'W7XYZ', label: null, has_cms_account: true, cms_registered: true, needs_auth: false },
    { callsign: 'W1ABC', label: 'Club', has_cms_account: true, cms_registered: true, needs_auth: true },
  ],
  tactical: [
    { label: 'EOC-3', parent: 'W1ABC', cms_badge: 'registered' },
    { label: 'SHELTER-1', parent: 'W1ABC', cms_badge: 'not_registered' },
    { label: 'NET-CTRL', parent: 'W1ABC', cms_badge: 'unknown' },
  ],
  last_selected: 'W7XYZ',
};

// ---------------------------------------------------------------------------
// TASK 7 — closed chip (footprint unchanged)
// ---------------------------------------------------------------------------

test('closed_chip shows ribbon-callsign + text; dropdown absent', () => {
  render(
    <IdentitySwitcher
      active={FULL_ACTIVE}
      list={LIST}
      onSwitch={vi.fn()}
    />,
  );
  const row = screen.getByTestId('ribbon-callsign');
  expect(row).toBeInTheDocument();
  expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent('W7XYZ');
  expect(screen.getByTestId('identity-switcher-trigger')).toBeInTheDocument();
  // bd-tuxlink-y8tf: the SSID select was removed from the ribbon chip.
  expect(screen.queryByTestId('ribbon-ssid-select')).not.toBeInTheDocument();
  // Dropdown not in DOM when closed.
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
});

test('tactical_active shows address_as as primary label + parent indicator', () => {
  render(
    <IdentitySwitcher
      active={TACTICAL_ACTIVE}
      list={LIST}
      onSwitch={vi.fn()}
    />,
  );
  expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent('EOC-3');
  const parent = screen.getByTestId('identity-active-parent');
  expect(parent).toBeInTheDocument();
  expect(parent).toHaveTextContent('W1ABC');
});

test('null_active renders fallback (list.last_selected)', () => {
  render(
    <IdentitySwitcher
      active={null}
      list={LIST}
      onSwitch={vi.fn()}
    />,
  );
  // Falls back to list.last_selected ('W7XYZ'); never an em-dash here since
  // last_selected is present.
  expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent('W7XYZ');
  expect(screen.queryByTestId('identity-active-parent')).not.toBeInTheDocument();
});

test('null_active with no last_selected renders an em-dash', () => {
  const emptyList: IdentityListDto = { full: [], tactical: [], last_selected: null };
  render(
    <IdentitySwitcher active={null} list={emptyList} onSwitch={vi.fn()} />,
  );
  expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent('—');
});

test('never renders an SSID select (bd-tuxlink-y8tf — SSID is per-transport)', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  expect(screen.queryByTestId('ribbon-ssid-select')).not.toBeInTheDocument();
  expect(screen.getByTestId('ribbon-callsign-text')).toHaveTextContent('W7XYZ');
});

// ---------------------------------------------------------------------------
// TASK 8 — open dropdown
// ---------------------------------------------------------------------------

function openDropdown() {
  fireEvent.click(screen.getByTestId('identity-switcher-trigger'));
}

test('opening_lists_full_with_nested_tacticals (DOM order)', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  const listbox = screen.getByTestId('identity-switcher-list');
  expect(listbox).toHaveAttribute('role', 'listbox');
  // Collect identity rows in DOM order.
  const rows = within(listbox).getAllByTestId(/^identity-row-/);
  const order = rows.map((r) => r.getAttribute('data-testid'));
  // W7XYZ (FULL, no tacticals) then W1ABC (FULL) then its three tacticals.
  expect(order).toEqual([
    'identity-row-full-W7XYZ',
    'identity-row-full-W1ABC',
    'identity-row-tactical-EOC-3',
    'identity-row-tactical-SHELTER-1',
    'identity-row-tactical-NET-CTRL',
  ]);
});

test('locked_full_shows_lock_glyph; active full does not', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  const lockedRow = screen.getByTestId('identity-row-full-W1ABC');
  expect(lockedRow.getAttribute('aria-label')?.toLowerCase()).toContain('locked');
  expect(within(lockedRow).getByText('🔒')).toHaveAttribute('aria-hidden', 'true');

  const activeRow = screen.getByTestId('identity-row-full-W7XYZ');
  expect(activeRow.getAttribute('aria-label')?.toLowerCase() ?? '').not.toContain('locked');
  expect(within(activeRow).queryByText('🔒')).not.toBeInTheDocument();
});

test('tactical_cms_badges_render (all three states)', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  expect(screen.getByTestId('cms-badge-ok')).toBeInTheDocument();
  const blocked = screen.getByTestId('cms-badge-blocked');
  expect(blocked).toBeInTheDocument();
  expect(blocked).toHaveAttribute('title');
  const unknown = screen.getByTestId('cms-badge-unknown');
  expect(unknown).toBeInTheDocument();
  expect(unknown).toHaveAttribute('title');
});

test('last_selected_row_marked_current', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  expect(screen.getByTestId('identity-row-full-W7XYZ')).toHaveAttribute('aria-current', 'true');
  expect(screen.getByTestId('identity-row-full-W1ABC')).not.toHaveAttribute('aria-current', 'true');
});

test('clicking_active_row_closes_without_unlock', () => {
  const onSwitch = vi.fn();
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W7XYZ'));
  // Dropdown closes, no unlock revealed, no switch fired.
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
  expect(screen.queryByTestId('identity-unlock-input')).not.toBeInTheDocument();
  expect(onSwitch).not.toHaveBeenCalled();
});

test('clicking_active_tactical_row_closes_without_unlock', () => {
  const onSwitch = vi.fn();
  render(
    <IdentitySwitcher active={TACTICAL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  // EOC-3 is the active address_as → re-selecting it is a no-op.
  fireEvent.click(screen.getByTestId('identity-row-tactical-EOC-3'));
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
  expect(onSwitch).not.toHaveBeenCalled();
});

test('Esc closes the open dropdown', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  fireEvent.keyDown(screen.getByTestId('identity-switcher-list'), { key: 'Escape' });
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
});

test('click outside closes the open dropdown', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  expect(screen.getByTestId('identity-switcher-list')).toBeInTheDocument();
  fireEvent.mouseDown(document.body);
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
});

test('loading_list_null_shows_placeholder_row', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={null} onSwitch={vi.fn()} />,
  );
  openDropdown();
  expect(screen.getByTestId('identity-switcher-list')).toBeInTheDocument();
  expect(screen.getByTestId('identity-list-loading')).toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// TASK 9 — inline unlock within the open dropdown
// ---------------------------------------------------------------------------

test('selecting_locked_full_reveals_inline_unlock (input descendant of list)', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  const listbox = screen.getByTestId('identity-switcher-list');
  const input = screen.getByTestId('identity-unlock-input');
  expect(input).toBeInTheDocument();
  expect(input).toHaveAttribute('type', 'password');
  // The input MUST be a descendant of the list (no portal / position:fixed popup).
  expect(listbox.contains(input)).toBe(true);
  expect(screen.getByTestId('identity-unlock-submit')).toHaveTextContent('Unlock');
});

test('unlock_submit_calls_onSwitch_with_credential (FULL)', async () => {
  const onSwitch = vi.fn().mockResolvedValue(undefined);
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  fireEvent.change(screen.getByTestId('identity-unlock-input'), { target: { value: 'hunter2' } });
  fireEvent.click(screen.getByTestId('identity-unlock-submit'));
  await waitFor(() =>
    expect(onSwitch).toHaveBeenCalledWith({ callsign: 'W1ABC', credential: 'hunter2', tacticalLabel: null }),
  );
});

test('unlock_submit_calls_onSwitch_with_credential (tactical → parent + label)', async () => {
  const onSwitch = vi.fn().mockResolvedValue(undefined);
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-tactical-SHELTER-1'));
  fireEvent.change(screen.getByTestId('identity-unlock-input'), { target: { value: 'pw' } });
  fireEvent.click(screen.getByTestId('identity-unlock-submit'));
  await waitFor(() =>
    expect(onSwitch).toHaveBeenCalledWith({ callsign: 'W1ABC', credential: 'pw', tacticalLabel: 'SHELTER-1' }),
  );
});

test('Enter submits the unlock form', async () => {
  const onSwitch = vi.fn().mockResolvedValue(undefined);
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  const input = screen.getByTestId('identity-unlock-input');
  fireEvent.change(input, { target: { value: 'secret' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  await waitFor(() =>
    expect(onSwitch).toHaveBeenCalledWith({ callsign: 'W1ABC', credential: 'secret', tacticalLabel: null }),
  );
});

test('unlock_reject_shows_error_and_stays_open (value retained)', async () => {
  const onSwitch = vi.fn().mockRejectedValue({ kind: 'AuthFailed', detail: { reason: 'bad password' } });
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  const input = screen.getByTestId('identity-unlock-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: 'wrong' } });
  fireEvent.click(screen.getByTestId('identity-unlock-submit'));
  const err = await screen.findByTestId('identity-unlock-error');
  expect(err).toHaveAttribute('role', 'alert');
  expect(err).toHaveTextContent('bad password');
  // Field stays open + value retained.
  expect(screen.getByTestId('identity-unlock-input')).toHaveValue('wrong');
  expect(screen.getByTestId('identity-switcher-list')).toBeInTheDocument();
});

test('unlock_esc_cancels_back_to_list (dropdown stays open)', () => {
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={vi.fn()} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  const input = screen.getByTestId('identity-unlock-input');
  fireEvent.keyDown(input, { key: 'Escape' });
  // Unlock field gone, dropdown still open.
  expect(screen.queryByTestId('identity-unlock-input')).not.toBeInTheDocument();
  expect(screen.getByTestId('identity-switcher-list')).toBeInTheDocument();
});

test('unlock_success_closes the dropdown', async () => {
  const onSwitch = vi.fn().mockResolvedValue(undefined);
  render(
    <IdentitySwitcher active={FULL_ACTIVE} list={LIST} onSwitch={onSwitch} />,
  );
  openDropdown();
  fireEvent.click(screen.getByTestId('identity-row-full-W1ABC'));
  fireEvent.change(screen.getByTestId('identity-unlock-input'), { target: { value: 'ok' } });
  fireEvent.click(screen.getByTestId('identity-unlock-submit'));
  await waitFor(() => expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument());
  expect(screen.queryByTestId('identity-unlock-input')).not.toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Adversarial-review fixes (moraine-swallow-bayou): case-insensitive active
// match (MINOR 1) + duplicate-tactical-label disambiguation (IMPORTANT 3)
// ---------------------------------------------------------------------------

test('active match is case-insensitive: a lowercase active call unlocks its stored FULL row', () => {
  // Backend auth + needs_auth fold ASCII case, so an active "w7xyz" must be
  // recognized as the stored "W7XYZ" — clicking it is a no-op (closes), and it
  // is aria-selected, not treated as a different, lockable identity.
  render(
    <IdentitySwitcher
      active={{ mycall: 'w7xyz', address_as: 'w7xyz', is_tactical: false }}
      list={LIST}
      onSwitch={vi.fn()}
    />,
  );
  openDropdown();
  const row = screen.getByTestId('identity-row-full-W7XYZ');
  expect(row).toHaveAttribute('aria-selected', 'true');
  fireEvent.click(row);
  // No-op close (the already-active identity), NOT an unlock prompt.
  expect(screen.queryByTestId('identity-switcher-list')).not.toBeInTheDocument();
  expect(screen.queryByTestId('identity-unlock-input')).not.toBeInTheDocument();
});

test('a tactical label shared across two parents is disambiguated by parent', () => {
  // Tactical labels are NOT globally unique (spec): two FULLs may each register
  // "NET-CTRL". Selecting one must reveal exactly ONE inline unlock (its own),
  // and only the matching-parent row is aria-selected when active.
  const DUP: IdentityListDto = {
    full: [
      { callsign: 'W1ABC', label: null, has_cms_account: true, cms_registered: true, needs_auth: true },
      { callsign: 'K9DEF', label: null, has_cms_account: true, cms_registered: true, needs_auth: true },
    ],
    tactical: [
      { label: 'NET-CTRL', parent: 'W1ABC', cms_badge: 'registered' },
      { label: 'NET-CTRL', parent: 'K9DEF', cms_badge: 'registered' },
    ],
    last_selected: null,
  };
  const { rerender } = render(<IdentitySwitcher active={null} list={DUP} onSwitch={vi.fn()} />);
  openDropdown();
  // Two same-label tactical rows are rendered (one per parent).
  const rows = screen.getAllByTestId('identity-row-tactical-NET-CTRL');
  expect(rows).toHaveLength(2);
  // Clicking the first (W1ABC's) reveals exactly ONE unlock form, not both.
  fireEvent.click(rows[0]);
  expect(screen.getAllByTestId('identity-unlock-input')).toHaveLength(1);
  expect(screen.getByText('Unlock NET-CTRL')).toBeInTheDocument();

  // When K9DEF's NET-CTRL is the active identity, only its row is aria-selected.
  rerender(
    <IdentitySwitcher
      active={{ mycall: 'K9DEF', address_as: 'NET-CTRL', is_tactical: true }}
      list={DUP}
      onSwitch={vi.fn()}
    />,
  );
  const after = screen.getAllByTestId('identity-row-tactical-NET-CTRL');
  const selected = after.filter((r) => r.getAttribute('aria-selected') === 'true');
  expect(selected).toHaveLength(1);
});

test('empty store shows an actionable empty-state, not a blank dropdown (tuxlink-z6yi)', () => {
  const emptyList = { full: [], tactical: [], last_selected: null };
  render(<IdentitySwitcher active={null} list={emptyList} onSwitch={vi.fn()} />);
  fireEvent.click(screen.getByTestId('identity-switcher-trigger'));
  const empty = screen.getByTestId('identity-switcher-empty');
  expect(empty).toBeInTheDocument();
  expect(empty).toHaveTextContent(/Settings/i);
});
