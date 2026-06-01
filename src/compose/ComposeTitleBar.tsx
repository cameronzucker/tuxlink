import { getCurrentWindow } from '@tauri-apps/api/window';
import '../shell/chrome/chrome.css';

interface ComposeTitleBarProps {
  onClose: () => void;
}

/**
 * Dark title bar for the compose window (tuxlink-ng3 / closes msr).
 * No menu (the compose window must not show the main menu). Minimize +
 * Maximize/Restore go straight to the window API (no unsaved-changes risk);
 * Close delegates to handleRequestClose so the unsaved-changes prompt
 * (spec §5.4) runs before compose_close_self destroys the window.
 */
export function ComposeTitleBar({ onClose }: ComposeTitleBarProps) {
  const win = getCurrentWindow();
  return (
    <div className="tux-compose-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-name">New Message</span>
      <span className="tux-controls">
        <button className="tux-ctrl tux-min" title="Minimize" aria-label="Minimize"
          onClick={() => void win.minimize()}>−</button>
        <button className="tux-ctrl tux-max" title="Maximize" aria-label="Maximize"
          onClick={() => void win.toggleMaximize()}>□</button>
        <button className="tux-ctrl tux-close" title="Close" aria-label="Close"
          onClick={onClose}>×</button>
      </span>
    </div>
  );
}
