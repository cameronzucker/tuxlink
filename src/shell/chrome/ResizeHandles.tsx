import { getCurrentWindow } from '@tauri-apps/api/window';
import './chrome.css';

// NOTE: in @tauri-apps/api 2.11.0, `ResizeDirection` is declared in window.d.ts but
// NOT exported (neither value nor type), so it cannot be imported. It is just a
// string union; we declare it locally — a string literal is structurally assignable
// to startResizeDragging's parameter. A borderless (decorations:false) GTK window has
// no native resize grips (spec §5); these invisible edge/corner handles call
// startResizeDragging so the window stays resizable. PRIMARY RISK: validate on
// labwc/Wayland in the grim smoke.
type ResizeDir =
  | 'North' | 'South' | 'East' | 'West'
  | 'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';

const HANDLES: { cls: string; dir: ResizeDir }[] = [
  { cls: 'n', dir: 'North' },
  { cls: 's', dir: 'South' },
  { cls: 'e', dir: 'East' },
  { cls: 'w', dir: 'West' },
  { cls: 'ne', dir: 'NorthEast' },
  { cls: 'nw', dir: 'NorthWest' },
  { cls: 'se', dir: 'SouthEast' },
  { cls: 'sw', dir: 'SouthWest' },
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
