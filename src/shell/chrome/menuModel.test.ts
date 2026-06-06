import { describe, it, expect } from 'vitest';
import { MENU_ACTION_IDS, ACCELERATORS } from './menuModel';

// Parity with the former Rust menu_event_ids() (menu.rs) — the menu:* vocabulary
// is the stable contract regardless of producer. Order matches the menu layout.
const EXPECTED_IDS = [
  'menu:file:quit',
  'menu:message:new', 'menu:message:reply', 'menu:message:reply_all', 'menu:message:forward', 'menu:message:archive', 'menu:message:catalog_request', 'menu:message:grib_request', 'menu:message:print',
  'menu:session:connect', 'menu:session:disconnect', 'menu:session:log',
  'menu:session:verify_cms', 'menu:session:show_transport',
  'menu:mailbox:inbox', 'menu:mailbox:sent', 'menu:mailbox:outbox', 'menu:mailbox:archive',
  'menu:view:status_bar', 'menu:view:radio_panel',
  'menu:view:scheme:default',
  'menu:view:scheme:daylight',
  'menu:view:scheme:high-contrast-light',
  'menu:view:scheme:paper',
  'menu:view:scheme:night-red',
  'menu:view:scheme:grayscale',
  'menu:view:scheme:custom',
  'menu:view:customize_theme',
  'menu:tools:templates', 'menu:tools:rig_control',
  'menu:tools:settings_connection', 'menu:tools:settings_privacy',
  'menu:help:about', 'menu:help:docs', 'menu:help:logging', 'menu:help:report_issue',
];

describe('menu model', () => {
  it('exposes exactly the menu:* action vocabulary', () => {
    expect(MENU_ACTION_IDS).toEqual(EXPECTED_IDS);
  });

  it('every accelerator maps to a real action id', () => {
    for (const a of ACCELERATORS) {
      expect(MENU_ACTION_IDS).toContain(a.id);
    }
  });

  it('binds F5 and Ctrl+Shift+O to connect', () => {
    const connectAccels = ACCELERATORS.filter((a) => a.id === 'menu:session:connect');
    expect(connectAccels.map((a) => a.combo).sort()).toEqual(['Ctrl+Shift+O', 'F5']);
  });
});
