// WebviewFormHost — embeds a child Tauri `Webview` INSIDE the current
// Compose window that renders a WLE Standard Form via the per-session
// loopback http_server, then bridges the form's parsed submission back
// to Compose via the `form-submitted` Tauri event.
//
// Mount mechanism (P1 sub-decision, rework of commit 40a5a2a):
//
//   Tauri 2 exposes two child-webview shapes:
//
//     - `WebviewWindow` (from @tauri-apps/api/webviewWindow) — spawns a
//       NEW top-level OS window containing one webview. This is what
//       help_window.rs + compose_window.rs use for their respective
//       sibling windows.
//
//     - `Webview` (from @tauri-apps/api/webview) — attaches a child
//       webview to an EXISTING parent `Window`, positioned by
//       x/y/width/height (logical pixels) within the parent's content
//       area. Tauri 2's `Webview` constructor takes `(parentWindow,
//       label, options)`; navigation URL goes in `options.url`.
//
//   Spec §8.2 literally says "React WebviewFormHost mounts
//   <webview src=url>" — i.e. an in-window embed inside the existing
//   Compose window, not a sibling top-level window. The operator's
//   sticky pet peeve (memory `feedback_inline_ui_no_window_clutter`:
//   "tuxlink UI must be inline ... Compose is the lone settled
//   exception") reinforces this — adding a second pop-up for the form
//   would be exactly the anti-pattern.
//
//   We therefore use `Webview`, pixel-position it over a placeholder
//   div rendered in the compose body, and re-measure + reposition on
//   parent-window resize via `ResizeObserver`. The placeholder div
//   reserves layout space; the child webview paints over it (child
//   webviews are stacked above the parent's WebContents in z-order,
//   so the placeholder is invisible at runtime — fine, it's a
//   layout-reservation device).
//
// Event routing:
//
//   The backend forwarder task in `ui_commands.rs::open_webview_form`
//   calls `app.emit_to("compose-form-<token>", "form-submitted",
//   parsed)`. Tauri 2's event system delivers `emit_to(label, ...)`
//   events only to listeners targeted to that label. We therefore pass
//   `{ target: "compose-form-<token>" }` to `listen()` so the Compose
//   window receives the event. A global `listen` with no target filter
//   (target defaults to `{ kind: 'Any' }`) does NOT receive
//   target-scoped events in Tauri 2.x — verified against the
//   `@tauri-apps/api/event` type surface (`Options.target` + the
//   `EventTarget` union).
//
//   The capability scope (`src-tauri/capabilities/forms-webview.json`)
//   matches webview labels `compose-form-*` — the same label space
//   used here, so the Tauri runtime applies the zero-IPC restriction
//   to the embedded child webview regardless of whether it lives in a
//   new top-level window or as a Webview inside Compose.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 9.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.2 + §5.4.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Webview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { exportFormPdf } from './pdfExport';
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
  // The child-webview label, lifted out of the effect so the Export-PDF
  // affordance can target it. Set once the webview opens; null while opening.
  const [exportLabel, setExportLabel] = useState<string | null>(null);
  // Transient export feedback shown in the chrome ("Exporting…", "Saved to …",
  // or an error). Cleared when a new export starts.
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  // Placeholder div in the compose body. The child Webview is pixel-
  // positioned to overlay this rect. The ref is read inside the useEffect
  // below (which runs AFTER the first paint, so getBoundingClientRect()
  // returns real coordinates) and by the ResizeObserver callback.
  const mountRef = useRef<HTMLDivElement | null>(null);
  // Hold the latest `onSubmit` in a ref so the `form-submitted` listener
  // (registered ONCE per mount, in the [formId]-deps effect below) always
  // sees the freshest closure. Without this, the listener would close over
  // the initial `onSubmit` and miss recipient/subject edits the operator
  // made AFTER opening the form (Critical #2 from the P1 Task 10 code
  // review: Compose's handleWebviewSubmit captures `to`/`cc` via
  // useCallback deps; if we re-register the listener on every onSubmit
  // change we tear down the webview each render, but if we don't update
  // *something*, stale-recipient submits go through to the wrong people).
  const onSubmitRef = useRef(onSubmit);
  useEffect(() => {
    onSubmitRef.current = onSubmit;
  });

  // Cancel handle for any in-flight requestAnimationFrame reposition.
  // Populated inside the main effect; the cleanup invokes it so a pending
  // RAF callback doesn't fire after the component unmounts.
  const rafCancelRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;
    let webview: Webview | null = null;
    let resizeObserver: ResizeObserver | null = null;
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
        //
        // The listener invokes the LATEST onSubmit via the ref — the
        // listener is registered once per mount; the ref propagates prop
        // changes (recipient edits, subject changes) without re-listening.
        const ul = await listen<ParsedBody>(
          'form-submitted',
          (e) => onSubmitRef.current(e.payload),
          { target: label },
        );
        if (cancelled) {
          ul();
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {});
          return;
        }
        unlisten = ul;

        // Read the placeholder rect AFTER first paint. This useEffect
        // fires post-commit, so getBoundingClientRect() returns real
        // layout coordinates. If the element isn't mounted (component
        // unmounted between paints), bail.
        const mountEl = mountRef.current;
        if (!mountEl) {
          ul();
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {});
          return;
        }
        const initialRect = mountEl.getBoundingClientRect();
        // Defensively floor + max(1, _) to dodge sub-pixel and 0-rect
        // edge cases that some Tauri versions reject.
        const initialX = Math.max(0, Math.floor(initialRect.left));
        const initialY = Math.max(0, Math.floor(initialRect.top));
        const initialW = Math.max(1, Math.floor(initialRect.width));
        const initialH = Math.max(1, Math.floor(initialRect.height));

        // Construct the in-window Webview. The parent Window is the
        // current Compose window; the label is the same as before so
        // the capability scope + backend emit_to(label, ...) still
        // resolve to this webview.
        const parent = getCurrentWindow();
        webview = new Webview(parent, label, {
          url: res.url,
          x: initialX,
          y: initialY,
          width: initialW,
          height: initialH,
        });

        // tuxlink-rqrn I1: subscribe to Tauri's lifecycle events on the
        // child webview. The JS-side `new Webview(...)` is synchronous,
        // but actual webview creation on the Rust side is async — if it
        // fails (capability mismatch, OOM, host crash), we'd otherwise
        // sit in `status='open'` with an empty placeholder. `tauri://error`
        // surfaces failure; `tauri://created` confirms success.
        webview.once('tauri://error', (event) => {
          if (!cancelled) {
            setError(String(event.payload ?? 'webview creation failed'));
            setStatus('error');
          }
        });

        // Re-measure + reposition on placeholder resize. ResizeObserver
        // fires on element dimension changes. The document.body observer
        // catches WINDOW resizes (body bbox changes); layout reflows that
        // shift the placeholder WITHOUT resizing the body itself are an
        // accepted limitation (rare in practice given the compose body's
        // fixed-flex layout — placeholder is `flex: 1 1 auto` and the
        // compose-body height-source IS the window). tuxlink-rqrn I3:
        // setPosition + setSize IPC calls are coalesced via
        // requestAnimationFrame so window-drag-resize doesn't fire many
        // IPC round-trips per second.
        let rafHandle: number | null = null;
        const scheduleReposition = () => {
          if (cancelled || !webview || !mountRef.current) return;
          if (rafHandle !== null) return;  // already scheduled
          rafHandle = requestAnimationFrame(() => {
            rafHandle = null;
            if (cancelled || !webview || !mountRef.current) return;
            const rect = mountRef.current.getBoundingClientRect();
            const x = Math.max(0, Math.floor(rect.left));
            const y = Math.max(0, Math.floor(rect.top));
            const w = Math.max(1, Math.floor(rect.width));
            const h = Math.max(1, Math.floor(rect.height));
            // Fire-and-forget: setPosition/setSize return promises but
            // the failure mode is "webview is gone," which the cleanup
            // path handles on next teardown.
            void webview.setPosition(new LogicalPosition(x, y)).catch(() => {});
            void webview.setSize(new LogicalSize(w, h)).catch(() => {});
          });
        };
        resizeObserver = new ResizeObserver(scheduleReposition);
        resizeObserver.observe(mountEl);
        if (typeof document !== 'undefined' && document.body) {
          resizeObserver.observe(document.body);
        }
        rafCancelRef.current = () => {
          if (rafHandle !== null) {
            cancelAnimationFrame(rafHandle);
            rafHandle = null;
          }
        };

        setExportLabel(label);
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
      setExportLabel(null);
      unlisten?.();
      resizeObserver?.disconnect();
      rafCancelRef.current?.();
      rafCancelRef.current = null;
      if (activeToken) {
        invoke('close_webview_form_server', { token: activeToken }).catch(() => {
          /* idempotent — backend treats unknown tokens as Ok(()) */
        });
      }
      webview?.close().catch(() => {
        /* the webview may already be closed by the OS, or never created */
      });
    };
    // [formId]-only deps are intentional: the listener is registered once
    // per mount; subsequent `onSubmit` prop changes propagate via
    // onSubmitRef (updated in the small effect above on every render).
    // This avoids the stale-closure bug from the eslint-disabled version
    // (P1 Task 10 critical-fix) where recipient edits made AFTER opening
    // the form weren't reflected at submit time.
  }, [formId]);

  const handleExportPdf = async () => {
    if (!exportLabel || exporting) return;
    setExporting(true);
    setExportMsg('Exporting…');
    try {
      const path = await exportFormPdf(exportLabel, formId);
      // `null` = the operator dismissed the Save dialog; leave no message.
      setExportMsg(path ? `Saved PDF to ${path}` : null);
    } catch (e) {
      setExportMsg(`Export failed: ${String(e)}`);
    } finally {
      setExporting(false);
    }
  };

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
      {/* The placeholder div the child Webview is pixel-positioned over.
        * Layout-only; the webview paints above it at runtime. Sized to
        * fill the available compose-body region so the form has room. */}
      <div
        ref={mountRef}
        className="webview-form-host__embed"
        data-testid="webview-form-host-embed"
        aria-hidden="true"
      />
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
          className="webview-form-host__btn"
          data-testid="webview-form-host-export-pdf"
          onClick={handleExportPdf}
          disabled={status !== 'open' || exporting}
          title="Save this form as a PDF — a faithful copy for a served agency or non-ham recipient"
        >
          {exporting ? 'Exporting…' : 'Export PDF'}
        </button>
        {exportMsg && (
          <span className="webview-form-host__export-msg" role="status">
            {exportMsg}
          </span>
        )}
        <button
          type="button"
          className="webview-form-host__btn webview-form-host__btn--fallback"
          disabled
          title="Diagnostic only — use the form's own Submit button (top of the embedded form)"
        >
          Submit (fallback)
        </button>
      </div>
    </div>
  );
}
