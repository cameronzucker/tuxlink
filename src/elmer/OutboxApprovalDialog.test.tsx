/**
 * OutboxApprovalDialog tests — Task 11 (AC-3, AC-10, AC-12).
 *
 * Tests:
 *   - Manifest lists all staged records.
 *   - "ALL N messages" header.
 *   - Remove calls onRemove(mid).
 *   - Digest mismatch shows the re-review state.
 *
 * Mock strategy: invoke is command-gated. Vitest calls invoke mocks with NO
 * args at teardown — gate every branch on `cmd` so bare invoke() calls don't
 * throw. Async tests use waitFor to let useEffect resolve.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { OutboxApprovalDialog, type StagedRecordView, type OutboxApprovalDto } from './OutboxApprovalDialog';

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

const FAKE_RECORDS: StagedRecordView[] = [
  {
    mid: 'MID001',
    to: ['WB7NOI@winlink.org'],
    cc: [],
    subject: 'Shelter status update',
    body: 'The shelter at Oak Park is open with 42 occupants.',
  },
  {
    mid: 'MID002',
    to: ['EOC@winlink.org'],
    cc: ['WB7NOI@winlink.org'],
    subject: 'Resource request',
    body: 'Requesting 20 cots and 10 sleeping bags.',
  },
];

const FAKE_APPROVAL: OutboxApprovalDto = {
  approvalId: 'approval-abc-123',
  digest: 'sha256-deadbeef',
  sessionEpoch: 42,
  expiresUnix: Date.now() / 1000 + 300,
};

// ---------------------------------------------------------------------------
// Mock @tauri-apps/api/core (invoke)
// ---------------------------------------------------------------------------

// Factory so each test can override specific commands.
let _invokeImpl: (cmd?: string, args?: unknown) => Promise<unknown> = async (cmd?: string) => {
  if (cmd === 'outbox_staged_list') return FAKE_RECORDS;
  if (cmd === 'elmer_prepare_outbox_approval') return FAKE_APPROVAL;
  if (cmd === 'elmer_connect') return undefined;
  return undefined;
};

const mockInvoke = vi.fn(async (cmd?: string, args?: unknown) => _invokeImpl(cmd, args));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd?: string, args?: unknown) => mockInvoke(cmd, args),
}));

beforeEach(() => {
  mockInvoke.mockClear();
  // Reset to the default success path.
  _invokeImpl = async (cmd?: string) => {
    if (cmd === 'outbox_staged_list') return FAKE_RECORDS;
    if (cmd === 'elmer_prepare_outbox_approval') return FAKE_APPROVAL;
    if (cmd === 'elmer_connect') return undefined;
    return undefined;
  };
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('<OutboxApprovalDialog> — manifest (AC-3, AC-12)', () => {
  it('displays all staged records after loading', async () => {
    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    // Wait for the manifest to load.
    await waitFor(() => {
      expect(screen.getByTestId('obd-manifest')).toBeTruthy();
    });

    const recordEls = screen.getAllByTestId('obd-record');
    expect(recordEls).toHaveLength(FAKE_RECORDS.length);
  });

  it('renders the "ALL N messages" header', async () => {
    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    await waitFor(() => {
      const header = screen.getByTestId('obd-manifest-header');
      expect(header.textContent).toContain(`ALL ${FAKE_RECORDS.length} messages`);
    });
  });

  it('renders verbatim to/subject/body for each record', async () => {
    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    await waitFor(() => expect(screen.getByTestId('obd-manifest')).toBeTruthy());

    const toEls = screen.getAllByTestId('obd-record-to');
    expect(toEls[0].textContent).toContain('WB7NOI@winlink.org');

    const subjectEls = screen.getAllByTestId('obd-record-subject');
    expect(subjectEls[0].textContent).toContain('Shelter status update');

    const bodyEls = screen.getAllByTestId('obd-record-body');
    expect(bodyEls[0].textContent).toContain('42 occupants');
  });
});

describe('<OutboxApprovalDialog> — Remove action', () => {
  it('clicking Remove calls onRemove with the record MID', async () => {
    const onRemove = vi.fn();
    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
        onRemove={onRemove}
      />,
    );

    await waitFor(() => expect(screen.getByTestId('obd-manifest')).toBeTruthy());

    // Click Remove on the first record.
    const removeBtn = screen.getByTestId(`obd-remove-MID001`);
    fireEvent.click(removeBtn);
    expect(onRemove).toHaveBeenCalledWith('MID001');
  });
});

describe('<OutboxApprovalDialog> — digest mismatch (AC-3)', () => {
  it('a digest-mismatch error from elmer_connect shows the re-review state', async () => {
    // Make elmer_connect throw the mismatch error string the backend produces.
    _invokeImpl = async (cmd?: string) => {
      if (cmd === 'outbox_staged_list') return FAKE_RECORDS;
      if (cmd === 'elmer_prepare_outbox_approval') return FAKE_APPROVAL;
      if (cmd === 'elmer_connect') throw new Error('outbox changed since approval — flush denied');
      return undefined;
    };

    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    // Wait for review state.
    await waitFor(() => expect(screen.getByTestId('obd-connect')).toBeTruthy());

    // Trigger the connect (which will fail with digest mismatch).
    fireEvent.click(screen.getByTestId('obd-connect'));

    // The mismatch state must be surfaced.
    await waitFor(() => {
      expect(screen.getByTestId('obd-mismatch')).toBeTruthy();
    });

    // The mismatch header changes.
    expect(screen.getByTestId('obd-mismatch-header').textContent).toContain(
      'Outbox changed since you reviewed',
    );
  });

  it('clicking Re-review after a mismatch re-fetches the manifest', async () => {
    let connectCalls = 0;
    _invokeImpl = async (cmd?: string) => {
      if (cmd === 'outbox_staged_list') return FAKE_RECORDS;
      if (cmd === 'elmer_prepare_outbox_approval') return FAKE_APPROVAL;
      if (cmd === 'elmer_connect') {
        connectCalls++;
        if (connectCalls === 1) throw new Error('outbox changed since approval — flush denied');
        return undefined;
      }
      return undefined;
    };

    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    await waitFor(() => expect(screen.getByTestId('obd-connect')).toBeTruthy());
    fireEvent.click(screen.getByTestId('obd-connect'));

    await waitFor(() => expect(screen.getByTestId('obd-re-review')).toBeTruthy());

    // Clicking Re-review should re-fetch and return to the review state.
    fireEvent.click(screen.getByTestId('obd-re-review'));

    await waitFor(() => {
      // The manifest header should be back (loading → review).
      expect(screen.getByTestId('obd-manifest-header')).toBeTruthy();
    });
  });
});

describe('<OutboxApprovalDialog> — loading state', () => {
  it('shows a loading indicator before the manifest arrives', async () => {
    // Never resolve so the loading state is stable.
    _invokeImpl = async (cmd?: string) => {
      if (cmd === 'outbox_staged_list') return new Promise(() => { /* hang */ });
      return undefined;
    };

    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={vi.fn()}
      />,
    );

    expect(screen.getByTestId('obd-loading')).toBeTruthy();
  });
});

describe('<OutboxApprovalDialog> — arm to send', () => {
  it('clicking "Arm to send" calls elmer_connect with the approval token', async () => {
    const onConnected = vi.fn();
    render(
      <OutboxApprovalDialog
        onClose={vi.fn()}
        onConnected={onConnected}
      />,
    );

    await waitFor(() => expect(screen.getByTestId('obd-connect')).toBeTruthy());
    fireEvent.click(screen.getByTestId('obd-connect'));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        'elmer_connect',
        expect.objectContaining({ approval: expect.objectContaining({ approvalId: 'approval-abc-123' }) }),
      );
    });

    await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  });
});
