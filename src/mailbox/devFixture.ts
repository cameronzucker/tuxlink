// Dev-only mailbox fixture (tuxlink-yd4).
//
// v0.0.1 ships with the live Pat backend STUBBED (AppBackend = None →
// mailbox_list returns NotConfigured → empty states), so the rows + reading
// pane can't be SEEN in the real app without sample data. This module supplies
// realistic Winlink content — deliberately the SAME content as the approved
// Mock D so a `grim` screenshot of the dev app is directly comparable to
// docs/design/mockups/images/mock-d-mailapp-minimal.png.
//
// ACTIVATION: gated on `import.meta.env.MODE === 'development'`, which is:
//   - 'development' under the `vite` dev server  → fixture ON (validation)
//   - 'test'        under vitest                 → fixture OFF (no test pollution)
//   - 'production'  under `vite build`/`tauri build` → fixture OFF + tree-shaken
// So this never ships in a release and never affects the test suite.
//
// When tuxlink-22l lands the live PatBackend, the fixture stays dormant (it only
// fills in when the backend yields nothing); refine the gate then if a dev wants
// to see real empty states.

import type { MailboxFolder, MessageMeta, ParsedMessage } from './types';

/** True only under the live vite dev server (see module header). */
export const DEV_FIXTURE = import.meta.env.MODE === 'development';

// --- date helpers: dates RELATIVE to now so formatRowDate renders the mock's
// relative labels ("12:18Z" today, "Yesterday", "N days ago") whenever run. ---
const DAY_MS = 24 * 60 * 60 * 1000;

/** ISO for today (UTC) at HH:MM. */
function todayAt(h: number, m: number): string {
  const n = new Date();
  return new Date(Date.UTC(n.getUTCFullYear(), n.getUTCMonth(), n.getUTCDate(), h, m, 0)).toISOString();
}
/** ISO for `days` ago (UTC) at HH:MM. */
function daysAgoAt(days: number, h: number, m: number): string {
  const t = todayAt(h, m);
  return new Date(new Date(t).getTime() - days * DAY_MS).toISOString();
}

/** Inbox fixture — mirrors the Mock D inbox rows (content + ordering). */
export const DEV_INBOX: MessageMeta[] = [
  {
    id: 'DEV-1',
    from: 'WX4MTL@winlink.org',
    to: ['W4PHS@winlink.org', 'MEMPHIS-ARES@winlink.org'],
    subject: 'Memphis ARES brief — severe wx expected 18-22Z',
    date: todayAt(12, 18),
    unread: true,
    bodySize: 2458,
    hasAttachments: false,
    preview:
      'NWS Memphis has issued a severe thunderstorm watch for Shelby, Tipton, and Fayette counties, effective 18:00Z…',
  },
  {
    id: 'DEV-2',
    from: 'K0SWE@winlink.org',
    to: ['TRI-STATE-NET@winlink.org'],
    subject: 'Net check-in 7Y3 — 2046Z roll call',
    date: todayAt(11, 45),
    unread: true,
    bodySize: 1229,
    hasAttachments: false,
    preview:
      'Roll call for Saturday’s training net. Please reply with QSL and your assigned net number (1-47)…',
  },
  {
    id: 'DEV-3',
    from: 'N5VSU@winlink.org',
    to: ['W4PHS@winlink.org', 'CHATTANOOGA-CERT@winlink.org'],
    subject: 'INCIDENT 8847 status update',
    date: todayAt(10, 42),
    unread: true,
    bodySize: 980,
    hasAttachments: true,
    formTag: 'ICS-213',
    preview: '[This message contains a Winlink form — form rendering arrives in v0.1]',
  },
  {
    id: 'DEV-4',
    from: 'SERVICE@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'Re: tuxlink setup test',
    date: todayAt(9, 32),
    unread: false,
    bodySize: 412,
    hasAttachments: false,
    preview:
      'Welcome to Winlink. Your test message was received at 2026-05-17 14:32:18 UTC and processed by gateway 1235-2…',
  },
  {
    id: 'DEV-5',
    from: 'VE3FUN@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'RE: HF prop tonight',
    date: daysAgoAt(1, 21, 5),
    unread: false,
    bodySize: 340,
    hasAttachments: false,
    preview: '20m looks marginal but 40m should open up around 02Z. I’ll be on 7.103 LSB calling CQ DX…',
  },
  {
    id: 'DEV-6',
    from: 'EM5KKK@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'Red Cross welfare query — Sarah Chen / Cocoa FL',
    date: daysAgoAt(1, 16, 20),
    unread: false,
    bodySize: 1638,
    hasAttachments: false,
    preview:
      'Welfare check requested by family in Toronto. Subject is at 1245 Brevard Ave, Cocoa, FL. Phone offline since…',
  },
  {
    id: 'DEV-7',
    from: 'KE7VBH@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'Position rpt — Mile 1242 PCT',
    date: daysAgoAt(2, 18, 30),
    unread: false,
    bodySize: 220,
    hasAttachments: false,
    preview:
      '35.2851N 117.9742W · 2300 ft · resupply at Kennedy Meadows tomorrow morning then 8 more days to Sonora…',
  },
  {
    id: 'DEV-8',
    from: 'SYSTEM@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'Account password expires in 14 days',
    date: daysAgoAt(3, 8, 0),
    unread: false,
    bodySize: 380,
    hasAttachments: false,
    preview:
      'Your Winlink CMS password will expire on 2026-05-31. Reset at https://winlink.org/user before that date…',
  },
  {
    id: 'DEV-9',
    from: 'K0SWE@winlink.org',
    to: ['W4PHS@winlink.org'],
    subject: 'Field day scheduling — Sat 13-15 local',
    date: daysAgoAt(4, 14, 10),
    unread: false,
    bodySize: 720,
    hasAttachments: false,
    preview:
      'Final field day prep: Sat 0900-1700 setup, 1300-1500 contesting block. Bring antenna analyzer if you have one…',
  },
];

/** Sent fixture — a couple of outbound messages for the Sent tab. */
export const DEV_SENT: MessageMeta[] = [
  {
    id: 'DEV-S1',
    from: 'W4PHS@winlink.org',
    to: ['WX4MTL@winlink.org'],
    subject: 'Re: Memphis ARES brief — QSL, GO-kit staged',
    date: todayAt(12, 40),
    unread: false,
    bodySize: 612,
    hasAttachments: false,
    preview: 'QSL on the storm watch. Will be on 146.940 by 17:45Z. Brought the GO-kit in yesterday. 73…',
  },
  {
    id: 'DEV-S2',
    from: 'W4PHS@winlink.org',
    to: ['K0SWE@winlink.org'],
    subject: 'Net check-in 7Y3 — W4PHS net #12, QSL',
    date: daysAgoAt(1, 20, 50),
    unread: false,
    bodySize: 388,
    hasAttachments: false,
    preview: 'Net control, W4PHS checking in, assigned net number 12. QSL, no traffic. 73…',
  },
];

// Parsed bodies keyed by message id (reading pane). The selected DEV-2 body is
// the mock's verbatim K0SWE roll-call so the grim shot matches the mock pane.
const BODIES: Record<string, { body: string; isForm?: boolean; routing?: string | null }> = {
  'DEV-2': {
    body: `All TRI-STATE-NET stations,

Saturday training net at 20:46Z on 7.235 MHz LSB.
Backup: 7.103 MHz LSB. Tertiary: 3.943 MHz LSB (after
local sunset).

Please reply to this message with:
  - Your callsign and assigned net number (1-47)
  - QSL or QRX (will-be-late) status
  - Any traffic to be passed during the net

Roll call begins promptly at 20:50Z. Late check-ins
accepted until 21:15Z, then we move to traffic.

Stations with prior commitments — please QRX in your
reply with anticipated check-in time. We'll hold a
slot.

73,
Mike / K0SWE
TRI-STATE-NET Control`,
    routing: 'CMS via 1235-2.cms.winlink.org',
  },
  'DEV-1': {
    body: `ARES members,

NWS Memphis has issued a severe thunderstorm watch for
Shelby, Tipton, and Fayette counties, effective 18:00Z
through 22:00Z today.

ARES net activation at 17:45Z on 146.940 MHz (-) PL 100.0.

73,
James / WX4MTL
EC Shelby County ARES`,
    routing: 'CMS via 1235-2.cms.winlink.org',
  },
  'DEV-3': {
    body: '',
    isForm: true,
    routing: 'CMS via 1235-2.cms.winlink.org',
  },
};

function metaById(id: string): MessageMeta | undefined {
  return DEV_INBOX.find((m) => m.id === id) ?? DEV_SENT.find((m) => m.id === id);
}

/** Sample messages for a folder (empty unless the fixture is active). */
export function devFixtureFor(folder: MailboxFolder): MessageMeta[] {
  if (!DEV_FIXTURE) return [];
  if (folder === 'inbox') return DEV_INBOX;
  if (folder === 'sent') return DEV_SENT;
  return [];
}

/** A parsed message for the reading pane (null unless the fixture is active). */
export function devMessageFor(id: string): ParsedMessage | null {
  if (!DEV_FIXTURE) return null;
  const meta = metaById(id);
  if (!meta) return null;
  const extra = BODIES[id] ?? {};
  return {
    id: meta.id,
    subject: meta.subject,
    from: meta.from,
    to: meta.to,
    cc: [],
    date: meta.date,
    body: extra.body ?? meta.preview ?? '',
    attachments: meta.hasAttachments ? [{ filename: 'attachment.bin', size: meta.bodySize }] : [],
    isForm: extra.isForm ?? false,
    routing: extra.routing ?? null,
  };
}

/** Message the shell pre-selects in dev so the reading pane is visible for
 *  validation — DEV-2 (K0SWE), the message the mock shows open. */
export const DEV_SELECTED = DEV_FIXTURE ? { folder: 'inbox' as MailboxFolder, id: 'DEV-2' } : null;
