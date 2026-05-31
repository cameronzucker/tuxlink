// Dev-only mailbox fixture (the approved design is Mock B — principles-faithful).
//
// v0.0.1 ships with the live Pat backend STUBBED (AppBackend = None →
// mailbox_list returns NotConfigured → empty states), so the rows + reading
// pane + dashboard + session log can't be SEEN without sample data. This module
// supplies the EXACT content of the approved Mock B so a `grim` screenshot
// reproduces docs/design/mockups/images/mock-b-principles-faithful.png.
//
// ACTIVATION: OPT-IN. The fixture is OFF by default everywhere — including the
// `vite` dev server — so `tauri dev` shows the REAL backend (real callsign/grid,
// the real native mailbox). It is enabled ONLY for deliberate design work by
// setting `VITE_TUXLINK_FIXTURE=1` before `pnpm tauri dev`.
//
// Rationale (tuxlink-0ic): the client is now live-CMS-capable, so masking the
// real station identity with a fictional callsign/grid by default is unsafe — an
// operator must never see a call sign that isn't theirs on a client that can
// transmit. The fixture remains available for reproducing the Mock B design
// (grim screenshots), but never by default.

import type { MailboxFolder, MessageMeta, ParsedMessage } from './types';

/**
 * True ONLY when explicitly opted in for design work, under the vite dev server.
 * Set `VITE_TUXLINK_FIXTURE=1` before `pnpm tauri dev` to populate the UI with
 * the Mock B sample content. Off by default (and in tests + production builds),
 * so the real backend — real identity, real mailbox — drives the UI.
 */
export const DEV_FIXTURE =
  import.meta.env.MODE === 'development' && import.meta.env.VITE_TUXLINK_FIXTURE === '1';

// --- dates RELATIVE to now so formatRowDate renders the mock's labels
// ("12:18" today, "Yesterday", "N days ago") whenever run. ---
const DAY_MS = 24 * 60 * 60 * 1000;
function todayAt(h: number, m: number): string {
  const n = new Date();
  return new Date(Date.UTC(n.getUTCFullYear(), n.getUTCMonth(), n.getUTCDate(), h, m, 0)).toISOString();
}
function daysAgoAt(days: number, h: number, m: number): string {
  return new Date(new Date(todayAt(h, m)).getTime() - days * DAY_MS).toISOString();
}

// --- Dashboard ribbon + status-bar strings (mock B values) ---
export const DEV_CALLSIGN = 'W4PHS';
export const DEV_GRID = 'EM75xx';
export const DEV_POSITION = 'GPS · 35.05° -90.04°';
export const DEV_CONNECTION_DASH = 'Idle · telnet ready'; // dashboard Connection
export const DEV_CONNECTION_STATUS = 'Telnet ready'; // status-bar (good dot)

/** Human session-log steps (mock B), shown by the SessionLog in dev. */
export interface DevLogStep {
  ts: string;
  message: string;
  ok?: boolean;
}
export const DEV_SESSION_LINES: DevLogStep[] = [
  { ts: '14:32:14', message: 'Connecting to Winlink CMS via telnet…' },
  { ts: '14:32:15', message: 'Connected to CMS gateway 1235-2.cms.winlink.org' },
  { ts: '14:32:18', message: 'Receiving message 4 of 4 from WX4MTL@winlink.org' },
  { ts: '14:32:19', message: 'Session complete · 4 received · 1 sent · 7s', ok: true },
];

/** Inbox fixture — the Mock B inbox rows (7), content + ordering byte-for-byte. */
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
      'NWS Memphis has issued a severe thunderstorm watch for Shelby, Tipton, and Fayette counties…',
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
    preview: 'Roll call for Saturday\'s training net. Please reply with QSL and your assigned net number…',
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
    preview: 'Welcome to Winlink. Your test message was received at 2026-05-17 14:32:18 UTC…',
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
    preview: '20m looks marginal but 40m should open up around 02Z. I\'ll be on 7.103…',
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
    preview: 'Welfare check requested by family in Toronto. Subject is at 1245 Brevard Ave, Cocoa…',
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
    preview: '35.2851N 117.9742W · 2300 ft · resupply at Kennedy Meadows tomorrow…',
  },
];

// Sent fixture — the sidebar "Sent" nav-item shows the folder TOTAL (mock: 87),
// so the fixture carries 87 messages (only the count is visible in the mock).
const SENT_BASE: MessageMeta[] = [
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
function buildSent(total: number): MessageMeta[] {
  const out = [...SENT_BASE];
  for (let i = out.length; i < total; i++) {
    out.push({
      id: `DEV-S${i + 1}`,
      from: 'W4PHS@winlink.org',
      to: ['SERVICE@winlink.org'],
      subject: `Outbound message ${i + 1}`,
      date: daysAgoAt(2 + (i % 45), 9, (i * 7) % 60),
      unread: false,
      bodySize: 300 + ((i * 37) % 600),
      hasAttachments: false,
      preview: 'Sent message.',
    });
  }
  return out;
}
export const DEV_SENT: MessageMeta[] = buildSent(87);

// Parsed bodies (reading pane). The selected DEV-3 is the ICS-213 form message
// the mock shows open. `fromDisplay` → reading pane renders "addr · Name".
// `formKind` → the "Form" metadata row label.
const BODIES: Record<
  string,
  {
    body: string;
    isForm?: boolean;
    routing?: string | null;
    fromDisplay?: string;
    formKind?: string;
    formCode?: string;
    formPayloadBytes?: number;
    formId?: string;
    formPayload?: unknown;
  }
> = {
  'DEV-3': {
    fromDisplay: 'Maria / Incident Commander',
    body: '',
    isForm: true,
    formKind: 'ICS-213 · General Message',
    formCode: 'ICS213',
    formPayloadBytes: 980,
    routing: 'CMS via 1235-2.cms.winlink.org',
    formId: 'ICS213_Initial',
    formPayload: {
      formId: 'ICS213_Initial',
      formParameters: {
        xmlFileVersion: '1.0',
        rmsExpressVersion: 'Tuxlink/0.3.0',
        submissionDatetime: '20260530143000',
        sendersCallsign: 'N5VSU',
        gridSquare: 'EM15',
        displayForm: 'ICS213_Initial_Viewer.html',
        replyTemplate: 'ICS213_SendReply.0',
      },
      fields: [
        ['inc_name', 'HURRICANE WALDO'],
        ['to_name', 'James / WX4MTL'],
        ['fm_name', 'Maria / N5VSU'],
        ['subjectline', 'REQUEST SUPPLIES'],
        ['mdate', '2026-05-30'],
        ['mtime', '14:30Z'],
        ['message', 'Need 200 cots and 500 blankets at the Bartlett shelter by 1800Z. Two truckloads from Memphis warehouse. Reply with ETA.'],
        ['approved_name', 'Maria Vasquez'],
        ['approved_postitle', 'Incident Commander'],
      ],
    },
  },
  'DEV-2': {
    fromDisplay: 'Mike / Net Control',
    body: `All TRI-STATE-NET stations,

Saturday training net at 20:46Z on 7.235 MHz LSB.
Backup: 7.103 MHz LSB. Tertiary: 3.943 MHz LSB (after
local sunset).

73,
Mike / K0SWE
TRI-STATE-NET Control`,
    routing: 'CMS via 1235-2.cms.winlink.org',
  },
  'DEV-1': {
    fromDisplay: 'James / EC Shelby County',
    body: `ARES members,

NWS Memphis has issued a severe thunderstorm watch for
Shelby, Tipton, and Fayette counties, effective 18:00Z
through 22:00Z today.

73,
James / WX4MTL
EC Shelby County ARES`,
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

/** Extra reading-pane fields the fixture carries that ParsedMessage doesn't
 *  (the Mock B "Form" metadata row + form-attached box). Null outside dev. */
export function devFormMeta(
  id: string,
): { formKind: string; formCode: string; payloadBytes: number } | null {
  if (!DEV_FIXTURE) return null;
  const b = BODIES[id];
  if (b?.formKind) {
    return { formKind: b.formKind, formCode: b.formCode ?? 'FORM', payloadBytes: b.formPayloadBytes ?? 0 };
  }
  return null;
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
    from: extra.fromDisplay ? `${extra.fromDisplay} <${meta.from}>` : meta.from,
    to: meta.to,
    cc: [],
    date: meta.date,
    body: extra.body ?? meta.preview ?? '',
    attachments: meta.hasAttachments && !extra.isForm
      ? [{ filename: 'attachment.bin', size: meta.bodySize }]
      : [],
    isForm: extra.isForm ?? false,
    routing: extra.routing ?? null,
    formId: extra.formId ?? null,
    formPayload: extra.formPayload ?? null,
  };
}

/** Message the shell pre-selects in dev — DEV-3 (N5VSU / ICS-213), the message
 *  Mock B shows open. */
export const DEV_SELECTED = DEV_FIXTURE ? { folder: 'inbox' as MailboxFolder, id: 'DEV-3' } : null;
