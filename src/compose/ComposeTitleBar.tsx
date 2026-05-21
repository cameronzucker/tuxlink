import '../shell/chrome/chrome.css';

interface ComposeTitleBarProps {
  onClose: () => void;
}

/**
 * Minimal dark title bar for the compose window (tuxlink-ng3 / closes msr).
 * No menu (the compose window must not show the main menu). Close delegates to
 * the existing handleRequestClose (unsaved-changes prompt → compose_close_self).
 */
export function ComposeTitleBar({ onClose }: ComposeTitleBarProps) {
  return (
    <div className="tux-compose-titlebar">
      <span className="tux-drag" data-tauri-drag-region />
      <span className="tux-app-name">New Message</span>
      <span className="tux-controls">
        <button className="tux-ctrl tux-close" title="Close" aria-label="Close" onClick={onClose}>×</button>
      </span>
    </div>
  );
}
