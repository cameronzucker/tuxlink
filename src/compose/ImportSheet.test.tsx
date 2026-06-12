import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ImportSheet } from './ImportSheet';
import { importPreview, importCommit, importCancel } from './importApi';
import { open } from '@tauri-apps/plugin-dialog';
import type { ImportEntry } from './importApi';

vi.mock('./importApi', () => ({
  importPreview: vi.fn(),
  importCommit: vi.fn(),
  importCancel: vi.fn(),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

const fn = (m: unknown) => m as ReturnType<typeof vi.fn>;

const planOf = (entries: ImportEntry[]) => ({
  stagingToken: 'tok',
  entries,
  summary: {
    added: entries.filter((e) => e.kind === 'added').length,
    updated: entries.filter((e) => e.kind === 'update').length,
    overridesStandard: entries.filter((e) => e.kind === 'overridesStandard').length,
    skipped: 0,
    rejected: 0,
    companions: entries.filter((e) => e.kind === 'companion').length,
  },
});

const entry = (over: Partial<ImportEntry>): ImportEntry => ({
  relPath: 'A/X.html',
  id: 'X',
  folder: 'A',
  kind: 'added',
  reason: null,
  hasViewer: true,
  ...over,
});

describe('ImportSheet', () => {
  beforeEach(() => vi.clearAllMocks());

  it('leads with Choose ZIP and offers folder + single file', () => {
    render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByTestId('import-choose-zip')).toBeTruthy();
    expect(screen.getByTestId('import-choose-folder')).toBeTruthy();
    expect(screen.getByTestId('import-choose-file')).toBeTruthy();
  });

  it('renders a row per non-companion entry; companions are not listed', async () => {
    fn(open).mockResolvedValue('/x/org.zip');
    fn(importPreview).mockResolvedValue(
      planOf([
        entry({ id: 'New Initial', kind: 'added' }),
        entry({ id: 'comp', kind: 'companion' }),
      ]),
    );
    render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('import-choose-zip'));
    await screen.findByText('New Initial');
    expect(screen.queryByTestId('import-row-comp')).toBeNull();
  });

  it('commits with [] when the Update overwrite is left unchecked (default keep)', async () => {
    fn(open).mockResolvedValue('/x/org.zip');
    fn(importPreview).mockResolvedValue(
      planOf([
        entry({ id: 'New Initial', kind: 'added' }),
        entry({ id: 'Old Initial', kind: 'update' }),
      ]),
    );
    fn(importCommit).mockResolvedValue({
      installed: ['New Initial'],
      skippedUpdates: ['Old Initial'],
      entries: [],
    });
    const onDone = vi.fn();
    render(<ImportSheet onDone={onDone} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('import-choose-zip'));
    await screen.findByText('Old Initial');
    fireEvent.click(screen.getByTestId('import-commit'));
    await waitFor(() => expect(importCommit).toHaveBeenCalledWith('tok', []));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it('includes a checked overwrite id in the commit', async () => {
    fn(open).mockResolvedValue('/x/org.zip');
    fn(importPreview).mockResolvedValue(planOf([entry({ id: 'Old Initial', kind: 'update' })]));
    fn(importCommit).mockResolvedValue({
      installed: ['Old Initial'],
      skippedUpdates: [],
      entries: [],
    });
    render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('import-choose-zip'));
    const cb = await screen.findByTestId('import-approve-Old Initial');
    fireEvent.click(cb);
    fireEvent.click(screen.getByTestId('import-commit'));
    await waitFor(() => expect(importCommit).toHaveBeenCalledWith('tok', ['Old Initial']));
  });

  it('shows override + no-viewer warnings', async () => {
    fn(open).mockResolvedValue('/x/org.zip');
    fn(importPreview).mockResolvedValue(
      planOf([
        entry({
          id: 'ICS213_Initial',
          kind: 'overridesStandard',
          reason: 'Replaces the standard ICS213_Initial',
          hasViewer: false,
        }),
      ]),
    );
    render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('import-choose-zip'));
    await screen.findByTestId('import-warn-override-ICS213_Initial');
    expect(screen.getByTestId('import-warn-noviewer-ICS213_Initial')).toBeTruthy();
  });

  it('fires importCancel on unmount when a token is live and uncommitted', async () => {
    fn(open).mockResolvedValue('/x/org.zip');
    fn(importPreview).mockResolvedValue(planOf([entry({ id: 'New Initial', kind: 'added' })]));
    const { unmount } = render(<ImportSheet onDone={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('import-choose-zip'));
    await screen.findByText('New Initial');
    unmount();
    await waitFor(() => expect(importCancel).toHaveBeenCalledWith('tok'));
  });
});
