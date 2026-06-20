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

  // Main-thread stall detector (tuxlink-xsv5). The "drunk map" is slow tile LOADS
  // (fetch+parse), and even a 1-point in-memory geojson tile is slow — so the
  // bottleneck is the client tile-processing pipeline, not serving or render.
  // This splits the two remaining sub-mechanisms: a heartbeat that SHOULD tick
  // every HEARTBEAT_MS; when the real gap blows past it, the JS main thread was
  // blocked that long by a synchronous op → main-thread block (find the op). If
  // the map is slow but NO large stalls log, the main thread is responsive and
  // the bottleneck is the web-worker parse queue → reduce per-tile parse cost.
  // WebKit-safe (no PerformanceObserver('longtask'), which WebKitGTK lacks).
  const HEARTBEAT_MS = 250;
  const STALL_THRESHOLD_MS = 750;
  let lastBeat = performance.now();
  setInterval(() => {
    const now = performance.now();
    const blockedMs = now - lastBeat - HEARTBEAT_MS;
    lastBeat = now;
    if (blockedMs >= STALL_THRESHOLD_MS) {
      reportFrontendError('main-thread-stall', `main thread blocked ~${Math.round(blockedMs)}ms`);
    }
  }, HEARTBEAT_MS);
}
