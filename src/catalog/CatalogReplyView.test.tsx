import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { CatalogReplyView } from './CatalogReplyView';
import type { ReplyView } from './stationTypes';

beforeEach(() => vi.mocked(invoke).mockReset());

function mock(view: ReplyView) {
  vi.mocked(invoke).mockResolvedValue(view as unknown as never);
}

describe('CatalogReplyView (tuxlink-qyjr)', () => {
  it('renders a Tabular State Forecast as a table (locations × days) + raw toggle', async () => {
    mock({
      kind: 'area-weather',
      product: 'FPUS65 KPSR 090626',
      office: 'National Weather Service Phoenix AZ',
      issued: '1126 PM MST Mon Jun 8 2026',
      title: 'Tabular State Forecast for Southwest Arizona',
      raw: 'RAWBODY',
      forecast: {
        kind: 'tabular',
        days: [
          { dow: 'Tue', date: 'Jun 09' },
          { dow: 'Wed', date: 'Jun 10' },
        ],
        regions: [
          {
            name: 'SOUTH-CENTRAL ARIZONA',
            locations: [
              {
                name: 'Phoenix',
                cells: [
                  { condition: 'Vryhot', low: '77', high: '106', popNight: '00', popDay: '00' },
                  { condition: 'Sunny', low: '77', high: '108', popNight: '00', popDay: '20' },
                ],
              },
            ],
          },
        ],
      },
    });
    render(<CatalogReplyView subject="INQUIRY - https://tgftp.nws.noaa.gov/x" body="b" />);

    expect(await screen.findByText('Phoenix')).toBeTruthy();
    expect(screen.getByText('Tabular State Forecast for Southwest Arizona')).toBeTruthy();
    // Day columns rendered.
    expect(screen.getByText('Tue')).toBeTruthy();
    expect(screen.getByText('Jun 10')).toBeTruthy();
    // A cell with the high temp + condition.
    expect(screen.getByText('106')).toBeTruthy();
    expect(screen.getAllByText('Vryhot').length).toBeGreaterThan(0);
    // Precip surfaced as a percentage (20% on the wet day).
    expect(screen.getByText('20%')).toBeTruthy();

    fireEvent.click(screen.getByTestId('catalog-reply-toggle'));
    expect(screen.getByText('RAWBODY')).toBeTruthy();
  });

  it('renders a Zone Forecast Product as zone sections with title-cased period labels', async () => {
    mock({
      kind: 'area-weather',
      product: 'FPUS55 KFGZ 090632',
      office: 'National Weather Service Flagstaff AZ',
      issued: '1132 PM MST Mon Jun 8 2026',
      title: 'Zone Forecast Product for Northern Arizona',
      raw: 'RAWBODY',
      forecast: {
        kind: 'zone',
        zones: [
          {
            name: 'Western Mogollon Rim',
            cities: 'Flagstaff, Williams, and Munds Park',
            periods: [
              { label: 'REST OF TONIGHT', text: 'Mostly clear. Lows 43 to 53.' },
              { label: 'TUESDAY', text: 'Windy, sunny. Highs 77 to 85.' },
            ],
          },
        ],
      },
    });
    render(<CatalogReplyView subject="INQUIRY - https://tgftp.nws.noaa.gov/x" body="b" />);

    expect(await screen.findByText('Western Mogollon Rim')).toBeTruthy();
    expect(screen.getByText('Flagstaff, Williams, and Munds Park')).toBeTruthy();
    // UPPERCASE NWS label is title-cased for display.
    expect(screen.getByText('Rest of Tonight')).toBeTruthy();
    expect(screen.getByText('Tuesday')).toBeTruthy();
    expect(screen.getByText(/Windy, sunny\. Highs 77 to 85\./)).toBeTruthy();
  });

  it('renders header + raw for a recognised NWS product with no structured forecast (kind: none)', async () => {
    mock({
      kind: 'area-weather',
      product: 'FPUS65 KPSR 090626',
      office: 'National Weather Service Phoenix AZ',
      issued: '',
      title: 'Some Other Product',
      raw: 'FULLTEXT',
      forecast: { kind: 'none' },
    });
    render(<CatalogReplyView subject="INQUIRY - https://tgftp.nws.noaa.gov/x" body="b" />);
    expect(await screen.findByText('Some Other Product')).toBeTruthy();
    // Toggle reads "Show full text" when there's no structured body.
    fireEvent.click(screen.getByRole('button', { name: /show full text/i }));
    expect(screen.getByText('FULLTEXT')).toBeTruthy();
  });

  it('renders raw when the parser returns raw (struct variant: { kind:"raw", text })', async () => {
    mock({ kind: 'raw', text: 'just text' });
    render(<CatalogReplyView subject="Service Advice Message" body="just text" />);
    await waitFor(() => expect(screen.getByText('just text')).toBeTruthy());
  });

  // The degrade-to-raw CONTRACT is exhaustively tested in Rust
  // (src-tauri/src/catalog/reply.rs); no brittle invoke-rejection test is duplicated here.
});
