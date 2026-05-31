// Single source of truth for the menu (tuxlink-ng3). Feeds <MenuBar>, the
// MENU_ACTION_IDS manifest (parity test = the migrated Rust menu_event_ids), and
// the keyboard ACCELERATORS. The menu:* IDs are the stable action vocabulary.

export type MenuActionId = string;

export interface MenuNode {
  /** Action id (leaf). Omitted for separators and pure submenu parents. */
  id?: MenuActionId;
  label?: string;
  /** Display-only accelerator hint (the real binding lives in ACCELERATORS). */
  accel?: string;
  separator?: boolean;
  submenu?: MenuNode[];
  /** Not-yet-wired item: rendered disabled with a "v0.1" badge so it reads as
   *  "coming" rather than broken (tuxlink-39b). Keeps its id in the vocabulary. */
  disabled?: boolean;
}

export interface TopMenu {
  label: string;
  items: MenuNode[];
}

export const MENU_TREE: TopMenu[] = [
  { label: 'File', items: [
    { id: 'menu:file:quit', label: 'Quit', accel: 'Ctrl+Q' },
  ] },
  { label: 'Message', items: [
    { id: 'menu:message:new', label: 'New Message', accel: 'Ctrl+N' },
    { separator: true },
    { id: 'menu:message:reply', label: 'Reply', accel: 'Ctrl+R' },
    { id: 'menu:message:reply_all', label: 'Reply All', accel: 'Ctrl+Shift+R' },
    { id: 'menu:message:forward', label: 'Forward' },
    { id: 'menu:message:print', label: 'Print', accel: 'Ctrl+P' },
  ] },
  { label: 'Session', items: [
    { id: 'menu:session:connect', label: 'Connect', accel: 'F5' },
    { id: 'menu:session:disconnect', label: 'Disconnect' },
    { separator: true },
    { id: 'menu:session:log', label: 'Session Log' },
    { id: 'menu:session:test_send', label: 'Test send' },
    { id: 'menu:session:show_transport', label: 'Show transport' },
  ] },
  { label: 'Mailbox', items: [
    { id: 'menu:mailbox:inbox', label: 'Inbox' },
    { id: 'menu:mailbox:sent', label: 'Sent' },
    { id: 'menu:mailbox:outbox', label: 'Outbox' },
  ] },
  { label: 'View', items: [
    // Session-log items removed in radio-panel-shell P1.6 — the bottom
    // session-log strip is gone; the log moves into the radio panel.
    { id: 'menu:view:status_bar', label: 'Toggle Status Bar' },
    { id: 'menu:view:radio_panel', label: 'Toggle Radio Panel', accel: 'Ctrl+Shift+M' },
    { separator: true },
    { label: 'Color scheme', submenu: [
      { id: 'menu:view:scheme:default', label: 'Default' },
      { id: 'menu:view:scheme:night-red', label: 'Night / tactical (red)' },
      { id: 'menu:view:scheme:grayscale', label: 'Grayscale' },
    ] },
  ] },
  { label: 'Tools', items: [
    // Not-yet-wired (v0.0.1): disabled + badged so they read as "coming", not broken.
    { id: 'menu:tools:templates', label: 'Templates', disabled: true },
    { id: 'menu:tools:rig_control', label: 'Rig Control', disabled: true },
    { separator: true },
    { label: 'Settings', submenu: [
      { id: 'menu:tools:settings_connection', label: 'Connection', disabled: true },
      // tuxlink-39b: one entry opens the GPS/privacy settings panel (gps_state +
      // position precision). The former granular leaves (GPS state / Position
      // precision / a duplicate GPS) all opened the same box — consolidated.
      { id: 'menu:tools:settings_privacy', label: 'GPS & Privacy…' },
    ] },
    // "Preferences" removed — it duplicated "Settings" (operator call 2026-05-22).
  ] },
  { label: 'Help', items: [
    { id: 'menu:help:about', label: 'About Tuxlink' },
    { id: 'menu:help:docs', label: 'Documentation' },
    { id: 'menu:help:report_issue', label: 'Report Issue' },
  ] },
];

/** Depth-first flatten of every action id, in layout order. */
function collectIds(nodes: MenuNode[]): MenuActionId[] {
  const out: MenuActionId[] = [];
  for (const n of nodes) {
    if (n.id) out.push(n.id);
    if (n.submenu) out.push(...collectIds(n.submenu));
  }
  return out;
}

export const MENU_ACTION_IDS: MenuActionId[] =
  MENU_TREE.flatMap((m) => collectIds(m.items));

export interface Accelerator {
  /** Human label, e.g. "Ctrl+Shift+O". */
  combo: string;
  key: string;        // KeyboardEvent.key, case-insensitive match (e.g. 'n', 'F5')
  ctrl: boolean;      // Ctrl OR Meta (CmdOrCtrl)
  shift: boolean;
  id: MenuActionId;
}

// Operator-locked set (2026-05-21). F5 and Ctrl+Shift+O both fire connect.
export const ACCELERATORS: Accelerator[] = [
  { combo: 'Ctrl+N', key: 'n', ctrl: true, shift: false, id: 'menu:message:new' },
  { combo: 'Ctrl+R', key: 'r', ctrl: true, shift: false, id: 'menu:message:reply' },
  { combo: 'Ctrl+Shift+R', key: 'r', ctrl: true, shift: true, id: 'menu:message:reply_all' },
  { combo: 'Ctrl+P', key: 'p', ctrl: true, shift: false, id: 'menu:message:print' },
  { combo: 'Ctrl+Q', key: 'q', ctrl: true, shift: false, id: 'menu:file:quit' },
  { combo: 'Ctrl+Shift+M', key: 'm', ctrl: true, shift: true, id: 'menu:view:radio_panel' },
  { combo: 'F5', key: 'F5', ctrl: false, shift: false, id: 'menu:session:connect' },
  { combo: 'Ctrl+Shift+O', key: 'o', ctrl: true, shift: true, id: 'menu:session:connect' },
];
