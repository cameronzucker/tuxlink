// Tests for the AttachmentStrip subcomponent (tuxlink-0fyj).
//
// Covers the click-to-save and click-to-preview flows:
//   - clicking Save opens the native dialog, then invokes message_attachment_save
//   - a cancelled dialog leaves no IPC call and no status change
//   - a successful save shows "✓ Saved"
//   - an IPC error shows "✗ Failed" (with the detail on the title attribute)
//   - image attachments can be previewed on demand without saving to disk
//   - the Save button is suppressed when the parent passes no `folder`
//
// Mocks the dialog plugin + invoke to avoid touching Tauri. The component
// only needs the call-shape contract, which these mocks pin.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AttachmentStrip } from './MessageView';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(saveDialog).mockReset();
});

const SAMPLE = [
  { filename: 'forecast.grb', size: 47_123 },
  { filename: 'README.txt', size: 256 },
];

const IMAGE_SAMPLE = [
  { filename: 'map.jpg', size: 1_024 },
  { filename: 'README.txt', size: 256 },
];

describe('AttachmentStrip', () => {
  it('shows name + size + Save button per attachment when folder is provided', () => {
    render(
      <AttachmentStrip
        attachments={SAMPLE}
        messageId="MID-1"
        folder="inbox"
      />
    );
    expect(screen.getByText('forecast.grb')).toBeInTheDocument();
    expect(screen.getByText('README.txt')).toBeInTheDocument();
    expect(screen.getByTestId('attachment-save-0')).toBeInTheDocument();
    expect(screen.getByTestId('attachment-save-1')).toBeInTheDocument();
  });

  it('shows Preview only for common image attachments when folder is provided', () => {
    render(
      <AttachmentStrip
        attachments={IMAGE_SAMPLE}
        messageId="MID-1"
        folder="inbox"
      />
    );
    expect(screen.getByTestId('attachment-preview-0')).toBeInTheDocument();
    expect(screen.queryByTestId('attachment-preview-1')).toBeNull();
  });

  it('suppresses the Save button when folder is undefined (no selection context)', () => {
    render(
      <AttachmentStrip
        attachments={IMAGE_SAMPLE}
        messageId="MID-1"
        folder={undefined}
      />
    );
    expect(screen.queryByTestId('attachment-save-0')).toBeNull();
    expect(screen.queryByTestId('attachment-preview-0')).toBeNull();
    // Names and sizes still render.
    expect(screen.getByText('map.jpg')).toBeInTheDocument();
  });

  it('routes through saveDialog → invoke and shows "✓ Saved" on success', async () => {
    vi.mocked(saveDialog).mockResolvedValue('/tmp/forecast.grb');
    vi.mocked(invoke).mockResolvedValue(undefined);

    render(
      <AttachmentStrip
        attachments={SAMPLE}
        messageId="MID-7"
        folder="inbox"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-save-0'));

    await waitFor(() => {
      expect(saveDialog).toHaveBeenCalledWith(
        expect.objectContaining({ defaultPath: 'forecast.grb' })
      );
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('message_attachment_save', {
        folder: 'inbox',
        id: 'MID-7',
        filename: 'forecast.grb',
        destPath: '/tmp/forecast.grb',
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId('attachment-status-0')).toHaveTextContent(/saved/i);
    });
  });

  it('no-ops when the user cancels the Save As dialog', async () => {
    vi.mocked(saveDialog).mockResolvedValue(null);

    render(
      <AttachmentStrip
        attachments={SAMPLE}
        messageId="MID-1"
        folder="inbox"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-save-0'));

    await waitFor(() => expect(saveDialog).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalled();
    // No status badge — the row returns to idle.
    expect(screen.queryByTestId('attachment-status-0')).toBeNull();
  });

  it('shows "✗ Failed" when the backend invoke rejects', async () => {
    vi.mocked(saveDialog).mockResolvedValue('/tmp/forecast.grb');
    vi.mocked(invoke).mockRejectedValue(new Error('write /tmp/forecast.grb: permission denied'));

    render(
      <AttachmentStrip
        attachments={SAMPLE}
        messageId="MID-1"
        folder="inbox"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-save-0'));

    await waitFor(() => {
      expect(screen.getByTestId('attachment-status-0')).toHaveTextContent(/failed/i);
    });
    const status = screen.getByTestId('attachment-status-0');
    expect(status.getAttribute('title')).toMatch(/permission denied/);
  });

  it('routes the second attachment with its own filename + index', async () => {
    vi.mocked(saveDialog).mockResolvedValue('/tmp/README.txt');
    vi.mocked(invoke).mockResolvedValue(undefined);

    render(
      <AttachmentStrip
        attachments={SAMPLE}
        messageId="MID-2"
        folder="sent"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-save-1'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('message_attachment_save', {
        folder: 'sent',
        id: 'MID-2',
        filename: 'README.txt',
        destPath: '/tmp/README.txt',
      });
    });
  });

  it('previews image attachments through the backend and renders the returned data URL', async () => {
    vi.mocked(invoke).mockResolvedValue({
      filename: 'map.jpg',
      mimeType: 'image/jpeg',
      dataBase64: '/9j/AA==',
    });

    render(
      <AttachmentStrip
        attachments={IMAGE_SAMPLE}
        messageId="MID-3"
        folder="inbox"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-preview-0'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('message_attachment_preview', {
        folder: 'inbox',
        id: 'MID-3',
        filename: 'map.jpg',
      });
    });
    const image = await screen.findByTestId('attachment-preview-image-0');
    expect(image).toHaveAttribute('src', 'data:image/jpeg;base64,/9j/AA==');
    expect(image).toHaveAttribute('alt', 'map.jpg');
  });

  it('hides an already-rendered preview when Preview is clicked again', async () => {
    vi.mocked(invoke).mockResolvedValue({
      filename: 'map.jpg',
      mimeType: 'image/jpeg',
      dataBase64: '/9j/AA==',
    });

    render(
      <AttachmentStrip
        attachments={IMAGE_SAMPLE}
        messageId="MID-4"
        folder="inbox"
      />
    );
    const previewButton = screen.getByTestId('attachment-preview-0');
    fireEvent.click(previewButton);
    expect(await screen.findByTestId('attachment-preview-image-0')).toBeInTheDocument();

    fireEvent.click(previewButton);
    await waitFor(() => {
      expect(screen.queryByTestId('attachment-preview-image-0')).toBeNull();
    });
  });

  it('shows a preview failure without hiding Save As', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('unsupported image type'));

    render(
      <AttachmentStrip
        attachments={IMAGE_SAMPLE}
        messageId="MID-5"
        folder="inbox"
      />
    );
    fireEvent.click(screen.getByTestId('attachment-preview-0'));

    await waitFor(() => {
      expect(screen.getByTestId('attachment-preview-status-0')).toHaveTextContent(/preview failed/i);
    });
    expect(screen.getByTestId('attachment-preview-status-0').getAttribute('title')).toMatch(
      /unsupported image type/,
    );
    expect(screen.getByTestId('attachment-save-0')).toBeInTheDocument();
  });
});
