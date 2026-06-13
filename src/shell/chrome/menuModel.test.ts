import { describe, it, expect } from 'vitest';
import { MENU_ACTION_IDS, ACCELERATORS } from './menuModel';

// Parity with the former Rust menu_event_ids() (menu.rs) — the menu:* vocabulary
// is the stable contract regardless of producer. Order matches the menu layout.
const EXPECTED_IDS = [
  'menu:message:print', 'menu:file:quit',
  'menu:message:new', 'menu:message:reply', 'menu:message:reply_all', 'menu:message:forward', 'menu:message:archive', 'menu:message:request_center', 'menu:message:grib_request',
  // tuxlink-lqw2: the Session + Mailbox top menus were removed in the pre-Alpha
  // declutter; the surviving Verify CMS Connection moved into Tools (below).
  'menu:view:status_bar', 'menu:view:radio_panel',
  'menu:view:scheme:default',
  'menu:view:scheme:github-dark',
  'menu:view:scheme:office-dark',
  'menu:view:scheme:daylight',
  'menu:view:scheme:high-contrast-light',
  'menu:view:scheme:paper',
  'menu:view:scheme:night-red',
  'menu:view:scheme:grayscale',
  'menu:view:scheme:custom',
  'menu:view:customize_theme',
  'menu:tools:find_gateway', 'menu:tools:verify_cms', 'menu:tools:templates',
  'menu:tools:settings_privacy',
  'menu:help:about', 'menu:help:docs', 'menu:help:logging', 'menu:help:report_issue', 'menu:help:uninstall_cleanup',
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

  // tuxlink-lqw2: the Connect menu item + its F5 / Ctrl+Shift+O accelerators
  // were removed in the pre-Alpha declutter (connect lives on the ribbon).
  it('no longer binds any accelerator to connect', () => {
    expect(ACCELERATORS.some((a) => a.id === 'menu:session:connect')).toBe(false);
    expect(ACCELERATORS.some((a) => a.combo === 'F5')).toBe(false);
  });
});
