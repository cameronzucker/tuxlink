/**
 * GridEdit — inline grid-edit + source chip tests (tuxlink-686, Task 8).
 *
 * Spec: docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §Task 8
 */

import { test, expect, vi } from 'vitest';
import { act } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { GridEdit } from './GridEdit';

test('clicking the grid value enters edit mode and commits a valid grid', async () => {
  const onCommit = vi.fn().mockResolvedValue(undefined);
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  const input = screen.getByTestId('grid-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: 'DM33ab' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  await act(async () => {});
  expect(onCommit).toHaveBeenCalledWith('DM33ab');
  expect(screen.getByTestId('grid-value-display')).toBeInTheDocument();
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('invalid grid shows a validation message and does not commit', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  const input = screen.getByTestId('grid-input');
  fireEvent.change(input, { target: { value: 'NOPE' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.getByTestId('grid-error')).toBeInTheDocument();
});

// tuxlink-pjih: the "GPS ready — tap to switch" affordance was removed.
// Under the new arbiter semantics, the displayed grid follows GPS-fresh-
// else-manual unconditionally, so the explicit "switch to GPS" step is
// structurally unreachable: a `source === 'Manual'` chip means there's no
// fresh fix, and gpsReady would be false. The test below codifies that
// the affordance is gone — its absence is now part of the contract.
test('no Use-GPS affordance is rendered (tuxlink-pjih)', () => {
  render(<GridEdit grid="CN87" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.queryByTestId('use-gps')).not.toBeInTheDocument();
});

test('Escape cancels edit without committing', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Escape' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('backend rejection shows the error detail and stays in edit mode', async () => {
  const onCommit = vi.fn().mockRejectedValue({ kind: 'Rejected', detail: 'Grid must be a 4- or 6-char Maidenhead locator.' });
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  fireEvent.change(screen.getByTestId('grid-input'), { target: { value: 'DM33' } });
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Enter' });
  await act(async () => {});
  expect(screen.getByTestId('grid-error')).toHaveTextContent('Grid must be a 4- or 6-char Maidenhead locator.');
  expect(screen.getByTestId('grid-input')).toBeInTheDocument();
});

// tuxlink-39b round 2: the source chip must read as ACTIVE (green) when GPS is
// the source AND a fix is locked — not greyed-out-as-if-disabled.
test('GPS chip is marked locked when GPS is the source and a fix is locked', () => {
  render(<GridEdit grid="DM33xx" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  const chip = screen.getByTestId('source-chip');
  expect(chip).toHaveClass('gps');
  expect(chip).toHaveClass('locked');
});

test('GPS chip is NOT locked when GPS source has no fix', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  const chip = screen.getByTestId('source-chip');
  expect(chip).toHaveClass('gps');
  expect(chip).not.toHaveClass('locked');
});

// tuxlink-39b round 2: the edit input had no prompt, so it "demanded input with
// no instructions". A format placeholder makes it self-explanatory.
test('grid input shows a format placeholder when editing', () => {
  render(<GridEdit grid="DM33" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  const input = screen.getByTestId('grid-input') as HTMLInputElement;
  expect(input.placeholder).toMatch(/DM33xx|6-char|Maidenhead/i);
});

// tuxlink-c79g Task 10: restore `onUseGps` per spec §4.1 — clicking the source
// chip while source = Manual fires onUseGps so the operator can switch to GPS.
test('calls onUseGps when source chip is clicked while source = Manual', () => {
  const onUseGps = vi.fn();
  render(
    <GridEdit
      grid="EM75"
      source="Manual"
      gpsReady={false}
      onCommit={vi.fn()}
      onUseGps={onUseGps}
    />,
  );
  fireEvent.click(screen.getByTestId('source-chip'));
  expect(onUseGps).toHaveBeenCalledTimes(1);
});

// tuxlink-c79g Task 11: per spec §2.2 + §4.2, the State 2 "GPS ready" hint is
// passive text — NOT a <button>. Pre-pjih had it as <button data-testid="use-gps">;
// the restoration renders a passive <span className="dash-gps-ready-status">.
test('GPS-ready hint in State 2 is a <span> (passive), not a <button>', () => {
  render(
    <GridEdit
      grid="EM75"
      source="Manual"
      gpsReady={true}
      onCommit={vi.fn()}
      onUseGps={vi.fn()}
    />,
  );
  const hint = screen.getByText(/GPS ready/i);
  expect(hint.tagName).toBe('SPAN');
});

// tuxlink-c79g Task 12: per spec §2.1 + §4.2, the source chip renders as a
// real <button> (keyboard-accessible, screen-reader-actionable) when source =
// Manual, and as a passive <span role="status"> when source = Gps.
test('source chip is a <button> when source = Manual', () => {
  render(
    <GridEdit
      grid="EM75"
      source="Manual"
      gpsReady={false}
      onCommit={vi.fn()}
      onUseGps={vi.fn()}
    />,
  );
  expect(screen.getByTestId('source-chip').tagName).toBe('BUTTON');
});

// tuxlink-c79g Task 12 follow-up: per spec §4.4 line 371 + plan body line 1481
// (grounded in adrev R2 #3 + R2 #6), the Manual source-chip <button> carries
// aria-pressed={false}. The initial T12 implementation deliberately omitted
// this citing WAI-ARIA's framing of aria-pressed as a toggle attribute; the
// spec is canonical and the attribute is restored. If there's appetite to
// amend the spec, that's a separate adrev-round decision — this test
// prevents future sessions from silently dropping the attribute again.
test('Manual source-chip <button> carries aria-pressed="false" (spec §4.4)', () => {
  render(
    <GridEdit
      grid="EM75"
      source="Manual"
      gpsReady={false}
      onCommit={vi.fn()}
      onUseGps={vi.fn()}
    />,
  );
  const chip = screen.getByTestId('source-chip');
  expect(chip).toHaveAttribute('aria-pressed', 'false');
});

test('source chip is a <span> with role=status when source = Gps', () => {
  render(
    <GridEdit
      grid="DM33"
      source="Gps"
      gpsReady={true}
      onCommit={vi.fn()}
      onUseGps={vi.fn()}
    />,
  );
  const chip = screen.getByTestId('source-chip');
  expect(chip.tagName).toBe('SPAN');
  expect(chip.getAttribute('role')).toBe('status');
});

test('source chip <span> does not call onUseGps on click when source = Gps', () => {
  const onUseGps = vi.fn();
  render(
    <GridEdit
      grid="DM33"
      source="Gps"
      gpsReady={true}
      onCommit={vi.fn()}
      onUseGps={onUseGps}
    />,
  );
  fireEvent.click(screen.getByTestId('source-chip'));
  expect(onUseGps).not.toHaveBeenCalled();
});

// tuxlink-c79g Task 13: spec §2.3 + §2.4 — `Set manually` button rendered in
// State 4 + State 5 (source = Gps && !gpsReady) so the operator can escape to
// inline-edit. The 4-quadrant matrix tests the present/absent contract; the
// focus test pins the Codex P2 #6 a11y promise; the interpunct + dimmed-chip
// test pins the State 1 vs State 4 visual differentiation from R2 #4.
test('Set manually button is present in State 4 (source = Gps && !gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.getByTestId('set-manually-button')).toBeInTheDocument();
});

test('Set manually button is absent in State 1 (source = Manual && !gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button is absent in State 3 (source = Gps && gpsReady)', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button is absent in State 2 (source = Manual && gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button focuses the grid input on click (Codex P2 #6)', async () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('set-manually-button'));
  // Wait for the inline-edit transition.
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  expect(document.activeElement).toBe(screen.getByTestId('grid-input'));
});

test('State 4 grid value has interpunct prefix + chip dimmed', () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  // The grid value should contain "· EM75" or render the interpunct as a separate element.
  expect(screen.getByTestId('grid-value-display').textContent).toMatch(/·\s+EM75/);
  expect(screen.getByTestId('source-chip').className).toContain('dimmed');
});
