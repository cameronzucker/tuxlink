// src/dock/PopTitleBar.tsx — slim custom chrome for a popped surface's own OS
// window (spec §4). Models on src/help/HelpTitleBar.tsx for the drag-region +
// getCurrentWindow() minimize/maximize mechanics, with one deliberate
// divergence: HelpTitleBar's ✕ calls `win.close()` directly, which pop
// windows must NEVER do (a pop window's close is ALWAYS a dock-back — the
// backend's `surface_dock_back` command destroys the window itself, never a
// bare native close). `onDockBack` and `onClose` are both owned by
// PoppedSurfaceHost, which routes every path (⇤, ✕, Ctrl+W, close-intent)
// through the same collected-state dock-back call.
import { getCurrentWindow } from '@tauri-apps/api/window';
import '../shell/chrome/chrome.css';
import './PoppedSurfaceHost.css';

export interface PopTitleBarProps {
  /** Static per-surface title (spec §3 wire table) — never changes while the
   *  window is open, even if the mounted surface navigates internally. */
  title: string;
  /** ⇤ Dock back — collects state, dockBack(surface, { foreground: true, state }). */
  onDockBack: () => void;
  /** ✕ — collects state, dockBack(surface, { foreground: false, state }). Never
   *  `win.close()` (see module doc above). */
  onClose: () => void;
}

export function PopTitleBar({ title, onDockBack, onClose }: PopTitleBarProps) {
  const win = getCurrentWindow();
  return (
    <div className="tux-titlebar pop-titlebar">
      <button
        type="button"
        className="tux-ctrl pop-dockback"
        title="Dock back into main window"
        aria-label="Dock back into main window"
        onClick={onDockBack}
      >
        ⇤
      </button>
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-name pop-title">{title}</span>
      <span className="tux-controls">
        <button type="button" className="tux-ctrl tux-min" title="Minimize" aria-label="Minimize"
          onClick={() => void win.minimize()}>−</button>
        <button type="button" className="tux-ctrl tux-max" title="Maximize" aria-label="Maximize"
          onClick={() => void win.toggleMaximize()}>□</button>
        <button type="button" className="tux-ctrl tux-close" title="Close" aria-label="Close"
          onClick={onClose}>×</button>
      </span>
    </div>
  );
}
