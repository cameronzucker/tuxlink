// icons.tsx — shared line-icon set for the Request Center redesign
// (bd-tuxlink-hbbw, Task 1).
//
// Usage:
//   <Icon name="weather" />
//   <Icon name="pin" size={15} />
//   <Icon name="arrow" className="lead" />
//
// All icons use stroke="currentColor" fill="none" so callers control color
// via the CSS `color` property. `aria-hidden="true"` on every SVG — icons are
// decorative; the accessible label lives on the surrounding button/element.
// strokeWidth 1.85 and round caps/joins are part of the approved visual language.

import type { ReactNode } from 'react';

export type IconName =
  | 'pin'
  | 'search'
  | 'close'
  | 'plus'
  | 'arrow'
  | 'check'
  | 'radio'
  | 'weather'
  | 'wave'
  | 'prop'
  | 'sun'
  | 'aurora'
  | 'tower'
  | 'info'
  | 'list'
  | 'map'
  | 'basket'
  | 'trash';

// Inner SVG content keyed by icon name. viewBox is 0 0 24 24 for all icons.
const ICON_PATHS: Record<IconName, ReactNode> = {
  pin: (
    <>
      <path d="M12 21s-7-6.5-7-11a7 7 0 1 1 14 0c0 4.5-7 11-7 11z" />
      <circle cx="12" cy="10" r="2.4" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6L6 18" />,
  plus: <path d="M12 5v14M5 12h14" />,
  arrow: <path d="M5 12h14M13 6l6 6-6 6" />,
  check: <path d="M20 6L9 17l-5-5" />,
  radio: (
    <>
      <path d="M5 12a14 14 0 0 1 14 0" />
      <path d="M8 15a8 8 0 0 1 8 0" />
      <circle cx="12" cy="18" r="1.3" />
    </>
  ),
  weather: (
    <>
      <circle cx="8" cy="9" r="3.2" />
      <path d="M8 2.5v1.6M3.2 4l1.1 1.1M2 9h1.6M13 16.5a3.5 3.5 0 0 0-1-6.8 5 5 0 0 0-9.5 1.6A3.5 3.5 0 0 0 4 18h9a2.6 2.6 0 0 0 0-1.5z" />
    </>
  ),
  wave: (
    <>
      <path d="M2 8c2 0 2 1.6 4 1.6S8 8 10 8s2 1.6 4 1.6S16 8 18 8s2 1.6 4 1.6" />
      <path d="M2 13c2 0 2 1.6 4 1.6S8 13 10 13s2 1.6 4 1.6 2-1.6 4-1.6 2 1.6 4 1.6" />
      <path d="M2 18c2 0 2 1.6 4 1.6S8 18 10 18s2 1.6 4 1.6 2-1.6 4-1.6 2 1.6 4 1.6" />
    </>
  ),
  prop: (
    <>
      <path d="M4.5 9a10 10 0 0 1 15 0" />
      <path d="M7.5 12a6 6 0 0 1 9 0" />
      <circle cx="12" cy="16" r="1.6" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8" />
    </>
  ),
  aurora: (
    <>
      <path d="M4 20c1.5-7 3-11 5-11s2 5 3 5 2.5-5 4.5-5 2.5 4 3.5 6" />
      <path d="M6 5l.6 1.6L8 7l-1.4.4L6 9l-.6-1.6L4 7l1.4-.4z" />
      <path d="M17 3l.5 1.3L19 5l-1.5.4L17 7l-.5-1.6L15 5l1.5-.3z" />
    </>
  ),
  tower: (
    <>
      <path d="M12 9v12M8 21h8" />
      <path d="M7.5 6.5a6 6 0 0 1 9 0" />
      <path d="M5 4a9 9 0 0 1 14 0" />
      <circle cx="12" cy="9" r="1.4" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5M12 7.5v.5" />
    </>
  ),
  list: <path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01" />,
  map: (
    <>
      <path d="M9 4 3 6.5v13L9 17l6 2.5 6-2.5v-13L15 7 9 4.5z" />
      <path d="M9 4.5v12.5M15 7v12.5" />
    </>
  ),
  basket: (
    <>
      <path d="M5 8h14l-1.2 10.2a2 2 0 0 1-2 1.8H8.2a2 2 0 0 1-2-1.8z" />
      <path d="M9 8l3-4 3 4" />
      <path d="M9.5 12v4M14.5 12v4" />
    </>
  ),
  trash: <path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12" />,
};

export interface IconProps {
  name: IconName;
  size?: number;
  className?: string;
}

export function Icon({ name, size = 18, className }: IconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.85}
      strokeLinecap="round"
      strokeLinejoin="round"
      width={size}
      height={size}
      aria-hidden="true"
      className={className}
    >
      {ICON_PATHS[name]}
    </svg>
  );
}
