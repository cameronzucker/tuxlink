// WebviewFormHost — opens a child Tauri WebviewWindow that renders a
// WLE Standard Form via the per-session loopback http_server, then
// bridges the form's parsed submission back to Compose via the
// `form-submitted` Tauri event.
//
// Mount mechanism (P1 sub-decision, documented per the plan's TBD note):
//
//   Tauri 2 supports two child-webview shapes: a separate top-level
//   `WebviewWindow` (one webview per window), and `Webview` (multiple
//   webviews positioned by x/y/width/height inside one parent `Window`).
//   The latter is the literal interpretation of spec §8.2's "inline in
//   the compose body" — but it requires hand-wiring pixel geometry
//   against the compose layout, with resize/scroll handlers, on every
//   theme change. The codebase's existing pattern (help_window.rs,
//   compose_window.rs) uses `WebviewWindow` (separate top-level child
//   windows) for everything; we follow that here for P1. The form opens
//   as a sibling window of Compose with the `compose-form-<token>`
//   label, and the chrome rendered INLINE in the compose body shows
//   status + the Cancel + diagnostic Submit (fallback) buttons. Real
//   in-window embedding is a future polish that can move the form
//   rendering surface without re-architecting the host component or
//   the backend's event-routing contract.
//
// Event routing:
//
//   The backend forwarder task in `ui_commands.rs::open_webview_form`
//   calls `app.emit_to("compose-form-<token>", "form-submitted",
//   parsed)`. Tauri 2's event system delivers `emit_to(label, ...)`
//   events only to listeners targeted to that label. We therefore pass
//   `{ target: "compose-form-<token>" }` to `listen()` so the parent
//   Compose window receives the event. A global `listen` with no target
//   filter (target defaults to `{ kind: 'Any' }`) does NOT receive
//   target-scoped events in Tauri 2.x — verified against the
//   `@tauri-apps/api/event` type surface (`Options.target` + the
//   `EventTarget` union).
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 9.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.2 + §5.4.

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import './WebviewFormHost.css';

/** Mirror of the Rust `forms::multipart::ParsedBody` payload. The keys are
 * HTML form field names; values are string arrays so checkbox/multi-select
 * groups round-trip without losing entries. `submitter` records which
 * submit button fired (used by WLE forms with multiple Send actions). */
export interface ParsedBody {
  fields: Record<string, string[]>;
  submitter: string | null;
}

interface OpenFormResult {
  url: string;
  port: number;
  token: string;
}

export interface WebviewFormHostProps {
  /** WLE Standard Form id (e.g. `ICS213_Initial`). Must match an entry in
   * `forms_list_catalog`'s output. */
  formId: string;
  /** Fired when the user submits the form via its native Submit button
   * (the loopback POST path) — the canonical completion path. */
  onSubmit: (payload: ParsedBody) => void;
  /** Fired when the user clicks Cancel on the host chrome. Compose
   * decides whether to switch back to plain-text mode, re-open the
   * picker, or close the draft. */
  onCancel: () => void;
}

export function WebviewFormHost({ formId, onSubmit, onCancel }: WebviewFormHostProps) {
  const [error, setError] = useState<string | null>(null);
  // Surface the open status to the operator while the loopback bind +
  // bundle template read complete. Becomes 'open' on success or 'error'
  // on failure; 'opening' is the initial state.
  const [status, setStatus] = useState<'opening' | 'open' | 'error'>('opening');

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;
    let webview: WebviewWindow | null = null;
    let activeToken: string | null = null;

    (async () => {
      try {
        const res = await invoke<OpenFormResult>('open_webview_form', { formId });
        if (cancelled) {
          // The component unmounted before we could even register the
          // listener. Tear the session down so we don't leak a bound
          // loopback socket + forwarder task.
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {
            /* idempotent; already-closed sessions are fine */
          });
          return;
        }

        activeToken = res.token;
        const label = `compose-form-${res.token}`;

        // Register the submit listener BEFORE creating the webview so we
        // can't lose a same-frame submission. The capability config
        // (forms-webview.json) gives the child webview zero IPC powers,
        // so the only path back to tuxlink is the loopback POST that
        // populates this event channel.
        const ul = await listen<ParsedBody>(
          'form-submitted',
          (e) => onSubmit(e.payload),
          { target: label },
        );
        if (cancelled) {
          ul();
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {});
          return;
        }
        unlisten = ul;

        // WebviewWindow's constructor is synchronous on the JS side; the
        // actual window creation is async on the Rust side and signals
        // completion via the `tauri://created` event. We don't await
        // that here — the form will become visible whenever it's ready,
        // and if it fails the user sees an OS-level window error.
        webview = new WebviewWindow(label, {
          url: res.url,
          title: `Form: ${formId}`,
          width: 900,
          height: 700,
          minWidth: 480,
          minHeight: 360,
          resizable: true,
        });

        setStatus('open');
      } catch (e) {
        if (!cancelled) {
          setError(String(e));
          setStatus('error');
        }
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
      if (activeToken) {
        invoke('close_webview_form_server', { token: activeToken }).catch(() => {
          /* idempotent — backend treats unknown tokens as Ok(()) */
        });
      }
      webview?.close().catch(() => {
        /* the webview may already be closed by the user or by the OS */
      });
    };
    // onSubmit is intentionally omitted from deps: the listener is
    // registered once per mount with the initial callback, and Compose
    // re-renders shouldn't tear the webview down. If Compose needs to
    // swap the handler, that's a remount (different formId or
    // unmount/remount cycle).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [formId]);

  return (
    <div className="webview-form-host" data-testid="webview-form-host">
      {status === 'opening' && !error && (
        <div className="webview-form-host__status">
          Opening form{formId ? <> <code>{formId}</code></> : ''}…
        </div>
      )}
      {error && (
        <div className="webview-form-host__error" role="alert">
          Form failed to open: {error}
        </div>
      )}
      {status === 'open' && !error && (
        <div className="webview-form-host__status">
          The form opened in a separate window. Submit it there; this pane
          will receive the result automatically.
        </div>
      )}
      <div className="webview-form-host__chrome">
        <button
          type="button"
          className="webview-form-host__btn"
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          type="button"
          className="webview-form-host__btn webview-form-host__btn--fallback"
          disabled
          title="Diagnostic only — use the form's own Submit button in the child window"
        >
          Submit (fallback)
        </button>
      </div>
    </div>
  );
}
