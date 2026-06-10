import type { MenuActionId } from './menuModel';
import type { MailboxFolder } from '../../mailbox/types';
import { isColorScheme, type ColorScheme } from '../colorScheme';

/** Effects the dispatcher can invoke. Supplied by AppShell (closes over state). */
export interface MenuHandlers {
  openCompose: () => void;
  connect: () => void;
  reply: () => void;
  replyAll: () => void;
  forward: () => void;
  /** Move the open message to Archive (tuxlink-ca5x). No-op when nothing is
   *  open or when the open message is already in Archive. */
  archive: () => void;
  /** Print the open message via the webview's native print dialog
   *  (tuxlink-j0m3). No-op when nothing is open — Ctrl+P with no
   *  selection shouldn't open the system print dialog on an empty
   *  reading pane. Follow-up tuxlink-zdfj filed for the @media print
   *  stylesheet that drops the dashboard/sidebar/statusbar chrome. */
  print: () => void;
  // toggleSessionLog removed in radio-panel-shell P1.6 — the bottom session-log
  // strip is gone; the log moves into the radio panel as a per-mode section.
  toggleStatusBar: () => void;
  toggleRadioPanel: () => void;
  selectFolder: (folder: MailboxFolder) => void;
  setScheme: (id: ColorScheme) => void;
  /** Open the inline Settings panel (GPS state + position precision), tuxlink-39b. */
  openSettings: () => void;
  /** Open the inline LAN map-tile source settings overlay (tuxlink-a1cc / dyop, design §8.7). */
  openMapTileSettings: () => void;
  /** Open the inline Theme Designer (View → Color Scheme → Customize…), tuxlink-vgth. */
  openThemeDesigner: () => void;
  /** Open the inline About Tuxlink dialog (tuxlink-35g0). */
  openAbout: () => void;
  /** Open the inline Help / Documentation panel (tuxlink-35g0). */
  openHelp: () => void;
  /** Open the Logging window (tuxlink-qjgx Task 8). */
  openLogging: () => void;
  /** Open the Report Issue modal — auto-export + pre-filled GitHub URL (tuxlink-qjgx Task 8). */
  reportIssue: () => void;
  /** Open the inline uninstall cleanup dialog (tuxlink-uodl). */
  openUninstallCleanup: () => void;
  /** Open the inline Catalog Builder panel (tuxlink-a2gd) — location-aware
   *  station finder (direct /listings poll) + by-message info-category requests. */
  openCatalogBuilder: () => void;
  /** Open the inline Request Center overlay (tuxlink-eymu) — unified catalog
   *  browse + WLE inquiries + Saildocs GRIB requests. The optional initialView
   *  selects the inner view ('home' default; 'grib' from GRIB File Request…). */
  openRequestCenter: (initialView?: 'home' | 'browse' | 'grib') => void;
  quit: () => void;
}

/**
 * Route a menu:* action id (from an HTML menu click OR a keyboard accelerator)
 * to the matching handler. In-process, main-window only — there is no app-global
 * event broadcast (which is what caused tuxlink-msr + the F7 recursion guard).
 * Unhandled ids (stub actions: tools/help/raw_log/etc.) are intentionally no-ops.
 */
export function dispatchMenuAction(id: MenuActionId, h: MenuHandlers): void {
  switch (id) {
    case 'menu:message:new': h.openCompose(); return;
    case 'menu:file:quit': h.quit(); return;
    case 'menu:session:connect': h.connect(); return;
    case 'menu:message:reply': h.reply(); return;
    case 'menu:message:reply_all': h.replyAll(); return;
    case 'menu:message:forward': h.forward(); return;
    case 'menu:message:archive': h.archive(); return;
    case 'menu:message:print': h.print(); return;
    // tuxlink-eymu: the Request Center replaces the standalone Catalog Request
    // menu item. GRIB File Request… opens it directly on its 'grib' view.
    case 'menu:message:request_center': h.openRequestCenter(); return;
    // tuxlink-6jpf: "Find a Gateway" relocated from Message → Tools.
    case 'menu:tools:find_gateway': h.openCatalogBuilder(); return;
    case 'menu:message:grib_request': h.openRequestCenter('grib'); return;
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
    case 'menu:view:radio_panel': h.toggleRadioPanel(); return;
    // tuxlink-39b: the consolidated GPS & Privacy settings item opens the inline
    // Settings panel (previously a cluster of dead no-op stubs found in the
    // post-merge smoke of #113).
    case 'menu:tools:settings_privacy':
      h.openSettings(); return;
    // tuxlink-a1cc / dyop: opens the LAN map-tile source config overlay — the
    // one reachable home for the dyop tile backend (design §8.7).
    case 'menu:tools:settings_map_tiles':
      h.openMapTileSettings(); return;
    // tuxlink-vgth: opens the inline Theme Designer panel.
    case 'menu:view:customize_theme':
      h.openThemeDesigner(); return;
    // tuxlink-35g0: Help menu wiring. About + Documentation are inline
    // overlays. tuxlink-qjgx Task 8: Logging opens the Logging window;
    // Report Issue triggers the auto-export + GitHub URL flow (spec §8.5).
    case 'menu:help:about':
      h.openAbout(); return;
    case 'menu:help:docs':
      h.openHelp(); return;
    case 'menu:help:logging':
      h.openLogging(); return;
    case 'menu:help:report_issue':
      h.reportIssue(); return;
    case 'menu:help:uninstall_cleanup':
      h.openUninstallCleanup(); return;
    case 'menu:mailbox:inbox':
    case 'menu:mailbox:sent':
    case 'menu:mailbox:outbox':
    case 'menu:mailbox:archive':
      h.selectFolder(id.slice('menu:mailbox:'.length) as MailboxFolder);
      return;
  }
  if (id.startsWith('menu:view:scheme:')) {
    const scheme = id.slice('menu:view:scheme:'.length);
    if (isColorScheme(scheme)) h.setScheme(scheme);
    return;
  }
  // Stub / not-yet-wired actions (tools, help, disconnect, raw_log, …): no-op.
}
