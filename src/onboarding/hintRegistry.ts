import type { HintEntry } from './types';

/** Tips shown at user discretion. Copy is written for onboarding. */
export const HINTS: HintEntry[] = [
  {
    id: 'find-a-station', anchor: 'find-a-station',
    title: 'Find a station',
    body: 'Discover nearby stations by FT8 band and frequency. Select your band to see active stations calling in your area.',
    fallback: 'skip',
  },
  {
    id: 'aprs', anchor: 'aprs',
    title: 'APRS',
    body: 'Monitor tactical chat and position reports from nearby stations on a real-time map. Choose a frequency to receive APRS traffic near you.',
    fallback: 'skip',
  },
  {
    id: 'settings', anchor: 'settings',
    title: 'Settings',
    body: 'Configure your callsign, radio modes, and operational preferences. Start with General settings to set your callsign and preferred bands.',
    fallback: 'skip',
  },
  {
    id: 'compose', anchor: 'compose',
    title: 'Compose',
    body: 'Compose opens in its own window so you can keep browsing mail while you write. Start with a new message and pick recipients from your contacts.',
    fallback: 'skip',
  },
];
