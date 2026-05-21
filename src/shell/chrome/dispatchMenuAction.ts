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
  toggleSessionLog: () => void;
  toggleStatusBar: () => void;
  selectFolder: (folder: MailboxFolder) => void;
  setScheme: (id: ColorScheme) => void;
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
    case 'menu:file:new': h.openCompose(); return;
    case 'menu:file:quit': h.quit(); return;
    case 'menu:session:connect': h.connect(); return;
    case 'menu:message:reply': h.reply(); return;
    case 'menu:message:reply_all': h.replyAll(); return;
    case 'menu:message:forward': h.forward(); return;
    case 'menu:view:session_log': h.toggleSessionLog(); return;
    case 'menu:view:status_bar': h.toggleStatusBar(); return;
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
