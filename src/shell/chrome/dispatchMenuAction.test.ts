import { describe, it, expect, vi } from 'vitest';
import { dispatchMenuAction, type MenuHandlers } from './dispatchMenuAction';

function handlers(): MenuHandlers {
  return {
    openCompose: vi.fn(),
    connect: vi.fn(),
    reply: vi.fn(),
    replyAll: vi.fn(),
    forward: vi.fn(),
    toggleStatusBar: vi.fn(),
    toggleRadioPanel: vi.fn(),
    selectFolder: vi.fn(),
    setScheme: vi.fn(),
    openSettings: vi.fn(),
    openThemeDesigner: vi.fn(),
    openAbout: vi.fn(),
    openHelp: vi.fn(),
    reportIssue: vi.fn(),
    quit: vi.fn(),
  };
}

describe('dispatchMenuAction', () => {
  it('routes message:new to openCompose', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:new', h);
    expect(h.openCompose).toHaveBeenCalledOnce();
  });

  it('routes file:quit to quit', () => {
    const h = handlers();
    dispatchMenuAction('menu:file:quit', h);
    expect(h.quit).toHaveBeenCalledOnce();
  });

  it('routes session:connect to connect', () => {
    const h = handlers();
    dispatchMenuAction('menu:session:connect', h);
    expect(h.connect).toHaveBeenCalledOnce();
  });

  it('routes view toggles', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:status_bar', h);
    expect(h.toggleStatusBar).toHaveBeenCalledOnce();
  });

  // tuxlink-mnk4: the radio-panel menu item + Ctrl+Shift+M accelerator must
  // route through the dispatcher (radio-panel-shell P1.7 renamed it from
  // radio_dock / toggleRadioDock).
  it('routes view:radio_panel to toggleRadioPanel', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:radio_panel', h);
    expect(h.toggleRadioPanel).toHaveBeenCalledOnce();
  });

  it('routes mailbox folder selection with the folder name', () => {
    const h = handlers();
    dispatchMenuAction('menu:mailbox:sent', h);
    expect(h.selectFolder).toHaveBeenCalledWith('sent');
  });

  it('routes scheme selection with the scheme id', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:scheme:night-red', h);
    expect(h.setScheme).toHaveBeenCalledWith('night-red');
  });

  it('routes the new light presets', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:scheme:daylight', h);
    dispatchMenuAction('menu:view:scheme:high-contrast-light', h);
    dispatchMenuAction('menu:view:scheme:paper', h);
    expect(h.setScheme).toHaveBeenNthCalledWith(1, 'daylight');
    expect(h.setScheme).toHaveBeenNthCalledWith(2, 'high-contrast-light');
    expect(h.setScheme).toHaveBeenNthCalledWith(3, 'paper');
  });

  it('routes the "custom" scheme sentinel', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:scheme:custom', h);
    expect(h.setScheme).toHaveBeenCalledWith('custom');
  });

  it('routes Customize… to openThemeDesigner', () => {
    const h = handlers();
    dispatchMenuAction('menu:view:customize_theme', h);
    expect(h.openThemeDesigner).toHaveBeenCalledOnce();
  });

  it('routes reply / reply_all / forward', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:reply', h);
    dispatchMenuAction('menu:message:reply_all', h);
    dispatchMenuAction('menu:message:forward', h);
    expect(h.reply).toHaveBeenCalledOnce();
    expect(h.replyAll).toHaveBeenCalledOnce();
    expect(h.forward).toHaveBeenCalledOnce();
  });

  it('is a safe no-op for stub/unhandled ids', () => {
    const h = handlers();
    expect(() => dispatchMenuAction('menu:tools:preferences', h)).not.toThrow();
    expect(() => dispatchMenuAction('menu:session:disconnect', h)).not.toThrow();
  });

  // tuxlink-35g0: the Help menu is now wired — About / Documentation /
  // Report Issue each route to a dedicated handler.
  it('routes Help → About Tuxlink to openAbout', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:about', h);
    expect(h.openAbout).toHaveBeenCalledOnce();
  });

  it('routes Help → Documentation to openHelp', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:docs', h);
    expect(h.openHelp).toHaveBeenCalledOnce();
  });

  it('routes Help → Report Issue to reportIssue', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:report_issue', h);
    expect(h.reportIssue).toHaveBeenCalledOnce();
  });

  // tuxlink-39b: the consolidated GPS & Privacy settings item opens the panel
  // (previously a cluster of dead no-op stubs).
  it('routes the GPS & Privacy settings item to openSettings', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:settings_privacy', h);
    expect(h.openSettings).toHaveBeenCalledOnce();
  });
});
