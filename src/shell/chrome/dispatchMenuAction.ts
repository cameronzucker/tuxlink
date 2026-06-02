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
  // toggleSessionLog removed in radio-panel-shell P1.6 — the bottom session-log
  // strip is gone; the log moves into the radio panel as a per-mode section.
  toggleStatusBar: () => void;
  toggleRadioPanel: () => void;
  selectFolder: (folder: MailboxFolder) => void;
  setScheme: (id: ColorScheme) => void;
  /** Open the inline Settings panel (GPS state + position precision), tuxlink-39b. */
  openSettings: () => void;
  /** Open the inline Theme Designer (View → Color Scheme → Customize…), tuxlink-vgth. */
  openThemeDesigner: () => void;
  /** Open the inline About Tuxlink dialog (tuxlink-35g0). */
  openAbout: () => void;
  /** Open the inline Help / Documentation panel (tuxlink-35g0). */
  openHelp: () => void;
  /** Open the project's GitHub issue tracker in the operator's default
   *  browser (tuxlink-35g0). */
  reportIssue: () => void;
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
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
    case 'menu:view:radio_panel': h.toggleRadioPanel(); return;
    // tuxlink-39b: the consolidated GPS & Privacy settings item opens the inline
    // Settings panel (previously a cluster of dead no-op stubs found in the
    // post-merge smoke of #113).
    case 'menu:tools:settings_privacy':
      h.openSettings(); return;
    // tuxlink-vgth: opens the inline Theme Designer panel.
    case 'menu:view:customize_theme':
      h.openThemeDesigner(); return;
    // tuxlink-35g0: Help menu wiring. About + Documentation are inline
    // overlays; Report Issue opens the project's issue tracker in the
    // operator's default browser via @tauri-apps/plugin-shell.
    case 'menu:help:about':
      h.openAbout(); return;
    case 'menu:help:docs':
      h.openHelp(); return;
    case 'menu:help:report_issue':
      h.reportIssue(); return;
    case 'menu:mailbox:inbox':
    case 'menu:mailbox:sent':
    case 'menu:mailbox:outbox':
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
