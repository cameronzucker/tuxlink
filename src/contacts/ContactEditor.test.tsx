// ContactEditor tests — the "Radio dials" manual-channel section
// (tuxlink-6vn4x, the Peers fix) plus the freq parser.
//
// The dials section edits MANUAL channels only: observed channels never render
// in the form and pass through onSave untouched. Frequencies display in MHz
// and store as integer freq_hz.

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';

import { ContactEditor, emptyContact, parseDialFreq } from './ContactEditor';
import type { Channel, Contact } from './types';

const NOW = '2026-07-11T09:00:00-07:00';

const OBSERVED_CH: Channel = {
  transport: 'vara-hf',
  target_callsign: 'N0DAJ',
  via: ['RELAY1'],
  freq_hz: 7_101_000,
  bandwidth: null,
  direction: 'incoming',
  counts: { ok: 3, fail: 1 },
  last_seen: NOW,
  last_ok: NOW,
  last_ok_direction: 'incoming',
  source: 'observed',
};

const MANUAL_CH: Channel = {
  transport: 'ardop',
  target_callsign: 'N0DAJ',
  via: [],
  freq_hz: 7_103_500,
  bandwidth: null,
  direction: 'unknown',
  counts: { ok: 0, fail: 0 },
  last_seen: '',
  last_ok: null,
  last_ok_direction: null,
  source: 'manual',
};

const DOUG: Contact = {
  id: 'c-doug',
  name: 'Doug Jarmuth',
  callsign: 'N0DAJ',
  channels: [OBSERVED_CH, MANUAL_CH],
  created_at: NOW,
  updated_at: NOW,
};

describe('parseDialFreq', () => {
  it('parses MHz with a decimal point: "7.1035" → 7_103_500 Hz', () => {
    expect(parseDialFreq('7.1035')).toBe(7_103_500);
  });

  it('tolerates a raw-Hz paste (no dot, > 100000): "7103500" → 7_103_500', () => {
    expect(parseDialFreq('7103500')).toBe(7_103_500);
  });

  it('treats a small dot-less number as MHz: "146" → 146_000_000', () => {
    expect(parseDialFreq('146')).toBe(146_000_000);
  });

  it('rejects empty / non-numeric / non-positive input', () => {
    expect(parseDialFreq('')).toBeNull();
    expect(parseDialFreq('  ')).toBeNull();
    expect(parseDialFreq('seven')).toBeNull();
    expect(parseDialFreq('-7.1')).toBeNull();
    expect(parseDialFreq('0')).toBeNull();
  });
});

describe('<ContactEditor> — Radio dials', () => {
  it('renders only MANUAL channels as dial rows (observed rows never appear)', () => {
    render(<ContactEditor contact={DOUG} onSave={vi.fn()} onCancel={vi.fn()} />);
    // One dial row: the manual ARDOP channel, displayed in MHz.
    const row = screen.getByTestId('editor-dial-0');
    expect(within(row).getByTestId('editor-dial-transport-0')).toHaveValue('ardop');
    expect(within(row).getByTestId('editor-dial-freq-0')).toHaveValue('7.1035');
    // The observed VARA HF channel is NOT an editable row.
    expect(screen.queryByTestId('editor-dial-1')).not.toBeInTheDocument();
  });

  it('"+ add dial" adds a row; saving includes the manual channel with the parsed freq_hz', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={emptyContact('W7XYZ')} onSave={onSave} onCancel={vi.fn()} />);

    fireEvent.click(screen.getByTestId('editor-dial-add'));
    fireEvent.change(screen.getByTestId('editor-dial-transport-0'), {
      target: { value: 'vara-hf' },
    });
    fireEvent.change(screen.getByTestId('editor-dial-freq-0'), { target: { value: '7.1035' } });
    fireEvent.click(screen.getByTestId('editor-save'));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.channels).toEqual([
      expect.objectContaining({
        transport: 'vara-hf',
        freq_hz: 7_103_500,
        source: 'manual',
        target_callsign: 'W7XYZ',
        via: [],
        counts: { ok: 0, fail: 0 },
        last_ok: null,
      }),
    ]);
  });

  it('save passes observed channels through untouched and round-trips an unchanged manual dial', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={DOUG} onSave={onSave} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('editor-save'));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    // Observed first (verbatim), then the untouched manual dial (verbatim —
    // its history fields survive an edit that did not change the dial).
    expect(saved.channels).toEqual([OBSERVED_CH, MANUAL_CH]);
  });

  it('a changed dial becomes a fresh manual channel; a removed dial is dropped; observed survives', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={DOUG} onSave={onSave} onCancel={vi.fn()} />);

    // Change the existing manual dial's frequency.
    fireEvent.change(screen.getByTestId('editor-dial-freq-0'), { target: { value: '10.1442' } });
    fireEvent.click(screen.getByTestId('editor-save'));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.channels).toEqual([
      OBSERVED_CH,
      expect.objectContaining({ transport: 'ardop', freq_hz: 10_144_200, source: 'manual' }),
    ]);
  });

  it('the remove button deletes the dial row; save omits it', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={DOUG} onSave={onSave} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('editor-dial-remove-0'));
    expect(screen.queryByTestId('editor-dial-0')).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId('editor-save'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.channels).toEqual([OBSERVED_CH]);
  });

  it('a dial row with an empty/unparseable frequency is dropped on save (a dial needs a frequency)', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={emptyContact('W7XYZ')} onSave={onSave} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByTestId('editor-dial-add'));
    // Frequency left blank.
    fireEvent.click(screen.getByTestId('editor-save'));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const saved = onSave.mock.calls[0][0];
    expect(saved.channels).toEqual([]);
  });

  it('identity fields still save (callsign/name/email/tactical/notes unchanged by the dials work)', async () => {
    const onSave = vi.fn<(c: Contact) => Promise<void>>(async () => {});
    render(<ContactEditor contact={emptyContact()} onSave={onSave} onCancel={vi.fn()} />);
    fireEvent.change(screen.getByTestId('editor-callsign'), { target: { value: 'N0DXE' } });
    fireEvent.change(screen.getByTestId('editor-name'), { target: { value: 'Dixie' } });
    fireEvent.click(screen.getByTestId('editor-save'));
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        expect.objectContaining({ callsign: 'N0DXE', name: 'Dixie' }),
      ),
    );
  });
});
