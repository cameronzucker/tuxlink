// CatalogBrowser — hierarchical picker tests. The picker fetches the
// flat catalog via `forms_list_catalog` and tree-builds folders client-
// side; this suite verifies tree rendering, expand/collapse, search,
// pick + cancel callbacks, the "Custom last" sort invariant, and the
// error surface.
//
// Tauri's `invoke` is mocked so the component can call
// `forms_list_catalog` without booting the runtime.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 10.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §7.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// Hoist the invoke spy so the vi.mock factory can wire it; the
// individual tests reach in via `mocks.invoke` to swap return values.
const mocks = vi.hoisted(() => {
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === 'forms_list_catalog') {
      return [
        { id: 'ICS213_Initial', label: 'ICS213_Initial', folder: 'ICS Forms', source: 'Bundled', path: '' },
        { id: 'Bulletin_Initial', label: 'Bulletin_Initial', folder: 'General', source: 'Bundled', path: '' },
        { id: 'ARC213', label: 'ARC213', folder: 'ARC Forms', source: 'Bundled', path: '' },
        { id: 'MyCustom', label: 'MyCustom', folder: '', source: 'Custom', path: '' },
      ];
    }
    return null;
  });
  return { invoke };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

// Component import MUST come after the vi.mock call so its module-
// level `import { invoke } from '@tauri-apps/api/core'` resolves to
// the mock.
// eslint-disable-next-line import/first
import { CatalogBrowser } from './CatalogBrowser';

describe('<CatalogBrowser>', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    // Reset default impl between tests so a `mockImplementationOnce`
    // (e.g. the error path) doesn't leak into a later success-path test.
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'forms_list_catalog') {
        return [
          { id: 'ICS213_Initial', label: 'ICS213_Initial', folder: 'ICS Forms', source: 'Bundled', path: '' },
          { id: 'Bulletin_Initial', label: 'Bulletin_Initial', folder: 'General', source: 'Bundled', path: '' },
          { id: 'ARC213', label: 'ARC213', folder: 'ARC Forms', source: 'Bundled', path: '' },
          { id: 'MyCustom', label: 'MyCustom', folder: '', source: 'Custom', path: '' },
        ];
      }
      return null;
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders all top-level folders', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText('ICS Forms')).toBeInTheDocument();
    expect(screen.getByText('General')).toBeInTheDocument();
    expect(screen.getByText('ARC Forms')).toBeInTheDocument();
    expect(screen.getByText('Custom')).toBeInTheDocument();
  });

  it('expanding a folder reveals its templates', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByText('ICS Forms'));
    expect(screen.getByText('ICS213_Initial')).toBeInTheDocument();
  });

  it('search filters across folders', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    const input = await screen.findByPlaceholderText(/search forms/i);
    fireEvent.change(input, { target: { value: 'arc' } });
    expect(screen.getByText('ARC213')).toBeInTheDocument();
    expect(screen.queryByText('Bulletin_Initial')).toBeNull();
  });

  it('picking a form fires onPick with the id', async () => {
    const onPick = vi.fn();
    render(<CatalogBrowser onPick={onPick} onCancel={vi.fn()} />);
    fireEvent.click(await screen.findByText('ICS Forms'));
    fireEvent.click(screen.getByText('ICS213_Initial'));
    expect(onPick).toHaveBeenCalledWith('ICS213_Initial');
  });

  it('places the Custom folder last regardless of alphabetical order', async () => {
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    // Wait for the catalog to load.
    await screen.findByText('Custom');
    const folderHeadings = screen.getAllByTestId('catalog-folder-name');
    const names = folderHeadings.map((el) => el.textContent);
    // ARC Forms, General, ICS Forms sort alphabetically; Custom is last.
    expect(names).toEqual(['ARC Forms', 'General', 'ICS Forms', 'Custom']);
  });

  it('renders an error banner when forms_list_catalog rejects', async () => {
    mocks.invoke.mockImplementationOnce(async () => {
      throw new Error('catalog load failed');
    });
    render(<CatalogBrowser onPick={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(/catalog load failed/i);
    });
  });

  it('calls onCancel when the Cancel button is clicked', async () => {
    const onCancel = vi.fn();
    render(<CatalogBrowser onPick={vi.fn()} onCancel={onCancel} />);
    // Wait for initial load so the Cancel button is in the steady-state
    // chrome (the picker always renders a cancel control even during
    // loading, but waiting for `findByText` also guards against the
    // useEffect not having fired yet).
    await screen.findByText('Custom');
    fireEvent.click(screen.getByTestId('catalog-browser-cancel'));
    expect(onCancel).toHaveBeenCalled();
  });
});
