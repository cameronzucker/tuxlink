import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Ics213Form } from './Ics213Form';

// Mock @tauri-apps/api/core for FormDraftLibrary IPC calls.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'form_draft_library_list') return [];
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'mock-slot-id',
        form_id: 'ICS213_Initial',
        label: 'Test Slot',
        payload: { to_name: 'JOHN', fm_name: 'JANE', subjectline: 'TEST', message: 'hello' },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  }),
}));

// Reset mock defaults before each test.
beforeEach(async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
    if (cmd === 'form_draft_library_list') return [];
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'mock-slot-id',
        form_id: 'ICS213_Initial',
        label: 'Test Slot',
        payload: { to_name: 'JOHN', fm_name: 'JANE', subjectline: 'TEST', message: 'hello' },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  });
});

describe('Ics213Form', () => {
  const noop = () => {};

  it('renders all ICS-213 input fields', () => {
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Incident Name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Addressee.*name and position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Originator.*name and position/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Subject/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Message/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Approved by/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Is exercise/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields empty', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with field values when required fields filled', () => {
    const onSubmit = vi.fn();
    render(<Ics213Form onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Addressee.*name and position/i), { target: { value: 'JOHN' } });
    fireEvent.change(screen.getByLabelText(/Originator.*name and position/i), { target: { value: 'JANE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'TEST' } });
    fireEvent.change(screen.getByLabelText(/Date/i), { target: { value: '2026-05-30' } });
    fireEvent.change(screen.getByLabelText(/Time/i), { target: { value: '14:30Z' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'hello' } });
    fireEvent.click(screen.getByTestId('ics213-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const values = onSubmit.mock.calls[0][0];
    expect(values.to_name).toBe('JOHN');
    expect(values.fm_name).toBe('JANE');
    expect(values.subjectline).toBe('TEST');
    expect(values.message).toBe('hello');
  });

  it('initialValues pre-populates fields', () => {
    render(<Ics213Form initialValues={{ inc_name: 'WALDO' }} onSubmit={noop} onCancel={noop} />);
    const incName = screen.getByLabelText(/Incident Name/i) as HTMLInputElement;
    expect(incName.value).toBe('WALDO');
  });

  it('calls onChange when a field is edited (controlled host pattern)', () => {
    const onChange = vi.fn();
    render(<Ics213Form onChange={onChange} onSubmit={noop} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Incident Name/i), { target: { value: 'WALDO' } });
    expect(onChange).toHaveBeenCalled();
    const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
    expect(lastCall.inc_name).toBe('WALDO');
  });

  // ── FormDraftLibrary slot tests ────────────────────────────────────────────

  it('lists saved slots on mount', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-1',
            form_id: 'ICS213_Initial',
            label: 'Net Template',
            payload: { to_name: 'NET CONTROL', fm_name: 'N7CPZ', subjectline: 'Check-in', message: 'Checking in' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return [];
    });
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    expect(await screen.findByText('Net Template')).toBeInTheDocument();
  });

  it('applies a slot payload when selected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-apply',
            form_id: 'ICS213_Initial',
            label: 'Pre-filled template',
            payload: { to_name: 'NET CONTROL', fm_name: 'N7CPZ', subjectline: 'Weekly check-in', message: 'All OK' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return [];
    });
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    await screen.findByText('Pre-filled template');
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'slot-apply' } });
    // Field state should reflect the slot payload
    expect((screen.getByLabelText(/Addressee.*name and position/i) as HTMLInputElement).value).toBe('NET CONTROL');
    expect((screen.getByLabelText(/Subject/i) as HTMLInputElement).value).toBe('Weekly check-in');
    expect((screen.getByLabelText(/Message/i) as HTMLTextAreaElement).value).toBe('All OK');
  });

  it('saves a new slot via the Save as slot… button', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Net Template');
    const { invoke } = await import('@tauri-apps/api/core');
    const mockInvoke = invoke as ReturnType<typeof vi.fn>;
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    // Fill required saveable fields
    fireEvent.change(screen.getByLabelText(/Addressee.*name and position/i), { target: { value: 'NET CONTROL' } });
    fireEvent.change(screen.getByLabelText(/Originator.*name and position/i), { target: { value: 'N7CPZ' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'Check-in' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'All OK' } });
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'form_draft_library_upsert');
      expect(upsertCalls.length).toBeGreaterThan(0);
      expect(upsertCalls[0][1]).toMatchObject({
        formId: 'ICS213_Initial',
        label: 'Net Template',
        payload: expect.objectContaining({ to_name: 'NET CONTROL', subjectline: 'Check-in' }),
      });
    });
    vi.restoreAllMocks();
  });

  it('deletes the selected slot', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-to-delete',
            form_id: 'ICS213_Initial',
            label: 'Old template',
            payload: {},
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      if (cmd === 'form_draft_library_delete') return undefined;
      return [];
    });
    render(<Ics213Form onSubmit={noop} onCancel={noop} />);
    await screen.findByText('Old template');
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'slot-to-delete' } });
    const deleteBtn = await screen.findByTestId('slot-delete-btn');
    fireEvent.click(deleteBtn);
    const { invoke: inv } = await import('@tauri-apps/api/core');
    await waitFor(() => {
      const deleteCalls = (inv as ReturnType<typeof vi.fn>).mock.calls.filter(
        (c) => c[0] === 'form_draft_library_delete',
      );
      expect(deleteCalls.length).toBeGreaterThan(0);
      expect(deleteCalls[0][1]).toEqual({ slotId: 'slot-to-delete' });
    });
    vi.restoreAllMocks();
  });
});
