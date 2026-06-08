// WebviewFormViewer — receive-side fallback that renders unknown / non-
// native received forms via a Viewer-mode child Tauri `Webview` embedded
// IN-WINDOW inside the current parent (the main window's MessageView pane).
// The webview loads the WLE `*_Viewer.html` template via the per-session
// loopback http_server (forms::http_server, Viewer mode — P1 Task 11) with
// the parsed FormPayload's field values pre-bound into the HTML.
//
// Mount mechanism mirrors WebviewFormHost (P1 Task 9 / commit a2b34a8): a
// child Tauri `Webview` (NOT `WebviewWindow` — operator memory
// `feedback_inline_ui_no_window_clutter` + spec §8.2) attached to the
// current parent window, pixel-positioned over the `mount` placeholder
// div, repositioned on ResizeObserver fire. The form HTML itself paints
// above the placeholder (child webviews stack above the parent
// WebContents in z-order).
//
// Differences from WebviewFormHost:
//
//   1. Calls `open_webview_viewer(form_id, field_values)`, not
//      `open_webview_form(form_id)`. The viewer command additionally
//      binds the parsed FormPayload into the HTML server-side (both
//      `{var X}` placeholders and `name=""` hidden inputs via a script
//      that runs on DOMContentLoaded).
//   2. NO Submit button — Viewer mode is read-only; the http_server's
//      POST route 404s.
//   3. NO `form-submitted` listener — there's no submit path.
//   4. The webview label is `viewer-form-<token>` (the `forms-webview.json`
//      capability matches BOTH `compose-form-*` and `viewer-form-*`).
//   5. Calls `onFallback()` on failure to open (template missing, etc.),
//      so MessageView can fall through to KeyValueView.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 11.
// Spec: docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md §8.3.

import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Webview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi';
import './WebviewFormViewer.css';

interface OpenViewerResult {
  url: string;
  port: number;
  token: string;
}

export interface WebviewFormViewerProps {
  /** WLE form id (e.g. `Quick_Message_Initial`). The Rust side resolves
   *  the Viewer template filename via FormDef::display_form when the
   *  form is in the BUNDLED_FORMS catalog; falls back to
   *  `<formId>_Viewer.html` convention otherwise. */
  formId: string;
  /** Parsed FormPayload's `(field_id, value)` pairs. Passed to the Rust
   *  side as a plain `{key: value}` HashMap; the http_server binds them
   *  into the served HTML via two complementary paths (server-side
   *  `{var X}` substitution + a DOMContentLoaded script that assigns to
   *  `[name="X"]` inputs). */
  fieldValues: Record<string, string>;
  /** Fired when the operator clicks the Close button in the viewer chrome.
   *  MessageView is responsible for whatever cleanup makes sense (e.g.
   *  switching the reading pane back to the next message in the list). */
  onClose: () => void;
  /** Fired when the Viewer-mode session fails to open. Typical cause: the
   *  resolved Viewer template doesn't exist on disk (catalog drift, or a
   *  custom form with no companion `_Viewer.html`). MessageView falls
   *  through to KeyValueView when this fires. */
  onFallback?: (error: string) => void;
  /** When true, the child webview is hidden (`.hide()`) and ResizeObserver
   *  repositioning is paused. The webview is NOT destroyed — form state +
   *  the loopback session survive. Used by AppShell to hide the viewer
   *  while the radio drawer is open (compact-mode overlay coexistence,
   *  tuxlink-813d Task 2). Defaults to false. */
  suppressed?: boolean;
}

export function WebviewFormViewer({
  formId,
  fieldValues,
  onClose,
  onFallback,
  suppressed = false,
}: WebviewFormViewerProps) {
  const [error, setError] = useState<string | null>(null);
  // Open status surface to the operator while the loopback bind + viewer
  // template read complete. Mirrors WebviewFormHost's state surface.
  const [status, setStatus] = useState<'opening' | 'open' | 'error'>('opening');
  // Placeholder div the child Webview is pixel-positioned over. The webview
  // paints above this div at runtime.
  const mountRef = useRef<HTMLDivElement | null>(null);
  // Hold the created webview so the suppression effect can call hide/show
  // without being in the creation effect's dependency list.
  const webviewRef = useRef<Webview | null>(null);
  // Keep a ref to the latest suppressed value so the ResizeObserver callback
  // (which closes over the ref, not the state) can read the current value.
  const suppressedRef = useRef(suppressed);
  useEffect(() => {
    suppressedRef.current = suppressed;
  }, [suppressed]);
  // Hold the latest onFallback in a ref so the open-failure path always
  // sees the freshest closure even if MessageView re-renders before the
  // open promise rejects.
  const onFallbackRef = useRef(onFallback);
  useEffect(() => {
    onFallbackRef.current = onFallback;
  });

  useEffect(() => {
    let cancelled = false;
    let webview: Webview | null = null;
    let resizeObserver: ResizeObserver | null = null;
    let activeToken: string | null = null;

    (async () => {
      try {
        const res = await invoke<OpenViewerResult>('open_webview_viewer', {
          formId,
          fieldValues,
        });
        if (cancelled) {
          // The component unmounted before we could attach the webview;
          // tear down the loopback session so we don't leak a port.
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {
            /* idempotent */
          });
          return;
        }

        activeToken = res.token;
        const label = `viewer-form-${res.token}`;

        const mountEl = mountRef.current;
        if (!mountEl) {
          await invoke('close_webview_form_server', { token: res.token }).catch(() => {});
          return;
        }
        const initialRect = mountEl.getBoundingClientRect();
        const initialX = Math.max(0, Math.floor(initialRect.left));
        const initialY = Math.max(0, Math.floor(initialRect.top));
        const initialW = Math.max(1, Math.floor(initialRect.width));
        const initialH = Math.max(1, Math.floor(initialRect.height));

        const parent = getCurrentWindow();
        webview = new Webview(parent, label, {
          url: res.url,
          x: initialX,
          y: initialY,
          width: initialW,
          height: initialH,
        });
        webviewRef.current = webview;

        // Reposition on layout changes (parent window resize, sibling
        // panel reflow, etc.). Same mechanism as WebviewFormHost.
        resizeObserver = new ResizeObserver(() => {
          if (cancelled || !webview || !mountRef.current || suppressedRef.current) return;
          const rect = mountRef.current.getBoundingClientRect();
          const x = Math.max(0, Math.floor(rect.left));
          const y = Math.max(0, Math.floor(rect.top));
          const w = Math.max(1, Math.floor(rect.width));
          const h = Math.max(1, Math.floor(rect.height));
          void webview.setPosition(new LogicalPosition(x, y)).catch(() => {});
          void webview.setSize(new LogicalSize(w, h)).catch(() => {});
        });
        resizeObserver.observe(mountEl);
        if (typeof document !== 'undefined' && document.body) {
          resizeObserver.observe(document.body);
        }

        setStatus('open');
      } catch (e) {
        if (!cancelled) {
          const msg = String(e);
          setError(msg);
          setStatus('error');
          // Surface the failure to MessageView so it can fall back to
          // KeyValueView. We still render an error banner ourselves so
          // a parent without an onFallback handler still sees the
          // problem; the ref-deref is defensive against late prop edits.
          onFallbackRef.current?.(msg);
        }
      }
    })();

    return () => {
      cancelled = true;
      resizeObserver?.disconnect();
      webviewRef.current = null;
      if (activeToken) {
        invoke('close_webview_form_server', { token: activeToken }).catch(() => {
          /* idempotent — backend treats unknown tokens as Ok(()) */
        });
      }
      webview?.close().catch(() => {
        /* may already be closed by the OS, or never created */
      });
    };
    // [formId, fieldValues] deps: re-opening for a different form_id or
    // when the parsed payload changes is the right behavior. In practice
    // MessageView keys the component on message.id so a different message
    // triggers a full unmount/remount; this deps array is a safety net.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [formId, JSON.stringify(fieldValues)]);

  // Hide/show the child webview when the radio drawer opens/closes (tuxlink-813d
  // Task 2). Keyed on [suppressed] only — does NOT add suppressed to the creation
  // effect's deps so the webview is never recreated on toggle.
  useEffect(() => {
    const wv = webviewRef.current;
    if (!wv) return;
    if (suppressed) {
      void wv.hide().catch(() => {});
    } else {
      void wv.show().catch(() => {});
      const el = mountRef.current;
      if (el) {
        const r = el.getBoundingClientRect();
        void wv.setPosition(new LogicalPosition(Math.max(0, Math.floor(r.left)), Math.max(0, Math.floor(r.top)))).catch(() => {});
        void wv.setSize(new LogicalSize(Math.max(1, Math.floor(r.width)), Math.max(1, Math.floor(r.height)))).catch(() => {});
      }
    }
  }, [suppressed]);

  return (
    <div className="webview-form-viewer" data-testid="webview-form-viewer">
      {status === 'opening' && !error && (
        <div className="webview-form-viewer__status">
          Opening viewer for <code>{formId}</code>…
        </div>
      )}
      {error && (
        <div className="webview-form-viewer__error" role="alert">
          Viewer failed to open: {error}
        </div>
      )}
      {/* Layout-reservation div the child Webview is pixel-positioned over.
        * Aria-hidden because the webview paints above it; the form's own
        * accessibility tree comes from the served HTML. */}
      <div
        ref={mountRef}
        className="webview-form-viewer__embed"
        data-testid="webview-form-viewer-embed"
        aria-hidden="true"
      />
      <div className="webview-form-viewer__chrome">
        <button
          type="button"
          className="webview-form-viewer__btn"
          data-testid="webview-form-viewer-close-btn"
          onClick={onClose}
        >
          Close
        </button>
      </div>
    </div>
  );
}
