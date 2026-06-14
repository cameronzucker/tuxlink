// Frontend error → robust log forwarding (tuxlink-4b96).
//
// The structured logs (`~/.local/state/tuxlink/logs/tuxlink.<hour>.jsonl` + the
// Logging window) are Rust/`tracing`-based and previously captured backend events
// only. A React ErrorBoundary capture / window.onerror / unhandledrejection went
// ONLY to the WebKitGTK devtools console + `tauri dev` stdout — so a webview crash
// could not be diagnosed from the logs. This forwards them to the backend
// `log_frontend_error` command, which re-emits them as `tracing::error!` so they
// land in the same logs + Logging window as everything else.

import { invoke } from '@tauri-apps/api/core';

/**
 * Forward a single frontend error into the structured log. Fire-and-forget and
 * never throws — the caller is already on an error path, and in a non-Tauri
 * context (browser/test) the invoke simply rejects and is swallowed.
 */
export function reportFrontendError(source: string, message: string, stack?: string): void {
  try {
    void invoke('log_frontend_error', { source, message, stack: stack ?? null }).catch(() => {});
  } catch {
    // invoke itself can throw synchronously outside a Tauri webview — ignore.
  }
}

let installed = false;

/**
 * Install global handlers that forward otherwise-uncaught errors (outside any
 * React boundary) and unhandled promise rejections into the structured log.
 * Idempotent; call once at app bootstrap.
 */
export function installGlobalErrorForwarding(): void {
  if (installed || typeof window === 'undefined') return;
  installed = true;

  window.addEventListener('error', (event: ErrorEvent) => {
    const err = event.error;
    const message = err instanceof Error ? err.message : event.message || String(err);
    const stack = err instanceof Error ? err.stack : undefined;
    reportFrontendError('window.error', message, stack);
  });

  window.addEventListener('unhandledrejection', (event: PromiseRejectionEvent) => {
    const reason = event.reason;
    const message = reason instanceof Error ? reason.message : String(reason);
    const stack = reason instanceof Error ? reason.stack : undefined;
    reportFrontendError('unhandledrejection', message, stack);
  });
}
