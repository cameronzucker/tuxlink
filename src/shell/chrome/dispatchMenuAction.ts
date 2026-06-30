import type { MenuActionId } from './menuModel';
import { isColorScheme, type ColorScheme } from '../colorScheme';

/** Effects the dispatcher can invoke. Supplied by AppShell (closes over state). */
export interface MenuHandlers {
  openCompose: () => void;
  reply: () => void;
  replyAll: () => void;
  forward: () => void;
  /** Move the open message to Archive (tuxlink-ca5x). No-op when nothing is
   *  open or when the open message is already in Archive. */
  archive: () => void;
  /** Move the open message to the Deleted folder (Trash) (tuxlink-wl7n). No-op
   *  when nothing is open or when the open message is already in Trash. Services
   *  the Message → Delete menubar item; the Del KEY is handled by the reading
   *  pane (MessageViewLoaded). */
  delete: () => void;
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
  setScheme: (id: ColorScheme) => void;
  /** Run the connect-only CMS reachability+auth probe (verify_cms_connection)
   *  and show the inline result overlay (tuxlink-lqw2). Internet telnet, no
   *  transmission. */
  verifyCms: () => void;
  /** Open the inline multi-section Settings panel at its default section
   *  (the panel's own left nav reaches Winlink Account, Location & GPS, etc.),
   *  tuxlink-39b / tuxlink-esb65. */
  openSettings: () => void;
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
  /** Open the Connect an AI agent modal — per-agent MCP copy-paste connect
   *  commands (tuxlink-l9sq4). */
  openConnectAgent: () => void;
  /** Open the Elmer agent pane (tuxlink-13v2l). */
  openElmer: () => void;
  /** Open the Elmer agent pane with the Model section expanded
   *  (tuxlink-1wi5w). Distinct from openElmer so AppShell can set the
   *  expandModel flag independently of a plain Elmer open. */
  openElmerModel: () => void;
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
 * Unhandled ids (an unknown or removed id with no case) are intentionally
 * no-ops rather than throwing.
 */
export function dispatchMenuAction(id: MenuActionId, h: MenuHandlers): void {
  switch (id) {
    case 'menu:message:new': h.openCompose(); return;
    case 'menu:file:quit': h.quit(); return;
    case 'menu:message:reply': h.reply(); return;
    case 'menu:message:reply_all': h.replyAll(); return;
    case 'menu:message:forward': h.forward(); return;
    case 'menu:message:archive': h.archive(); return;
    case 'menu:message:delete': h.delete(); return;
    case 'menu:message:print': h.print(); return;
    // tuxlink-eymu: the Request Center replaces the standalone Catalog Request
    // menu item. GRIB File Request… opens it directly on its 'grib' view.
    case 'menu:message:request_center': h.openRequestCenter(); return;
    // tuxlink-6jpf: "Find a Gateway" relocated from Message → Tools.
    case 'menu:tools:find_gateway': h.openCatalogBuilder(); return;
    // tuxlink-lqw2: Verify CMS Connection — runs the connect-only probe and
    // shows the inline result overlay. Relocated from the (removed) Session menu.
    case 'menu:tools:verify_cms': h.verifyCms(); return;
    case 'menu:message:grib_request': h.openRequestCenter('grib'); return;
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
    case 'menu:view:radio_panel': h.toggleRadioPanel(); return;
    // tuxlink-esb65: one honest "Settings…" entry opens the inline multi-section
    // Settings panel. Replaces the former settings_privacy + settings_account
    // leaves, which both opened this same panel (the GPS-flavored naming hid the
    // other sections). The account section is reached via the panel's left nav.
    case 'menu:tools:settings':
      h.openSettings(); return;
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
    // tuxlink-l9sq4: Tools → Connect an AI agent opens the ConnectAgentModal.
    case 'menu:tools:connect_agent':
      h.openConnectAgent(); return;
    // tuxlink-13v2l: Tools → Elmer opens the Elmer agent pane.
    case 'menu:tools:elmer':
      h.openElmer(); return;
    // tuxlink-1wi5w: Tools → Set up Elmer's model… opens the Elmer pane with
    // the Model section expanded. connect_agent / ConnectAgentModal are UNCHANGED.
    case 'menu:tools:elmer_model':
      h.openElmerModel(); return;
  }
  if (id.startsWith('menu:view:scheme:')) {
    const scheme = id.slice('menu:view:scheme:'.length);
    if (isColorScheme(scheme)) h.setScheme(scheme);
    return;
  }
  // Unknown / removed ids with no case reach here: no-op.
}
