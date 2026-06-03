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
  /** Not-yet-wired item: rendered disabled with a "soon" badge so it reads as
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
    { separator: true },
    // tuxlink-ca5x (user-folders Phase 1): move open message to Archive.
    // The `A` accelerator is gated on input-focus (see useAccelerators.ts).
    { id: 'menu:message:archive', label: 'Archive', accel: 'A' },
    { separator: true },
    // tuxlink-ddiq: WLE catalog-request feature (Inquiry messages).
    // Opens the CatalogRequestPanel; routes through the existing outgoing
    // rails to INQUIRY@winlink.org.
    { id: 'menu:message:catalog_request', label: 'Catalog Request…' },
    // tuxlink-vrpk: Saildocs GRIB request (3rd-party SMTP, not Winlink CMS).
    // Opens GribRequestPanel; routes through outgoing rails to
    // query@saildocs.com.
    { id: 'menu:message:grib_request', label: 'GRIB File Request…' },
    { separator: true },
    // tuxlink-j0m3: Print fires window.print() via the openMessage-gated
    // handler in AppShell. No-op when nothing is selected, which keeps
    // Ctrl+P from opening the print dialog on an empty reading pane.
    // @media print stylesheet follow-up tracked at tuxlink-zdfj.
    { id: 'menu:message:print', label: 'Print', accel: 'Ctrl+P' },
  ] },
  { label: 'Session', items: [
    { id: 'menu:session:connect', label: 'Connect', accel: 'F5' },
    // Not-yet-wired: dispatchMenuAction routes these to its safe no-op default
    // (tuxlink-dpf). Disabled + badged so they read as "coming", not broken.
    { id: 'menu:session:disconnect', label: 'Disconnect', disabled: true },
    { separator: true },
    { id: 'menu:session:log', label: 'Session Log', disabled: true },
    { id: 'menu:session:verify_cms', label: 'Verify CMS Connection', disabled: true },
    { id: 'menu:session:show_transport', label: 'Show transport', disabled: true },
  ] },
  { label: 'Mailbox', items: [
    { id: 'menu:mailbox:inbox', label: 'Inbox' },
    { id: 'menu:mailbox:sent', label: 'Sent' },
    { id: 'menu:mailbox:outbox', label: 'Outbox' },
    { id: 'menu:mailbox:archive', label: 'Archive' },
  ] },
  { label: 'View', items: [
    // Session-log items removed in radio-panel-shell P1.6 — the bottom
    // session-log strip is gone; the log moves into the radio panel.
    // tuxlink-qxqj: the bottom bar's content is mailbox queue + unread state
    // (the connection chip moved out; it duplicated DashboardRibbon). Menu
    // label tracks the new purpose; the action id stays so muscle-memory
    // keybindings and tests don't churn.
    { id: 'menu:view:status_bar', label: 'Toggle Mailbox Bar' },
    { id: 'menu:view:radio_panel', label: 'Toggle Radio Panel', accel: 'Ctrl+Shift+M' },
    { separator: true },
    // tuxlink-c22r + tuxlink-vgth: dark default, three light presets, two
    // specialty schemes, then a separator and the operator's saved custom
    // theme + the Customize designer entry. The Customize action opens an
    // inline panel (per feedback_inline_ui_no_window_clutter), not a window.
    { label: 'Color scheme', submenu: [
      { id: 'menu:view:scheme:default', label: 'Default (dark)' },
      { id: 'menu:view:scheme:daylight', label: 'Daylight (light)' },
      { id: 'menu:view:scheme:high-contrast-light', label: 'High contrast (light)' },
      { id: 'menu:view:scheme:paper', label: 'Paper (warm light)' },
      { id: 'menu:view:scheme:night-red', label: 'Night / tactical (red)' },
      { id: 'menu:view:scheme:grayscale', label: 'Grayscale' },
      { separator: true },
      { id: 'menu:view:scheme:custom', label: 'My custom theme' },
      { id: 'menu:view:customize_theme', label: 'Customize…' },
    ] },
  ] },
  { label: 'Tools', items: [
    // Not-yet-wired: disabled + badged so they read as "coming", not broken.
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
  /** When true, the accelerator is suppressed while a text input / textarea /
   *  contenteditable element is focused — required for plain-letter bindings
   *  (e.g. `A` for Archive) so they don't intercept typing. Modifier-bound
   *  accelerators (Ctrl+*, Ctrl+Shift+*, F-keys) don't set this. (tuxlink-ca5x) */
  suppressInTextInput?: boolean;
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
  // tuxlink-ca5x: Archive shortcut — plain `A`, gated on text-input focus so
  // typing the letter 'a' in the search bar or compose body doesn't archive.
  { combo: 'A', key: 'a', ctrl: false, shift: false, id: 'menu:message:archive', suppressInTextInput: true },
];
