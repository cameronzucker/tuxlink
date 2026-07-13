import { describe, it, expect, vi } from 'vitest';
import { dispatchMenuAction, type MenuHandlers } from './dispatchMenuAction';

function handlers(): MenuHandlers {
  return {
    openCompose: vi.fn(),
    reply: vi.fn(),
    replyAll: vi.fn(),
    forward: vi.fn(),
    archive: vi.fn(),
    delete: vi.fn(),
    print: vi.fn(),
    toggleStatusBar: vi.fn(),
    toggleRadioPanel: vi.fn(),
    verifyCms: vi.fn(),
    setScheme: vi.fn(),
    openSettings: vi.fn(),
    openThemeDesigner: vi.fn(),
    openAbout: vi.fn(),
    openHelp: vi.fn(),
    replayTour: vi.fn(),
    openLogging: vi.fn(),
    reportIssue: vi.fn(),
    openUninstallCleanup: vi.fn(),
    openConnectAgent: vi.fn(),
    openElmer: vi.fn(),
    openElmerModel: vi.fn(),
    openCatalogBuilder: vi.fn(),
    openRequestCenter: vi.fn(),
    quit: vi.fn(),
  };
}

describe('dispatchMenuAction', () => {
  it('routes message:new to openCompose', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:new', h);
    expect(h.openCompose).toHaveBeenCalledOnce();
  });

  it('routes tools:find_gateway to openCatalogBuilder (tuxlink-6jpf)', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:find_gateway', h);
    expect(h.openCatalogBuilder).toHaveBeenCalledOnce();
  });

  // tuxlink-lqw2: Verify CMS Connection relocated from the (removed) Session
  // menu into Tools, now wired to the inline probe overlay.
  it('routes tools:verify_cms to verifyCms', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:verify_cms', h);
    expect(h.verifyCms).toHaveBeenCalledOnce();
  });

  it('routes file:quit to quit', () => {
    const h = handlers();
    dispatchMenuAction('menu:file:quit', h);
    expect(h.quit).toHaveBeenCalledOnce();
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

  // tuxlink-ca5x: Archive (Message menu) moves the open message to Archive.
  // tuxlink-lqw2: the Mailbox menu was removed (folder nav lives in the
  // FolderSidebar), so the message:archive action is the only Archive route.
  it('routes message:archive to archive', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:archive', h);
    expect(h.archive).toHaveBeenCalledOnce();
  });

  // tuxlink-wl7n: Message → Delete moves the open message to Trash. Regression
  // pin for the final-review finding where the menu item + accelerator were
  // registered in menuModel but had no `dispatchMenuAction` case (silent no-op).
  it('routes message:delete to delete', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:delete', h);
    expect(h.delete).toHaveBeenCalledOnce();
  });

  // tuxlink-j0m3: Print fires the webview's native print dialog via the
  // open-message-gated handler in AppShell. The dispatcher's job is just
  // to route the id — open-message gating happens inside the handler.
  it('routes message:print to print', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:print', h);
    expect(h.print).toHaveBeenCalledOnce();
  });

  it('is a safe no-op for unknown/removed ids', () => {
    const h = handlers();
    // Ids with no case (removed or never-wired) no-op rather than throw.
    expect(() => dispatchMenuAction('menu:tools:templates', h)).not.toThrow();
    expect(() => dispatchMenuAction('menu:tools:preferences', h)).not.toThrow();
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

  // tuxlink-10bkw Task 6: Help → Replay tour restarts the guided tour.
  it('routes Help → Replay tour to replayTour', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:replay_tour', h);
    expect(h.replayTour).toHaveBeenCalledOnce();
  });

  it('routes Help → Report Issue to reportIssue', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:report_issue', h);
    expect(h.reportIssue).toHaveBeenCalledOnce();
  });

  // tuxlink-qjgx Task 8: Logging window + Report Issue flow menu wiring.
  it('routes Help → Logging to openLogging', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:logging', h);
    expect(h.openLogging).toHaveBeenCalledOnce();
  });

  it('routes Help → Report Issue… to reportIssue (modal flow)', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:report_issue', h);
    expect(h.reportIssue).toHaveBeenCalledOnce();
  });

  it('routes Help → Uninstall Cleanup to openUninstallCleanup', () => {
    const h = handlers();
    dispatchMenuAction('menu:help:uninstall_cleanup', h);
    expect(h.openUninstallCleanup).toHaveBeenCalledOnce();
  });

  // tuxlink-l9sq4: Tools → Connect an AI agent opens the ConnectAgentModal.
  it('routes tools:connect_agent to openConnectAgent', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:connect_agent', h);
    expect(h.openConnectAgent).toHaveBeenCalledOnce();
  });

  // tuxlink-esb65: the single honest "Settings…" item opens the multi-section
  // Settings panel. Replaces the former settings_privacy + settings_account
  // leaves that both opened this same panel.
  it('routes the Settings item to openSettings', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:settings', h);
    expect(h.openSettings).toHaveBeenCalledOnce();
  });

  // tuxlink-eymu: the Request Center replaces the standalone Catalog Request
  // panel in the menu and absorbs the GRIB request as an inner view.
  it('routes Message → Request Center to openRequestCenter (home view)', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:request_center', h);
    expect(h.openRequestCenter).toHaveBeenCalledOnce();
    // Home route passes NO initialView — a stray arg would land on the wrong
    // inner view (openRequestCenter is arg-overloaded; 'grib' is a valid one).
    expect(h.openRequestCenter).toHaveBeenCalledWith();
  });

  // tuxlink-eymu: GRIB File Request now opens the Request Center directly on
  // its GRIB view (the standalone GribRequestPanel is removed in F1).
  it('routes Message → GRIB File Request to openRequestCenter with the grib view', () => {
    const h = handlers();
    dispatchMenuAction('menu:message:grib_request', h);
    expect(h.openRequestCenter).toHaveBeenCalledWith('grib');
  });

  // tuxlink-1wi5w: Tools → Set up Elmer's model… opens the Elmer pane with the
  // Model section expanded. connect_agent → openConnectAgent is UNCHANGED.
  it('routes tools:elmer_model to openElmerModel', () => {
    const h = handlers();
    dispatchMenuAction('menu:tools:elmer_model', h);
    expect(h.openElmerModel).toHaveBeenCalledOnce();
    // Verify connect_agent is still untouched by this action.
    expect(h.openConnectAgent).not.toHaveBeenCalled();
  });
});
