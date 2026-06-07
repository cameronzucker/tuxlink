import { getCurrentWindow } from '@tauri-apps/api/window';
import iconUrl from '../assets/tuxlink-icon.png';
import '../shell/chrome/chrome.css';

export function LoggingTitleBar() {
  const win = getCurrentWindow();
  return (
    <div className="tux-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <img className="tux-app-icon" src={iconUrl} alt="" />
      <span className="tux-app-name">Tuxlink</span>
      <span className="tux-app-sub">— Logging</span>
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
