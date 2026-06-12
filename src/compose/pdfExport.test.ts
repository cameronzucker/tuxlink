import { describe, it, expect, vi, beforeEach } from 'vitest';
import { defaultPdfName, exportFormPdf, printForm } from './pdfExport';

const saveMock = vi.fn();
const invokeMock = vi.fn();

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: (...args: unknown[]) => saveMock(...args),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeEach(() => {
  saveMock.mockReset();
  invokeMock.mockReset();
});

describe('defaultPdfName', () => {
  it('replaces path separators and whitespace', () => {
    expect(defaultPdfName('AAMRON/Net Check-in')).toBe('AAMRON-Net_Check-in');
  });

  it('strips characters that are poor in a filename', () => {
    expect(defaultPdfName('ICS213: General (Message)')).toBe('ICS213_General_Message');
  });

  it('trims leading/trailing punctuation', () => {
    expect(defaultPdfName('  _ICS213_  ')).toBe('ICS213');
  });

  it('falls back to "form" when nothing usable remains', () => {
    expect(defaultPdfName('///   ')).toBe('form');
  });

  it('keeps an already-clean id intact', () => {
    expect(defaultPdfName('ICS213_Initial')).toBe('ICS213_Initial');
  });
});

describe('exportFormPdf', () => {
  it('passes the chosen path to the backend and returns the written path', async () => {
    saveMock.mockResolvedValue('/home/op/Desktop/report.pdf');
    invokeMock.mockResolvedValue('/home/op/Desktop/report.pdf');

    const result = await exportFormPdf('viewer-form-abc123', 'ICS213_Initial');

    expect(saveMock).toHaveBeenCalledWith({
      defaultPath: 'ICS213_Initial.pdf',
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    });
    expect(invokeMock).toHaveBeenCalledWith('forms_export_pdf', {
      webviewLabel: 'viewer-form-abc123',
      outPath: '/home/op/Desktop/report.pdf',
    });
    expect(result).toBe('/home/op/Desktop/report.pdf');
  });

  it('returns null and does NOT invoke the backend when the dialog is cancelled', async () => {
    saveMock.mockResolvedValue(null);

    const result = await exportFormPdf('compose-form-xyz', 'Quick Message Initial');

    expect(result).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('propagates a backend export failure to the caller', async () => {
    saveMock.mockResolvedValue('/tmp/out.pdf');
    invokeMock.mockRejectedValue(new Error('print failed: no printer'));

    await expect(exportFormPdf('viewer-form-abc', 'Form')).rejects.toThrow(
      /print failed/,
    );
  });
});

describe('printForm', () => {
  it('invokes forms_print with the webview label and returns true when printed', async () => {
    invokeMock.mockResolvedValue(true);

    const result = await printForm('viewer-form-abc123');

    expect(invokeMock).toHaveBeenCalledWith('forms_print', {
      webviewLabel: 'viewer-form-abc123',
    });
    // No Save dialog in the print path — it goes straight to the system dialog.
    expect(saveMock).not.toHaveBeenCalled();
    expect(result).toBe(true);
  });

  it('returns false when the operator cancels the print dialog', async () => {
    invokeMock.mockResolvedValue(false);

    const result = await printForm('compose-form-xyz');

    expect(result).toBe(false);
  });

  it('propagates a backend print failure to the caller', async () => {
    invokeMock.mockRejectedValue(new Error('print failed: dialog channel closed'));

    await expect(printForm('viewer-form-abc')).rejects.toThrow(/print failed/);
  });
});
