// On-demand faithful PDF export of a rendered WLE form (tuxlink-cumx / G8).
//
// Shared by WebviewFormHost (authoring side) and WebviewFormViewer (received
// side): both render a form in a labeled child Tauri webview, and export prints
// that exact webview to PDF via the Rust `forms_export_pdf` command (which
// drives WebKitPrintOperation). The audience is a served agency / non-ham who
// opens the PDF to read what was sent — so the output mirrors the on-screen
// form, not a data summary.

import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';

/** Sanitize a form id into a filesystem-friendly default filename. The catalog
 *  id can contain spaces and `/` (folder-qualified custom forms); both are
 *  poor defaults for a save dialog. */
export function defaultPdfName(formId: string): string {
  const cleaned = formId
    .replace(/[\\/]+/g, '-') // path separators → dash
    .replace(/\s+/g, '_') // whitespace → underscore
    .replace(/[^A-Za-z0-9._-]/g, '') // drop anything else
    .replace(/^[-_.]+|[-_.]+$/g, ''); // trim leading/trailing punctuation
  return cleaned.length > 0 ? cleaned : 'form';
}

/**
 * Prompt for a destination and export the form rendered in `webviewLabel` to
 * PDF. Returns the written path on success, or `null` if the operator
 * cancelled the save dialog. Throws if the backend print fails (caller surfaces
 * the message).
 */
export async function exportFormPdf(
  webviewLabel: string,
  formId: string,
): Promise<string | null> {
  const path = await saveDialog({
    defaultPath: `${defaultPdfName(formId)}.pdf`,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  });
  // `save` returns null when the operator dismisses the dialog.
  if (path === null || path === undefined) return null;
  return invoke<string>('forms_export_pdf', { webviewLabel, outPath: path });
}
