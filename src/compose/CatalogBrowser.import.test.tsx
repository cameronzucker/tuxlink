import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { CatalogBrowser, buildFolderTree, type Template } from './CatalogBrowser';
import { invoke } from '@tauri-apps/api/core';
import { openFormsFolder, formsCustomDelete } from './importApi';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('./importApi', () => ({
  openFormsFolder: vi.fn(),
  formsCustomDelete: vi.fn(),
}));
// Stub ImportSheet so we can drive onDone/onCancel without the dialog plumbing.
vi.mock('./ImportSheet', () => ({
  ImportSheet: ({
    onDone,
    onCancel,
  }: {
    onDone: (r: { installed: string[]; skippedUpdates: string[]; entries: [] }) => void;
    onCancel: () => void;
  }) => (
    <div data-testid="import-sheet-stub">
      <button
        data-testid="stub-import-done"
        onClick={() => onDone({ installed: ['MyForm Initial'], skippedUpdates: [], entries: [] })}
      >
        done
      </button>
      <button data-testid="stub-import-cancel" onClick={onCancel}>
        cancel
      </button>
    </div>
  ),
}));

const fn = (m: unknown) => m as ReturnType<typeof vi.fn>;

const CATALOG: Template[] = [
  { id: 'ICS213_Initial', label: 'ICS213_Initial', folder: 'ICS Forms', source: 'Bundled', path: '' },
  { id: 'MyForm Initial', label: 'MyForm Initial', folder: '', source: 'Custom', path: '' },
];

beforeEach(() => {
  vi.clearAllMocks();
  fn(invoke).mockImplementation((cmd: string) =>
    cmd === 'forms_list_catalog' ? Promise.resolve(CATALOG) : Promise.resolve(undefined),
  );
});

describe('buildFolderTree custom-first', () => {
  it('sorts custom categories before bundled', () => {
    const buckets = buildFolderTree([
      { id: 'Z', label: 'Z', folder: 'ICS Forms', source: 'Bundled', path: '' },
      { id: 'A', label: 'A', folder: '', source: 'Custom', path: '' },
    ]);
    expect(buckets[0].name).toBe('Custom');
  });
});

describe('CatalogBrowser import wiring', () => {
  it('footer offers Import + Update standard forms with distinct labels', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByTestId('catalog-browser-import');
    expect(screen.getByTestId('catalog-browser-import').textContent).toContain('Import group forms');
    expect(screen.getByTestId('catalog-browser-refresh').textContent).toContain(
      'Update standard forms',
    );
  });

  it('opens the import sheet from the footer button', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByTestId('catalog-browser-import'));
    expect(screen.getByTestId('import-sheet-stub')).toBeTruthy();
  });

  it('Open forms folder invokes openFormsFolder', async () => {
    fn(openFormsFolder).mockResolvedValue(undefined);
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByTestId('catalog-browser-open-folder'));
    await waitFor(() => expect(openFormsFolder).toHaveBeenCalled());
  });

  it('re-fetches the catalog after an import completes', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByTestId('catalog-browser-import'));
    const before = fn(invoke).mock.calls.filter((c) => c[0] === 'forms_list_catalog').length;
    fireEvent.click(screen.getByTestId('stub-import-done'));
    await waitFor(() =>
      expect(
        fn(invoke).mock.calls.filter((c) => c[0] === 'forms_list_catalog').length,
      ).toBeGreaterThan(before),
    );
  });

  it('removes a custom form through confirm → formsCustomDelete', async () => {
    fn(formsCustomDelete).mockResolvedValue(['MyForm Initial']);
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    // Expand the Custom folder (sorted first).
    fireEvent.click(await screen.findByText('Custom'));
    fireEvent.click(await screen.findByTestId('catalog-remove-MyForm Initial'));
    fireEvent.click(screen.getByTestId('catalog-remove-confirm-MyForm Initial'));
    await waitFor(() =>
      expect(formsCustomDelete).toHaveBeenCalledWith(['MyForm Initial']),
    );
  });

  it('Escape closes the import sheet rather than the picker', async () => {
    const onCancel = vi.fn();
    render(<CatalogBrowser onPick={vi.fn()} onCancel={onCancel} />);
    fireEvent.click(await screen.findByTestId('catalog-browser-import'));
    expect(screen.getByTestId('import-sheet-stub')).toBeTruthy();
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByTestId('import-sheet-stub')).toBeNull());
    expect(onCancel).not.toHaveBeenCalled();
  });
});
