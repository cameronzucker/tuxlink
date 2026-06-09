import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DeleteFolderDialog } from './DeleteFolderDialog';
import type { UserFolder } from './types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

function wrap(ui: React.ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const NETS: UserFolder = { slug: 'nets', displayName: 'Nets', createdAt: 'a' };

describe('DeleteFolderDialog — blast radius (tuxlink-ka3z A8)', () => {
  it('shows the blast-radius line with subfolder count + names when the folder has children', () => {
    wrap(
      <DeleteFolderDialog
        folder={NETS}
        childCount={2}
        childNames={['SATERN', 'ARES']}
        onClose={vi.fn()}
      />,
    );
    const note = screen.getByTestId('delete-folder-blast-radius');
    expect(note).toHaveTextContent('2 subfolders');
    expect(note).toHaveTextContent('SATERN');
    expect(note).toHaveTextContent('ARES');
  });

  it('omits the blast-radius line for a leaf folder', () => {
    wrap(<DeleteFolderDialog folder={NETS} childCount={0} onClose={vi.fn()} />);
    expect(screen.queryByTestId('delete-folder-blast-radius')).toBeNull();
  });
});
