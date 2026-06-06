import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Ics309FormV2 } from './Ics309FormV2';
import type { LogRow } from './Ics309FormV2';

// ── Mock Tauri IPC ────────────────────────────────────────────────────────────

// NOTE: vi.mock is hoisted — factory cannot reference outer-scope lets/consts
// declared after the vi.mock call. Use vi.fn() inside the factory and obtain the
// mock via import after the mock is registered.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const SAMPLE_ROWS: LogRow[] = [
  {
    datetime: '2024-05-20T10:13:00Z',
    from: 'N7CPZ',
    to: 'W1AW',
    subject: 'DAMAGE REPORT - SECTOR 7',
    direction: 'out',
  },
  {
    datetime: '2024-05-20T10:15:00Z',
    from: 'W1AW',
    to: 'N7CPZ',
    subject: 'RE: DAMAGE REPORT ACK',
    direction: 'in',
  },
];

// Reset mock implementation before each test so per-test overrides don't bleed.
beforeEach(async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  const mockInvoke = invoke as ReturnType<typeof vi.fn>;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'messages_meta_query_for_log') return SAMPLE_ROWS;
    if (cmd === 'render_ics309_pdf') {
      // Return minimal PDF magic bytes as a number array (Rust tests the real PDF).
      return Array.from(new TextEncoder().encode('%PDF-1.4\n%%EOF\n'));
    }
    return [];
  });
});

// ── Render helper ─────────────────────────────────────────────────────────────

function renderForm(props: Partial<React.ComponentProps<typeof Ics309FormV2>> = {}) {
  return render(
    <Ics309FormV2
      onSubmit={vi.fn()}
      onCancel={vi.fn()}
      {...props}
    />
  );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('<Ics309FormV2>', () => {
  // 1. Renders the time-range preset buttons
  it('renders the time-range preset buttons', () => {
    renderForm();
    expect(screen.getByTestId('preset-last-hour')).toBeInTheDocument();
    expect(screen.getByTestId('preset-today')).toBeInTheDocument();
    expect(screen.getByTestId('preset-op-period')).toBeInTheDocument();
    expect(screen.getByTestId('preset-custom')).toBeInTheDocument();
  });

  // 2. Picking "today" runs the query and shows preview rows
  it('picking "today" runs the query and shows preview rows', async () => {
    renderForm();
    fireEvent.click(screen.getByTestId('preset-today'));
    await waitFor(() => {
      expect(screen.getByTestId('preview-table')).toBeInTheDocument();
    });
    // Both sample rows are rendered
    expect(screen.getByText('DAMAGE REPORT - SECTOR 7')).toBeInTheDocument();
    expect(screen.getByText('RE: DAMAGE REPORT ACK')).toBeInTheDocument();
  });

  // 3. Preview is empty before the operator picks a range
  it('shows empty state before a range is selected', () => {
    // No preset is clicked — initial rows state is empty — empty hint visible.
    renderForm();
    expect(screen.getByTestId('preview-empty')).toBeInTheDocument();
  });

  // 4. Send is disabled until preview has rows
  it('Send button is disabled until preview has rows', async () => {
    // Initial state: no rows → Send disabled.
    renderForm();
    const sendBtn = screen.getByTestId('send-btn');
    expect(sendBtn).toBeDisabled();

    // Pick a preset → mock returns SAMPLE_ROWS → rows load → Send enabled.
    fireEvent.click(screen.getByTestId('preset-today'));
    await waitFor(() => {
      expect(sendBtn).not.toBeDisabled();
    });
  });

  // 5. CSV download click produces a Blob with the expected header + rows
  it('CSV download creates a blob with correct header and row count', async () => {
    // Load rows by picking a preset
    renderForm();
    fireEvent.click(screen.getByTestId('preset-today'));
    await waitFor(() => {
      expect(screen.getByTestId('csv-download-btn')).not.toBeDisabled();
    });

    // Spy on URL.createObjectURL and document.createElement to capture the Blob
    const createdBlobs: Blob[] = [];
    const origCreateObjectURL = URL.createObjectURL;
    URL.createObjectURL = vi.fn((blob) => {
      createdBlobs.push(blob as Blob);
      return 'blob:mock';
    });
    const origRevoke = URL.revokeObjectURL;
    URL.revokeObjectURL = vi.fn();

    // Stub anchor click so the test doesn't navigate
    const clickSpy = vi.fn();
    const origCreate = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tag) => {
      const el = origCreate(tag);
      if (tag === 'a') {
        vi.spyOn(el, 'click').mockImplementation(clickSpy);
      }
      return el;
    });

    fireEvent.click(screen.getByTestId('csv-download-btn'));

    await waitFor(() => expect(createdBlobs.length).toBeGreaterThan(0));
    const blob = createdBlobs[0];
    expect(blob.type).toBe('text/csv');
    const text = await blob.text();
    // Header present
    expect(text).toContain('Datetime (UTC),Dir,From,To,Subject');
    // Both rows encoded
    expect(text).toContain('DAMAGE REPORT - SECTOR 7');
    expect(text).toContain('RE: DAMAGE REPORT ACK');

    // Restore
    URL.createObjectURL = origCreateObjectURL;
    URL.revokeObjectURL = origRevoke;
    vi.restoreAllMocks();
  });

  // 6. PDF download click invokes render_ics309_pdf with the rows + range
  it('PDF download invokes render_ics309_pdf with rows and range', async () => {
    renderForm();
    fireEvent.click(screen.getByTestId('preset-today'));
    await waitFor(() => {
      expect(screen.getByTestId('pdf-download-btn')).not.toBeDisabled();
    });

    // Stub anchor so we don't navigate
    URL.createObjectURL = vi.fn(() => 'blob:mock');
    URL.revokeObjectURL = vi.fn();
    const origCreate = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tag) => {
      const el = origCreate(tag);
      if (tag === 'a') vi.spyOn(el, 'click').mockImplementation(vi.fn());
      return el;
    });

    fireEvent.click(screen.getByTestId('pdf-download-btn'));

    const { invoke } = await import('@tauri-apps/api/core');
    const capturedInvoke = invoke as ReturnType<typeof vi.fn>;

    await waitFor(() => {
      const calls = capturedInvoke.mock.calls.filter((call) => call[0] === 'render_ics309_pdf');
      expect(calls.length).toBeGreaterThan(0);
    });
    const pdfCall = capturedInvoke.mock.calls.find((call) => call[0] === 'render_ics309_pdf');
    expect(pdfCall).toBeTruthy();
    const pdfArg = pdfCall![1] as { req: { rows: LogRow[]; rangeStart: string; rangeEnd: string } };
    // Rows are passed through
    expect(pdfArg.req.rows).toHaveLength(2);
    expect(pdfArg.req.rows[0].subject).toBe('DAMAGE REPORT - SECTOR 7');
    // Range strings are ISO strings
    expect(pdfArg.req.rangeStart).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(pdfArg.req.rangeEnd).toMatch(/^\d{4}-\d{2}-\d{2}T/);

    vi.restoreAllMocks();
  });

  // 7. initialValues rehydration: restore draft range + rows on mount
  it('rehydrates preset, range, and rows from initialValues on mount', () => {
    const rows = JSON.stringify(SAMPLE_ROWS);
    renderForm({
      initialValues: {
        preset: 'custom',
        rangeStart: '2024-05-20T00:00:00.000Z',
        rangeEnd: '2024-05-20T23:59:59.000Z',
        rows,
      },
    });
    // Custom preset button should be active
    const customBtn = screen.getByTestId('preset-custom');
    expect(customBtn).toHaveAttribute('aria-pressed', 'true');
    // Rows from draft are rendered immediately (no query needed)
    expect(screen.getByTestId('preview-table')).toBeInTheDocument();
    expect(screen.getByText('DAMAGE REPORT - SECTOR 7')).toBeInTheDocument();
  });

  // Additional: Send emits the correct wire-format field IDs
  it('Send emits wire-format field IDs for Form-309_Initial', async () => {
    const onSubmit = vi.fn();
    renderForm({ onSubmit });
    fireEvent.click(screen.getByTestId('preset-today'));
    await waitFor(() => {
      expect(screen.getByTestId('send-btn')).not.toBeDisabled();
    });
    fireEvent.click(screen.getByTestId('send-btn'));
    expect(onSubmit).toHaveBeenCalledOnce();
    const payload = onSubmit.mock.calls[0][0] as Record<string, string>;
    // Wire-format header fields
    expect(payload.title).toBeTruthy();
    expect(payload.activitydatetime1).toBeTruthy();
    // Numbered row fields — time1, from1, to1, sub1 must be present
    expect(payload.time1).toBeTruthy();
    expect(payload.from1).toBe('N7CPZ');
    expect(payload.to1).toBe('W1AW');
    expect(payload.sub1).toContain('DAMAGE REPORT - SECTOR 7');
    // Must NOT send raw UI keys
    expect('rows' in payload).toBe(false);
    expect('preset' in payload).toBe(false);
    expect('rangeStart' in payload).toBe(false);
  });

  // onChange fires from preset selection (not from useEffect dep)
  it('onChange fires with UI-shape payload when a preset is picked', async () => {
    const onChange = vi.fn();
    renderForm({ onChange });
    fireEvent.click(screen.getByTestId('preset-last-hour'));
    await waitFor(() => {
      expect(onChange).toHaveBeenCalled();
      const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1];
      const arg = lastCall[0] as Record<string, string>;
      expect(arg.preset).toBe('last-hour');
      expect(arg.rangeStart).toBeTruthy();
      expect(arg.rangeEnd).toBeTruthy();
    });
  });
});
