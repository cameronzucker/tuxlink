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
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  const input = screen.getByTestId('grid-input') as HTMLInputElement;
  fireEvent.change(input, { target: { value: 'DM33ab' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  await act(async () => {});
  expect(onCommit).toHaveBeenCalledWith('DM33ab');
  expect(screen.getByTestId('ribbon-grid')).toBeInTheDocument();
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('invalid grid shows a validation message and does not commit', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  const input = screen.getByTestId('grid-input');
  fireEvent.change(input, { target: { value: 'NOPE' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.getByTestId('grid-error')).toBeInTheDocument();
});

test('shows GPS-ready affordance when a fix is available while Manual', () => {
  render(<GridEdit grid="CN87" source="Manual" gpsReady={true} onCommit={vi.fn()} onUseGps={vi.fn()} />);
  expect(screen.getByTestId('use-gps')).toBeInTheDocument();
});

test('Escape cancels edit without committing', () => {
  const onCommit = vi.fn();
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Escape' });
  expect(onCommit).not.toHaveBeenCalled();
  expect(screen.queryByTestId('grid-input')).not.toBeInTheDocument();
});

test('backend rejection shows the error detail and stays in edit mode', async () => {
  const onCommit = vi.fn().mockRejectedValue({ kind: 'Rejected', detail: 'Grid must be a 4- or 6-char Maidenhead locator.' });
  render(<GridEdit grid="CN87" source="Manual" gpsReady={false} onCommit={onCommit} onUseGps={vi.fn()} />);
  fireEvent.click(screen.getByTestId('ribbon-grid'));
  fireEvent.change(screen.getByTestId('grid-input'), { target: { value: 'DM33' } });
  fireEvent.keyDown(screen.getByTestId('grid-input'), { key: 'Enter' });
  await act(async () => {});
  expect(screen.getByTestId('grid-error')).toHaveTextContent('Grid must be a 4- or 6-char Maidenhead locator.');
  expect(screen.getByTestId('grid-input')).toBeInTheDocument();
});
