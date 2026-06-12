// FormSequenceSettings — CSS-blind vitest (jsdom). Verifies the status fetch,
// empty state, row rendering, and the reset round-trip (forms_sequence_reset +
// refresh). The Tauri invoke boundary is mocked.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

import { FormSequenceSettings } from './FormSequenceSettings';

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(() => {
  cleanup();
});

describe('<FormSequenceSettings>', () => {
  it('renders the empty state when no form has a counter', async () => {
    mockInvoke.mockImplementation(async (cmd: string) =>
      cmd === 'forms_sequence_status' ? [] : undefined,
    );
    render(<FormSequenceSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('form-seq-empty')).toBeInTheDocument();
    });
  });

  it('lists each form with its next serial', async () => {
    mockInvoke.mockImplementation(async (cmd: string) =>
      cmd === 'forms_sequence_status'
        ? [
            { formId: 'IARU_Message_Form_Initial', nextSerial: 4 },
            { formId: 'RRI_Radiogram', nextSerial: 1 },
          ]
        : undefined,
    );
    render(<FormSequenceSettings />);
    await waitFor(() => {
      expect(screen.getByTestId('form-seq-row-IARU_Message_Form_Initial')).toHaveTextContent('next: 4');
    });
    expect(screen.getByTestId('form-seq-row-RRI_Radiogram')).toHaveTextContent('next: 1');
  });

  it('resets a form to a new next serial and refreshes', async () => {
    let serial = 4;
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'forms_sequence_status') {
        return [{ formId: 'IARU_Message_Form_Initial', nextSerial: serial }];
      }
      if (cmd === 'forms_sequence_reset') {
        serial = (args as { next: number }).next;
        return undefined;
      }
      return undefined;
    });
    render(<FormSequenceSettings />);
    await waitFor(() => screen.getByTestId('form-seq-input-IARU_Message_Form_Initial'));

    fireEvent.change(screen.getByTestId('form-seq-input-IARU_Message_Form_Initial'), {
      target: { value: '1' },
    });
    fireEvent.click(screen.getByTestId('form-seq-set-IARU_Message_Form_Initial'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('forms_sequence_reset', {
        formId: 'IARU_Message_Form_Initial',
        next: 1,
      });
    });
    // After reset + refresh the row reflects the new next serial.
    await waitFor(() => {
      expect(screen.getByTestId('form-seq-row-IARU_Message_Form_Initial')).toHaveTextContent('next: 1');
    });
  });

  it('rejects a non-positive next serial without calling reset', async () => {
    mockInvoke.mockImplementation(async (cmd: string) =>
      cmd === 'forms_sequence_status'
        ? [{ formId: 'IARU_Message_Form_Initial', nextSerial: 4 }]
        : undefined,
    );
    render(<FormSequenceSettings />);
    await waitFor(() => screen.getByTestId('form-seq-input-IARU_Message_Form_Initial'));

    fireEvent.change(screen.getByTestId('form-seq-input-IARU_Message_Form_Initial'), {
      target: { value: '0' },
    });
    fireEvent.click(screen.getByTestId('form-seq-set-IARU_Message_Form_Initial'));

    await waitFor(() => expect(screen.getByTestId('form-seq-error')).toBeInTheDocument());
    expect(mockInvoke).not.toHaveBeenCalledWith('forms_sequence_reset', expect.anything());
  });
});
