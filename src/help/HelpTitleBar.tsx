/**
 * Custom titlebar for the help window (tuxlink-ew3k). Matches the project's
 * chrome (.tux-titlebar from src/shell/chrome/chrome.css) so the help window
 * doesn't look out of place next to the main client; the bare OS-native
 * GTK chrome was visually jarring on operator smoke.
 *
 * Spec §3.2 originally deferred custom chrome to v1.1 to avoid duplicating
 * drag-region wiring. The duplication turned out to be minimal — the help
 * version doesn't need a menu bar, just an icon + title + min/max/close
 * cluster — so we ship it now alongside the other polish fixes.
 *
 * Lives on bd-tuxlink-ew3k/help-polish; help_window.rs is updated to
 * `.decorations(false)` so this component owns the chrome.
 */
import { getCurrentWindow } from '@tauri-apps/api/window';
import iconUrl from '../assets/tuxlink-icon.png';
import '../shell/chrome/chrome.css';

export function HelpTitleBar() {
  const win = getCurrentWindow();
  return (
    <div className="tux-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <img className="tux-app-icon" src={iconUrl} alt="" />
      <span className="tux-app-name">Tuxlink</span>
      <span className="tux-app-sub">— Documentation</span>
      <span className="tux-controls">
        <button className="tux-ctrl tux-min" title="Minimize" aria-label="Minimize"
          onClick={() => void win.minimize()}>−</button>
        <button className="tux-ctrl tux-max" title="Maximize" aria-label="Maximize"
          onClick={() => void win.toggleMaximize()}>□</button>
        <button className="tux-ctrl tux-close" title="Close" aria-label="Close"
          onClick={() => void win.close()}>×</button>
      </span>
    </div>
  );
}
