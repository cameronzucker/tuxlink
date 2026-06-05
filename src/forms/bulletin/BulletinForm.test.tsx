import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { BulletinForm } from './BulletinForm';

// Mock @tauri-apps/api/core for FormDraftLibrary IPC calls.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'form_draft_library_list') return [];
    if (cmd === 'form_draft_library_upsert') {
      return {
        slot_id: 'mock-slot-id',
        form_id: 'Bulletin_Initial',
        label: 'Test Slot',
        payload: { level: 'ROUTINE', subjectline: 'Net update', name: 'ALL', from_name: 'W1AW', message: 'Net moved.' },
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
        form_id: 'Bulletin_Initial',
        label: 'Test Slot',
        payload: { level: 'ROUTINE', subjectline: 'Net update', name: 'ALL', from_name: 'W1AW', message: 'Net moved.' },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      };
    }
    if (cmd === 'form_draft_library_delete') return undefined;
    return null;
  });
});

describe('BulletinForm', () => {
  const noop = () => {};

  it('renders all bulletin fields', () => {
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    expect(screen.getByLabelText(/Precedence Level/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Subject/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Bulletin #/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/For.*Recipient/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Bulletin From/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Date\/Time/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Message/i)).toBeInTheDocument();
  });

  it('blocks submit when required fields are empty', () => {
    const onSubmit = vi.fn();
    render(<BulletinForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.click(screen.getByTestId('bulletin-submit'));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('submits with values when required fields filled', () => {
    const onSubmit = vi.fn();
    render(<BulletinForm onSubmit={onSubmit} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Precedence Level/i), { target: { value: 'ROUTINE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'Net schedule update' } });
    fireEvent.change(screen.getByLabelText(/Bulletin #/i), { target: { value: '42' } });
    fireEvent.change(screen.getByLabelText(/For.*Recipient/i), { target: { value: 'ALL' } });
    fireEvent.change(screen.getByLabelText(/Bulletin From/i), { target: { value: 'W1AW' } });
    fireEvent.change(screen.getByLabelText(/Date\/Time/i), { target: { value: '2026-05-31 09:00Z' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'Net moved to 0930 local.' } });
    fireEvent.click(screen.getByTestId('bulletin-submit'));
    expect(onSubmit).toHaveBeenCalled();
    const vals = onSubmit.mock.calls[0][0];
    expect(vals.level).toBe('ROUTINE');
    expect(vals.bullnr).toBe('42');
    expect(vals.message).toBe('Net moved to 0930 local.');
  });

  // ── FormDraftLibrary slot tests ────────────────────────────────────────────

  it('lists saved slots on mount', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-1',
            form_id: 'Bulletin_Initial',
            label: 'Weekly Net Bulletin',
            payload: { level: 'ROUTINE', subjectline: 'Net update', name: 'ALL', from_name: 'W1AW', message: 'Net active.' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return [];
    });
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    expect(await screen.findByText('Weekly Net Bulletin')).toBeInTheDocument();
  });

  it('applies a slot payload when selected', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    (invoke as ReturnType<typeof vi.fn>).mockImplementation(async (cmd: string) => {
      if (cmd === 'form_draft_library_list') {
        return [
          {
            slot_id: 'slot-apply',
            form_id: 'Bulletin_Initial',
            label: 'EMCOMM template',
            payload: { level: 'PRIORITY', subjectline: 'ARES activation', name: 'ALL ARES', from_name: 'K7EC', message: 'Net activated.' },
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      return [];
    });
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    await screen.findByText('EMCOMM template');
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'slot-apply' } });
    expect((screen.getByLabelText(/Precedence Level/i) as HTMLInputElement).value).toBe('PRIORITY');
    expect((screen.getByLabelText(/Subject/i) as HTMLInputElement).value).toBe('ARES activation');
    expect((screen.getByLabelText(/Message/i) as HTMLTextAreaElement).value).toBe('Net activated.');
  });

  it('saves a new slot via the Save as slot… button', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('Weekly Net Bulletin');
    const { invoke } = await import('@tauri-apps/api/core');
    const mockInvoke = invoke as ReturnType<typeof vi.fn>;
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    fireEvent.change(screen.getByLabelText(/Precedence Level/i), { target: { value: 'ROUTINE' } });
    fireEvent.change(screen.getByLabelText(/Subject/i), { target: { value: 'Net update' } });
    fireEvent.change(screen.getByLabelText(/For.*Recipient/i), { target: { value: 'ALL' } });
    fireEvent.change(screen.getByLabelText(/Bulletin From/i), { target: { value: 'W1AW' } });
    fireEvent.change(screen.getByLabelText(/Message/i), { target: { value: 'Net active.' } });
    mockInvoke.mockClear();
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCalls = mockInvoke.mock.calls.filter((c) => c[0] === 'form_draft_library_upsert');
      expect(upsertCalls.length).toBeGreaterThan(0);
      expect(upsertCalls[0][1]).toMatchObject({
        formId: 'Bulletin_Initial',
        label: 'Weekly Net Bulletin',
        payload: expect.objectContaining({ level: 'ROUTINE', subjectline: 'Net update' }),
      });
      // Volatile fields must NOT be in the payload
      expect(upsertCalls[0][1].payload).not.toHaveProperty('bullnr');
      expect(upsertCalls[0][1].payload).not.toHaveProperty('activitydatetime1');
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
            form_id: 'Bulletin_Initial',
            label: 'Old bulletin',
            payload: {},
            created_at: '2026-06-04T12:00:00Z',
            updated_at: '2026-06-04T12:00:00Z',
          },
        ];
      }
      if (cmd === 'form_draft_library_delete') return undefined;
      return [];
    });
    render(<BulletinForm onSubmit={noop} onCancel={noop} />);
    await screen.findByText('Old bulletin');
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
