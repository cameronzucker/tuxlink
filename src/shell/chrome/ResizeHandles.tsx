import { getCurrentWindow, ResizeDirection } from '@tauri-apps/api/window';
import './chrome.css';

// A borderless (decorations:false) GTK window has no native resize grips
// (spec §5). These invisible edge/corner handles call startResizeDragging so the
// window stays resizable. PRIMARY RISK: validate on labwc/Wayland in the grim smoke.
const HANDLES: { cls: string; dir: ResizeDirection }[] = [
  { cls: 'n', dir: ResizeDirection.North },
  { cls: 's', dir: ResizeDirection.South },
  { cls: 'e', dir: ResizeDirection.East },
  { cls: 'w', dir: ResizeDirection.West },
  { cls: 'ne', dir: ResizeDirection.NorthEast },
  { cls: 'nw', dir: ResizeDirection.NorthWest },
  { cls: 'se', dir: ResizeDirection.SouthEast },
  { cls: 'sw', dir: ResizeDirection.SouthWest },
];

export function ResizeHandles() {
  const win = getCurrentWindow();
  return (
    <>
      {HANDLES.map((h) => (
        <div
          key={h.cls}
          className={`tux-resize ${h.cls}`}
          onMouseDown={(e) => {
            if (e.button === 0) void win.startResizeDragging(h.dir);
          }}
        />
      ))}
    </>
  );
}
