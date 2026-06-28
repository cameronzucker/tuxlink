//! On-demand faithful PDF export of a rendered WLE form (tuxlink-cumx / G8).
//!
//! A form is displayed in a child WebKitWebView (label `compose-form-<token>`
//! for authoring, `viewer-form-<token>` for a received form — see
//! `forms::http_server` + the React `WebviewFormHost` / `WebviewFormViewer`).
//! Export reuses that *live* WebKitGTK view — the same engine that painted the
//! form — via `WebKitPrintOperation`, so the PDF matches exactly what the
//! operator sees on screen.
//!
//! Why this design: tuxlink's UI process already *is* WebKitGTK, so the
//! rendering engine for the PDF is already linked. WLE bolts a second engine
//! (wkhtmltopdf / NReco, a licensed native dep) onto the app purely for PDF;
//! the forms synthesis (2026-06-11) calls for dropping that. Reusing the live
//! webview means zero new dependencies and guarantees fidelity.
//!
//! Testability: the path-shaping logic (`ensure_pdf_extension`) is pure and
//! unit-tested. The actual GTK print is FFI against the live webview + GTK main
//! loop — it cannot run without a display, so it is validated by an operator
//! smoke (open a form → Export PDF → open the file), not a unit test.

use std::path::{Path, PathBuf};

/// Failure modes for a PDF export request.
#[derive(Debug, thiserror::Error)]
pub enum PdfExportError {
    /// No webview is registered under the requested label — the form session
    /// was torn down, or the label is stale.
    #[error("form webview not found: {0}")]
    WebviewNotFound(String),
    /// `with_webview` / the GTK print operation reported a failure.
    #[error("print failed: {0}")]
    PrintFailed(String),
    /// The print operation never signalled completion within the deadline.
    #[error("print timed out after {0}s")]
    TimedOut(u64),
    /// PDF export is only wired for Linux/WebKitGTK (tuxlink's only target).
    #[error("PDF export is only supported on Linux/WebKitGTK in this build")]
    UnsupportedPlatform,
}

/// Seconds to wait for the asynchronous GTK print to finish writing the file.
/// A single rendered form prints in well under a second; 30s is generous
/// headroom that still bounds a wedged print so the command can't hang forever.
/// Only the Linux (WebKitGTK) print path reads it; gated to Linux so non-Linux
/// targets — where `export_webview_pdf`/`print_webview` are the unsupported
/// stubs — don't trip the `dead_code` lint under `-D warnings`.
#[cfg(target_os = "linux")]
const PRINT_DEADLINE_SECS: u64 = 30;

/// Ensure the chosen output path ends in `.pdf` (case-insensitive). A native
/// save dialog can return a path the operator typed without an extension; the
/// served-agency recipient expects a `.pdf`, so we append it when absent.
/// An existing `.PDF` / `.pdf` is preserved as-is (no double extension).
pub fn ensure_pdf_extension(path: &Path) -> PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("pdf") => path.to_path_buf(),
        _ => {
            let mut os = path.as_os_str().to_owned();
            os.push(".pdf");
            PathBuf::from(os)
        }
    }
}

/// Print the child webview identified by `label` to a PDF at `out_path`.
///
/// Linux/WebKitGTK implementation: resolve the child `Webview` by label, reach
/// its underlying `webkit2gtk::WebView`, attach a `WebKitPrintOperation` whose
/// `GtkPrintSettings` target a `file://` URI with `output-file-format=pdf`, and
/// run it. The GTK main loop drives the async print; we block the calling
/// (worker) thread on a channel until the `finished`/`failed` signal fires.
#[cfg(target_os = "linux")]
pub fn export_webview_pdf<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    label: &str,
    out_path: &Path,
) -> Result<PathBuf, PdfExportError> {
    use std::sync::mpsc;
    use tauri::Manager;

    let out = ensure_pdf_extension(out_path);
    // GTK consumes the destination as a URI; `filename_to_uri` percent-encodes
    // spaces and other path characters correctly (served-agency Desktop paths
    // routinely contain spaces).
    // `glib` reaches us re-exported through webkit2gtk (it does not re-export
    // `gtk`, which is why that one is a direct dep). `filename_to_uri`
    // percent-encodes spaces and other path characters that a raw `file://`
    // concat would leave invalid.
    let uri = webkit2gtk::glib::filename_to_uri(&out, None)
        .map_err(|e| PdfExportError::PrintFailed(format!("encode output uri: {e}")))?
        .to_string();

    let webview = app
        .get_webview(label)
        .ok_or_else(|| PdfExportError::WebviewNotFound(label.to_string()))?;

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let tx_setup = tx.clone();
    webview
        .with_webview(move |platform| {
            use std::cell::RefCell;
            use std::rc::Rc;
            use webkit2gtk::{PrintOperation, PrintOperationExt};

            let wv = platform.inner(); // webkit2gtk::WebView
            let op = PrintOperation::new(&wv);

            let settings = gtk::PrintSettings::new();
            // GTK_PRINT_SETTINGS_OUTPUT_URI + OUTPUT_FILE_FORMAT — selects the
            // "print to file" backend and forces PDF output regardless of the
            // file extension.
            settings.set("output-uri", Some(uri.as_str()));
            settings.set("output-file-format", Some("pdf"));
            op.set_print_settings(&settings);

            // `op.print()` is asynchronous: it schedules the render on the GTK
            // main loop and returns immediately. If the only Rust handle to the
            // operation dropped here (closure end), the GObject could be
            // finalized mid-print and the `finished`/`failed` signal would never
            // fire — leaving the command's `recv_timeout` to hit the deadline.
            // Hold a ref alive in `keep`; whichever terminal signal fires first
            // `.take()`s it, releasing the operation exactly once. `Rc` is fine
            // here: it is created on the GTK main thread (inside this dispatched
            // closure) and never crosses a thread boundary.
            let keep = Rc::new(RefCell::new(Some(op.clone())));

            let keep_done = keep.clone();
            let tx_done = tx_setup.clone();
            op.connect_finished(move |_| {
                keep_done.borrow_mut().take();
                let _ = tx_done.send(Ok(()));
            });
            let keep_fail = keep.clone();
            let tx_fail = tx_setup.clone();
            op.connect_failed(move |_, err| {
                keep_fail.borrow_mut().take();
                let _ = tx_fail.send(Err(err.to_string()));
            });

            op.print();
        })
        .map_err(|e| PdfExportError::PrintFailed(e.to_string()))?;

    match rx.recv_timeout(std::time::Duration::from_secs(PRINT_DEADLINE_SECS)) {
        Ok(Ok(())) => Ok(out),
        Ok(Err(e)) => Err(PdfExportError::PrintFailed(e)),
        Err(_) => Err(PdfExportError::TimedOut(PRINT_DEADLINE_SECS)),
    }
}

/// Open the system print dialog for the form rendered in `label`'s child
/// webview and print on confirm — no intermediate file (tuxlink-954o / G8b).
///
/// Reuses the same `WebKitPrintOperation` machinery as [`export_webview_pdf`],
/// but `run_dialog` shows GTK's printer picker (physical printers + a
/// "Print to File" option) and prints synchronously when the operator
/// confirms. Saves the export-to-disk-then-open-then-print detour for a
/// hardcopy. Returns `true` if the operator printed, `false` if they
/// cancelled the dialog.
#[cfg(target_os = "linux")]
pub fn print_webview<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    label: &str,
) -> Result<bool, PdfExportError> {
    use std::sync::mpsc;
    use tauri::Manager;

    let webview = app
        .get_webview(label)
        .ok_or_else(|| PdfExportError::WebviewNotFound(label.to_string()))?;

    let (tx, rx) = mpsc::channel::<bool>();
    webview
        .with_webview(move |platform| {
            use webkit2gtk::{PrintOperation, PrintOperationExt, PrintOperationResponse};

            let wv = platform.inner(); // webkit2gtk::WebView
            let op = PrintOperation::new(&wv);
            // run_dialog runs a nested GTK loop until the operator confirms or
            // cancels, printing internally on confirm. No parent window is
            // passed: gtk3's `Widget::toplevel` is deprecated and would trip
            // `-D warnings`; the dialog is unparented but fully functional.
            let printed = matches!(
                op.run_dialog(None::<&gtk::Window>),
                PrintOperationResponse::Print
            );
            let _ = tx.send(printed);
        })
        .map_err(|e| PdfExportError::PrintFailed(e.to_string()))?;

    // No deadline: the operator may deliberate in the dialog. The closure
    // always sends once the dialog closes, so this resolves then.
    rx.recv()
        .map_err(|_| PdfExportError::PrintFailed("print dialog channel closed".into()))
}

/// Non-Linux fallback so the crate compiles on any dev host. tuxlink ships
/// Linux-only; this path is never taken in a real build.
#[cfg(not(target_os = "linux"))]
pub fn export_webview_pdf<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _label: &str,
    _out_path: &Path,
) -> Result<PathBuf, PdfExportError> {
    Err(PdfExportError::UnsupportedPlatform)
}

/// Non-Linux fallback for [`print_webview`].
#[cfg(not(target_os = "linux"))]
pub fn print_webview<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _label: &str,
) -> Result<bool, PdfExportError> {
    Err(PdfExportError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_pdf_extension_appends_when_missing() {
        let p = ensure_pdf_extension(Path::new("/home/op/ICS-213"));
        assert_eq!(p, PathBuf::from("/home/op/ICS-213.pdf"));
    }

    #[test]
    fn ensure_pdf_extension_preserves_existing_lowercase() {
        let p = ensure_pdf_extension(Path::new("/home/op/report.pdf"));
        assert_eq!(p, PathBuf::from("/home/op/report.pdf"));
    }

    #[test]
    fn ensure_pdf_extension_preserves_existing_uppercase() {
        // A save dialog on some desktops yields `.PDF`; don't double it.
        let p = ensure_pdf_extension(Path::new("/home/op/report.PDF"));
        assert_eq!(p, PathBuf::from("/home/op/report.PDF"));
    }

    #[test]
    fn ensure_pdf_extension_appends_for_unrelated_extension() {
        // A name like "ICS-213 v2.1" has a ".1" extension that is NOT pdf —
        // append rather than replace, so the operator's filename survives.
        let p = ensure_pdf_extension(Path::new("/home/op/ICS-213 v2.1"));
        assert_eq!(p, PathBuf::from("/home/op/ICS-213 v2.1.pdf"));
    }

    #[test]
    fn ensure_pdf_extension_handles_path_with_spaces() {
        let p = ensure_pdf_extension(Path::new("/home/op/Desktop/Skagit Flood Sitrep"));
        assert_eq!(
            p,
            PathBuf::from("/home/op/Desktop/Skagit Flood Sitrep.pdf")
        );
    }
}
