import type { HintEntry } from './types';

/** Order matters — this IS the tour. Anchors are placed in Task 6. */
export const TOUR_STOPS: HintEntry[] = [
  {
    id: 'ribbon-connect', anchor: 'ribbon-connect',
    title: 'Connect',
    body: 'One click runs your last-configured session — dial, exchange mail, disconnect. Nothing transmits until you click it.',
    fallback: 'center',
    openHint: 'The Connect button lives at the right end of the status ribbon.',
  },
  {
    id: 'mailbox', anchor: 'mailbox',
    title: 'Mailbox',
    body: 'Messages land here after a connect. Folders on the left; unread counts on the ribbon.',
    requiredPanelState: 'mailbox-visible', fallback: 'skip',
    openHint: 'Select any mail folder in the left sidebar.',
  },
  {
    id: 'contacts', anchor: 'contacts',
    title: 'Contacts',
    body: 'The one address surface: everyone you know, star Favorites, and stations you have heard.',
    fallback: 'center',
    openHint: 'Open the Contacts folder in the left sidebar.',
  },
  {
    id: 'radio-dock', anchor: 'radio-dock',
    title: 'Radio dock',
    body: 'When you start a radio mode (ARDOP, VARA, packet), its panel docks here — arming a listener, session status, and the dial all live in it.',
    requiredPanelState: 'radio-dock-open', fallback: 'center',
    openHint: 'Pick a radio mode from the ribbon to open its dock panel.',
  },
  {
    id: 'elmer', anchor: 'elmer',
    title: 'Elmer',
    body: 'Your built-in assistant. Ask it anything about the app or the hobby — try: where do I connect?',
    fallback: 'center',
    openHint: 'Elmer opens from its button on the status ribbon.',
  },
];
