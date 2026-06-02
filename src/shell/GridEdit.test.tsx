/**
 * GridEdit — inline grid-edit + source segmented control tests.
 *
 * Originally tuxlink-686 Task 8 (single chip). Restructured for tuxlink-z5pz
 * (2026-06-02): the source surface is now a radio-group segmented control with
 * two `<button role="radio">` segments (GPS + MANUAL) rendered side-by-side,
 * replacing the T12 conditional `<button>`-or-`<span>` chip pattern. The T11
 * passive `<span data-testid="gps-ready-status">` sibling-hint span folds INTO
 * the GPS segment as a `' ●'` text suffix.
 *
 * Spec: docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md §2.1, §2.2, §6.2
 */

import { test, expect, vi } from 'vitest';
import { act } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { GridEdit } from './GridEdit';

test('clicking the grid value enters edit mode and commits a valid grid', async () => {
  const onCommit = vi.fn().mockResolvedValue(undefined);
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
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
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  const input = screen.getByTestId('grid-input');
  fireEvent.change(input, { target: { value: 'NOPE' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.getByTestId('grid-error')).toBeInTheDocument();
});

// tuxlink-pjih: the pre-pjih "GPS ready — tap to switch" affordance was a
// <button data-testid="use-gps">. T11 replaced it with a passive
// <span data-testid="gps-ready-status">. tuxlink-z5pz folds the ready hint
// INTO the GPS segment as a ' ●' text suffix; the standalone span is removed.
// This test guards against any future regression that reintroduces the
// standalone use-gps button by testid.
test('no separate <button data-testid="use-gps"> affordance is rendered (tuxlink-pjih)', () => {
  render(<GridEdit grid="CN87" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.queryByTestId('use-gps')).not.toBeInTheDocument();
});

// tuxlink-z5pz: the standalone gps-ready-status sibling span is REMOVED; the
// in-segment ' ●' suffix on the GPS segment carries the same semantic load.
test('standalone gps-ready-status sibling span is removed (tuxlink-z5pz)', () => {
  render(<GridEdit grid="CN87" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.queryByTestId('gps-ready-status')).not.toBeInTheDocument();
});

test('Escape cancels edit without committing', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Escape' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('backend rejection shows the error detail and stays in edit mode', async () => {
  const onCommit = vi.fn().mockRejectedValue({ kind: 'Rejected', detail: 'Grid must be a 4- or 6-char Maidenhead locator.' });
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  fireEvent.change(screen.getByTestId('grid-input'), { target: { value: 'DM33' } });
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Enter' });
  await act(async () => {});
  expect(screen.getByTestId('grid-error')).toHaveTextContent('Grid must be a 4- or 6-char Maidenhead locator.');
  expect(screen.getByTestId('grid-input')).toBeInTheDocument();
});

// tuxlink-39b round 2: the GPS segment must read as ACTIVE (green) when GPS is
// the source AND a fix is locked — not greyed-out-as-if-disabled.
test('GPS segment carries gps-ready class when GPS is the source and a fix is locked', () => {
  render(<GridEdit grid="DM33xx" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  const gps = screen.getByTestId('source-segment-gps');
  expect(gps).toHaveClass('gps');
  expect(gps).toHaveClass('selected');
  expect(gps).toHaveClass('gps-ready');
});

test('GPS segment is selected-but-dimmed when GPS source has no fix (State 4/5)', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  const gps = screen.getByTestId('source-segment-gps');
  expect(gps).toHaveClass('gps');
  expect(gps).toHaveClass('selected');
  expect(gps).not.toHaveClass('gps-ready');
  // Per §2.4 amendment: the visual cue for State 4/5 dimming is encoded as the
  // .dash-source-segment.gps.selected:not(.gps-ready) CSS selector chain.
  // Implementation also tags the segment with a `dimmed` className for class-list
  // discoverability + symmetry with the prior chip.dimmed assertion below.
  expect(gps).toHaveClass('dimmed');
});

test('grid input shows a format placeholder when editing', () => {
  render(<GridEdit grid="DM33" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('grid-value-display'));
  const input = screen.getByTestId('grid-input') as HTMLInputElement;
  expect(input.placeholder).toMatch(/DM33xx|6-char|Maidenhead/i);
});

// =========================================================================
// tuxlink-z5pz segmented-control tests (spec §6.2 — 2026-06-02 amendment)
// =========================================================================

test('radiogroup_has_role_and_aria_label', () => {
  const { container } = render(
    <GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />,
  );
  const group = container.querySelector('[role="radiogroup"]') as HTMLElement;
  expect(group).not.toBeNull();
  expect(group.getAttribute('aria-label')).toBe('Position source');
});

test('gps_segment_is_selected_when_source_is_Gps', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-checked', 'true');
  expect(screen.getByTestId('source-segment-manual')).toHaveAttribute('aria-checked', 'false');
});

test('manual_segment_is_selected_when_source_is_Manual', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('source-segment-manual')).toHaveAttribute('aria-checked', 'true');
  expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-checked', 'false');
});

test('clicking_GPS_segment_when_source_is_Manual_fires_onUseGps', () => {
  const onUseGps = vi.fn();
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={onUseGps} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('source-segment-gps'));
  expect(onUseGps).toHaveBeenCalledTimes(1);
});

test('clicking_MANUAL_segment_when_source_is_Gps_fires_onUseManual_and_enters_edit_mode', async () => {
  const onUseManual = vi.fn();
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={onUseManual} />);
  fireEvent.click(screen.getByTestId('source-segment-manual'));
  // The MANUAL-segment click triggers BOTH the onUseManual prop AND the
  // internal enterEdit() flow (per Choice B in the implementation note —
  // explicit symmetry with onUseGps preserves DashboardRibbon-side test-spy
  // hooks for any optimistic invalidate behavior added later).
  expect(onUseManual).toHaveBeenCalledTimes(1);
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  const input = screen.getByTestId('grid-input');
  expect(input).toBeInTheDocument();
  expect(document.activeElement).toBe(input);
});

test('manual_segment_click_then_escape_keeps_source_gps', async () => {
  // Spec §2.1 amended: clicking the MANUAL segment from State 3 enters edit
  // mode, but if the operator cancels (Escape), source remains Gps — no
  // config_set_grid runs, so the T4-sticky source-flip side-effect never
  // triggers. This test pins the cancel contract: onCommit is NEVER called,
  // and the source value handed to GridEdit (still 'Gps' from the parent) is
  // not mutated by GridEdit itself (GridEdit derives nothing local).
  const onCommit = vi.fn();
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={onCommit} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('source-segment-manual'));
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Escape' });
  expect(onCommit).not.toHaveBeenCalled();
  // After cancel, edit mode exits and the GPS segment remains selected (the
  // parent-supplied `source` prop never changed).
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
  expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-checked', 'true');
});

test('clicking_already_selected_GPS_segment_is_a_noop', () => {
  const onUseGps = vi.fn();
  const onUseManual = vi.fn();
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={onUseGps} onUseManual={onUseManual} />);
  fireEvent.click(screen.getByTestId('source-segment-gps'));
  expect(onUseGps).not.toHaveBeenCalled();
  expect(onUseManual).not.toHaveBeenCalled();
});

test('clicking_already_selected_MANUAL_segment_is_a_noop', () => {
  const onUseGps = vi.fn();
  const onUseManual = vi.fn();
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={onUseGps} onUseManual={onUseManual} />);
  fireEvent.click(screen.getByTestId('source-segment-manual'));
  expect(onUseGps).not.toHaveBeenCalled();
  expect(onUseManual).not.toHaveBeenCalled();
  // Clicking the already-selected MANUAL segment does NOT enter edit mode.
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('in_segment_gps_ready_indicator_renders_dot_on_GPS_segment_when_source_is_Manual_and_gpsReady', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('source-segment-gps').textContent).toBe('GPS ●');
});

test('in_segment_gps_ready_indicator_is_absent_when_gpsReady_is_false', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('source-segment-gps').textContent).toBe('GPS');
});

test('in_segment_gps_ready_indicator_is_absent_when_source_is_Gps_even_if_gpsReady', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  // When the GPS segment is selected, the dot is redundant — the segment IS
  // the live-fix indicator.
  expect(screen.getByTestId('source-segment-gps').textContent).toBe('GPS');
});

test('GPS_segment_indicator_dot_is_aria_hidden_and_aria_label_conveys_fresh_fix', () => {
  // The '●' glyph would announce as "black circle" without aria-hidden.
  // The semantic "fresh fix available" cue rides through aria-label instead.
  render(<GridEdit grid="EM75" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  const gps = screen.getByTestId('source-segment-gps');
  expect(gps).toHaveAttribute('aria-label', 'GPS — fresh fix available');
  // The dot lives inside an aria-hidden span — assert the SR-relevant accessible
  // name is the aria-label, not the textContent.
  const dot = gps.querySelector('span[aria-hidden="true"]');
  expect(dot).not.toBeNull();
  expect(dot?.textContent).toBe(' ●');
});

test('GPS_segment_aria_label_is_plain_GPS_when_not_advertising_fresh_fix', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('source-segment-gps')).toHaveAttribute('aria-label', 'GPS');
});

// =========================================================================
// `Set manually` button tests (preserved from v3 — no segmented-control impact)
// =========================================================================

test('Set manually button is present in State 4 (source = Gps && !gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.getByTestId('set-manually-button')).toBeInTheDocument();
});

test('Set manually button is absent in State 1 (source = Manual && !gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button is absent in State 3 (source = Gps && gpsReady)', () => {
  render(<GridEdit grid="DM33" source="Gps" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button is absent in State 2 (source = Manual && gpsReady)', () => {
  render(<GridEdit grid="EM75" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  expect(screen.queryByTestId('set-manually-button')).not.toBeInTheDocument();
});

test('Set manually button focuses the grid input on click (Codex P2 #6)', async () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  fireEvent.click(screen.getByTestId('set-manually-button'));
  // Wait for the inline-edit transition.
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  expect(document.activeElement).toBe(screen.getByTestId('grid-input'));
});

test('State_4_grid_value_has_interpunct_prefix_and_GPS_segment_is_dimmed', () => {
  render(<GridEdit grid="EM75" source="Gps" gpsReady={false} onCommit={vi.fn()} onUseGps={vi.fn()} onUseManual={vi.fn()} />);
  // The grid value should contain "· EM75".
  expect(screen.getByTestId('grid-value-display').textContent).toMatch(/·\s+EM75/);
  // Per spec §2.4: the dimmed-selected GPS segment is matched by the CSS
  // selector .dash-source-segment.gps.selected:not(.gps-ready). The
  // implementation also adds an explicit `dimmed` className for class-list
  // discoverability (mirrors the prior chip.dimmed test assertion).
  const gps = screen.getByTestId('source-segment-gps');
  expect(gps.classList.contains('selected')).toBe(true);
  expect(gps.classList.contains('gps-ready')).toBe(false);
  expect(gps.classList.contains('dimmed')).toBe(true);
});
