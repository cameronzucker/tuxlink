import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogReplyView } from './CatalogReplyView';

beforeEach(() => vi.mocked(invoke).mockReset());

describe('CatalogReplyView', () => {
  it('renders a structured area-weather view + raw toggle', async () => {
    vi.mocked(invoke).mockResolvedValue({
      kind: 'area-weather',
      product: 'FPUS65 KPSR',
      office: 'National Weather Service Phoenix AZ',
      issued: '1138 PM MST Thu Jun 4 2026',
      raw: 'RAWBODY',
    });
    render(<CatalogReplyView subject="INQUIRY - https://tgftp.nws.noaa.gov/x" body="b" />);
    expect(await screen.findByText(/Phoenix AZ/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /show raw/i }));
    expect(screen.getByText('RAWBODY')).toBeTruthy();
  });

  it('renders raw when the parser returns raw (struct variant: { kind:"raw", text })', async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: 'raw', text: 'just text' });
    render(<CatalogReplyView subject="Service Advice Message" body="just text" />);
    await waitFor(() => expect(screen.getByText('just text')).toBeTruthy());
  });

  // Note: the component shows the raw body on first render (before parse resolves) and the
  // useEffect catch falls back to raw on invoke rejection — a thin safety net. The degrade-to-raw
  // CONTRACT is exhaustively tested in Rust (src-tauri/src/catalog/reply.rs), so no brittle
  // invoke-rejection test is duplicated here.
});
