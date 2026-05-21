import { describe, it, expect, vi } from 'vitest';
import { dispatchMenuAction, type MenuHandlers } from './dispatchMenuAction';

function handlers(): MenuHandlers {
  return {
    openCompose: vi.fn(),
    connect: vi.fn(),
    reply: vi.fn(),
    replyAll: vi.fn(),
    forward: vi.fn(),
    toggleSessionLog: vi.fn(),
    toggleStatusBar: vi.fn(),
    selectFolder: vi.fn(),
    setScheme: vi.fn(),
    quit: vi.fn(),
  };
}

describe('dispatchMenuAction', () => {
  it('routes file:new to openCompose', () => {
    const h = handlers();
    dispatchMenuAction('menu:file:new', h);
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
    dispatchMenuAction('menu:view:session_log', h);
    dispatchMenuAction('menu:view:status_bar', h);
    expect(h.toggleSessionLog).toHaveBeenCalledOnce();
    expect(h.toggleStatusBar).toHaveBeenCalledOnce();
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
    expect(() => dispatchMenuAction('menu:help:about', h)).not.toThrow();
  });
});
