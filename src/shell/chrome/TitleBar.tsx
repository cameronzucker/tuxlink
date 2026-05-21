import { getCurrentWindow } from '@tauri-apps/api/window';
import './chrome.css';

interface TitleBarProps {
  folderLabel: string;
}

/**
 * Custom dark titlebar (tuxlink-ng3). Drag region + Adwaita-style controls.
 * Close calls window.close() → the existing lib.rs CloseRequested handler keeps
 * the app alive on Linux (minimizes); only File→Quit / Ctrl+Q exit.
 */
export function TitleBar({ folderLabel }: TitleBarProps) {
  const win = getCurrentWindow();
  return (
    <div className="tux-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-icon">T</span>
      <span className="tux-app-name">Tuxlink</span>
      <span className="tux-app-sub">— {folderLabel}</span>
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
