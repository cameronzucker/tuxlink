/**
 * CheckInForm.test.tsx — WLE-aligned Winlink Check-In form tests.
 *
 * The field schema mirrors `Winlink_Check_In_Initial.html` per bd tuxlink-4ai0
 * (Codex 2026-06-04 P1 finding on PR #392). Tests cover:
 *   - Plan-spec parity (4 plan-required tests adapted to WLE field set)
 *   - Wire-format alignment (every checkin.rs::FIELDS key emitted on submit)
 *   - Auto-fill from config_read + position_current_fix
 *   - WLE-correct defaults (EXERCISE / AMATEUR / NA / Telnet)
 *   - All 4 radio groups (Status / Service / Band / Session)
 *   - FormDraftLibrary slot save / apply / delete
 *   - Required-field gating of Send
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

const mocks = vi.hoisted(() => {
  const invoke = vi.fn();
  return { invoke };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));

// eslint-disable-next-line import/first
import { CheckInForm } from './CheckInForm';

const DEFAULT_INVOKE = async (cmd: string) => {
  if (cmd === 'config_read') return { callsign: 'W7CPZ', identifier: 'John Smith' };
  if (cmd === 'position_current_fix') return { grid: 'CN87us', source: 'gps', fresh: true };
  if (cmd === 'form_draft_library_list') {
    return [
      {
        slot_id: 'slot-cascadia-net',
        form_id: 'Winlink_Check-In',
        label: 'Cascadia ARES Net',
        payload: {
          organization: 'Cascadia ARES Net',
          msgto: 'WL-NET',
          contactname: 'Net Control',
          assigned: 'W7CPZ',
          status: 'REAL EVENT',
          service: 'AMATEUR',
          band: 'HF',
          session: 'VARA HF',
        },
        created_at: '2026-06-04T12:00:00Z',
        updated_at: '2026-06-04T12:00:00Z',
      },
    ];
  }
  if (cmd === 'form_draft_library_upsert') {
    return {
      slot_id: 'new-slot-uuid',
      form_id: 'Winlink_Check-In',
      label: 'Test Slot',
      payload: {},
      created_at: '2026-06-05T00:00:00Z',
      updated_at: '2026-06-05T00:00:00Z',
    };
  }
  if (cmd === 'form_draft_library_delete') return undefined;
  return null;
};

describe('<CheckInForm> — plan-spec parity (WLE-aligned)', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  afterEach(() => { vi.clearAllMocks(); });

  it('pre-fills MsgSender (From callsign) from config', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('W7CPZ')).toBeInTheDocument();
  });

  it('renders the saved-slot dropdown', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByText(/Cascadia ARES Net/)).toBeInTheDocument();
  });

  it('clicking a saved slot applies its payload to the form', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // Wait for slot option to render before firing the change event (the
    // race fix from PR #392 commit a2c4169 — findByRole(combobox) resolves
    // synchronously but options come from async setSlots).
    await screen.findByText(/Cascadia ARES Net/);
    fireEvent.change(screen.getByLabelText(/saved slots/i), {
      target: { value: 'slot-cascadia-net' },
    });
    expect((screen.getByLabelText(/^Organization$/i) as HTMLInputElement).value).toBe('Cascadia ARES Net');
  });

  it('Status defaults to EXERCISE (WLE template default)', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ'); // wait for mount effects
    expect((screen.getByLabelText(/^EXERCISE$/i) as HTMLInputElement).checked).toBe(true);
  });
});

describe('<CheckInForm> — auto-fill from PositionArbiter + config', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  it('pre-fills Grid Square from position_current_fix (uppercased)', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('CN87US')).toBeInTheDocument();
  });

  it('pre-fills Station Contact Name from config.identifier', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    expect(await screen.findByDisplayValue('John Smith')).toBeInTheDocument();
  });

  it('sets locationsource to GPS when PositionArbiter has a fresh fix', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('CN87US'); // wait for GPS effect
    expect((screen.getByLabelText(/Location Source/i) as HTMLInputElement).value).toBe('GPS');
  });

  it('leaves locationsource as Operator when PositionArbiter has no grid', async () => {
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'position_current_fix') return { grid: null, source: 'config', fresh: false };
      return DEFAULT_INVOKE(cmd);
    });
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    expect((screen.getByLabelText(/Location Source/i) as HTMLInputElement).value).toBe('Operator');
  });
});

describe('<CheckInForm> — wire-format alignment (WLE Winlink_Check_In_Initial)', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  it('onSubmit payload emits every key in checkin.rs::FIELDS', async () => {
    const onSubmit = vi.fn();
    render(<CheckInForm onSubmit={onSubmit} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    // Fill remaining required fields so Send is enabled.
    fireEvent.change(screen.getByLabelText(/^Subject$/i),
      { target: { value: 'Weekly check-in' } });
    fireEvent.change(screen.getByLabelText(/^To$/i),
      { target: { value: 'WL-NET' } });
    fireEvent.click(screen.getByTestId('checkin-send-btn'));
    await waitFor(() => expect(onSubmit).toHaveBeenCalled());

    const payload = onSubmit.mock.calls[0][0] as Record<string, string>;
    const expectedKeys = [
      'organization', 'newsubject', 'exercise_id',
      'datetime', 'msgto', 'msgsender', 'contactname', 'assigned',
      'status', 'service', 'band', 'session',
      'location', 'maplat', 'maplon', 'mgrs', 'grid', 'locationsource',
      'comments',
      'templateversion', 'mapfilename',
    ];
    for (const key of expectedKeys) {
      expect(payload, `missing wire-format key: ${key}`).toHaveProperty(key);
    }
    // Spot-check concrete values to confirm the form's state actually
    // flowed into the payload (not just empty-string placeholders).
    expect(payload.msgsender).toBe('W7CPZ');
    expect(payload.contactname).toBe('John Smith');
    expect(payload.grid).toBe('CN87US');
    expect(payload.newsubject).toBe('Weekly check-in');
    expect(payload.msgto).toBe('WL-NET');
    expect(payload.organization).toBe('Winlink Net'); // default
    expect(payload.status).toBe('EXERCISE');          // default
    expect(payload.service).toBe('AMATEUR');          // default
    expect(payload.band).toBe('NA');                  // default
    expect(payload.session).toBe('Telnet');           // default
    expect(payload.templateversion).toBe('Winlink_Check_In_Initial V5');
    expect(payload.mapfilename).toBe('Winlink Check-in V5');
    // datetime is auto-refreshed at submit time — assert format, not value
    expect(payload.datetime).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });
});

describe('<CheckInForm> — radio groups', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  it('Type radio switches between EXERCISE and REAL EVENT', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const real = screen.getByLabelText(/REAL EVENT/i) as HTMLInputElement;
    fireEvent.click(real);
    expect(real.checked).toBe(true);
    expect((screen.getByLabelText(/^EXERCISE$/i) as HTMLInputElement).checked).toBe(false);
  });

  it('Service radio switches between AMATEUR and SHARES', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const shares = screen.getByLabelText(/^SHARES$/i) as HTMLInputElement;
    fireEvent.click(shares);
    expect(shares.checked).toBe(true);
  });

  it('Band radio cycles through NA / HF / VHF', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const hf = screen.getByLabelText(/^HF$/i) as HTMLInputElement;
    fireEvent.click(hf);
    expect(hf.checked).toBe(true);
    const vhf = screen.getByLabelText(/^VHF$/i) as HTMLInputElement;
    fireEvent.click(vhf);
    expect(vhf.checked).toBe(true);
    expect(hf.checked).toBe(false);
  });

  it('Session-mode radio selects VARA HF', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    const vara = screen.getByLabelText(/^VARA HF$/i) as HTMLInputElement;
    fireEvent.click(vara);
    expect(vara.checked).toBe(true);
  });
});

describe('<CheckInForm> — FormDraftLibrary slot integration', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  it('applying a slot updates organization + msgto + status + band + session (not msgsender/grid/etc)', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/Cascadia ARES Net/);
    fireEvent.change(screen.getByLabelText(/saved slots/i), {
      target: { value: 'slot-cascadia-net' },
    });
    // Saveable fields applied
    expect((screen.getByLabelText(/^Organization$/i) as HTMLInputElement).value).toBe('Cascadia ARES Net');
    expect((screen.getByLabelText(/^To$/i) as HTMLInputElement).value).toBe('WL-NET');
    expect((screen.getByLabelText(/REAL EVENT/i) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^HF$/i) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText(/^VARA HF$/i) as HTMLInputElement).checked).toBe(true);
    // Volatile fields unchanged
    expect((screen.getByLabelText(/From \(Callsign\)/i) as HTMLInputElement).value).toBe('W7CPZ');
    expect((screen.getByLabelText(/^Grid Square$/i) as HTMLInputElement).value).toBe('CN87US');
  });

  it('saving a slot calls form_draft_library_upsert with only the slot-saveable fields', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('My Net Template');
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    fireEvent.change(screen.getByLabelText(/^Organization$/i),
      { target: { value: 'Test Org' } });
    fireEvent.change(screen.getByLabelText(/^To$/i),
      { target: { value: 'NET-CONTROL' } });
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await waitFor(() => {
      const upsertCalls = mocks.invoke.mock.calls.filter(
        (c) => c[0] === 'form_draft_library_upsert',
      );
      expect(upsertCalls.length).toBeGreaterThan(0);
      const args = upsertCalls[0][1];
      expect(args.label).toBe('My Net Template');
      const payload = args.payload as Record<string, string>;
      // Saveable fields present
      expect(payload.organization).toBe('Test Org');
      expect(payload.msgto).toBe('NET-CONTROL');
      expect(payload.status).toBe('EXERCISE');
      expect(payload.service).toBe('AMATEUR');
      expect(payload.band).toBe('NA');
      expect(payload.session).toBe('Telnet');
      // Volatile fields absent (slot only saves "which net this is", not
      // per-checkin state)
      expect(payload).not.toHaveProperty('newsubject');
      expect(payload).not.toHaveProperty('msgsender');
      expect(payload).not.toHaveProperty('grid');
      expect(payload).not.toHaveProperty('datetime');
      expect(payload).not.toHaveProperty('comments');
    });
  });

  it('window.prompt with whitespace-only input does NOT save a slot', async () => {
    vi.spyOn(window, 'prompt').mockReturnValue('   ');
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ');
    fireEvent.click(screen.getByTestId('slot-save-btn'));
    await new Promise((r) => setTimeout(r, 50));
    const upsertCalls = mocks.invoke.mock.calls.filter(
      (c) => c[0] === 'form_draft_library_upsert',
    );
    expect(upsertCalls.length).toBe(0);
  });

  it('deleting the selected slot calls form_draft_library_delete', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/Cascadia ARES Net/);
    fireEvent.change(screen.getByLabelText(/saved slots/i), {
      target: { value: 'slot-cascadia-net' },
    });
    fireEvent.click(screen.getByTestId('slot-delete-btn'));
    await waitFor(() => {
      const deleteCalls = mocks.invoke.mock.calls.filter(
        (c) => c[0] === 'form_draft_library_delete',
      );
      expect(deleteCalls.length).toBeGreaterThan(0);
      expect(deleteCalls[0][1]).toEqual({ slotId: 'slot-cascadia-net' });
    });
  });
});

describe('<CheckInForm> — required-field gating', () => {
  beforeEach(() => {
    mocks.invoke.mockClear();
    mocks.invoke.mockImplementation(DEFAULT_INVOKE);
  });

  it('Send is disabled when required fields are empty', async () => {
    // Override DEFAULT_INVOKE so config_read returns nothing — msgsender +
    // contactname stay empty, gating Send.
    mocks.invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'config_read') return {};
      return DEFAULT_INVOKE(cmd);
    });
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    // Wait for the mount effects (even though they returned nothing useful).
    await new Promise((r) => setTimeout(r, 50));
    expect((screen.getByTestId('checkin-send-btn') as HTMLButtonElement).disabled).toBe(true);
  });

  it('Send is enabled once all required fields are populated', async () => {
    render(<CheckInForm onSubmit={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByDisplayValue('W7CPZ'); // msgsender + contactname auto-fill
    fireEvent.change(screen.getByLabelText(/^Subject$/i),
      { target: { value: 'Weekly check-in' } });
    fireEvent.change(screen.getByLabelText(/^To$/i),
      { target: { value: 'WL-NET' } });
    await waitFor(() => {
      expect((screen.getByTestId('checkin-send-btn') as HTMLButtonElement).disabled).toBe(false);
    });
  });
});
