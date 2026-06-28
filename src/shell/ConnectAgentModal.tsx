/**
 * ConnectAgentModal — show per-agent MCP copy-paste connect commands (tuxlink-l9sq4).
 *
 * Show-and-copy only. Tuxlink does not write agent config files.
 *
 * Chrome mirrors ReportIssueModal (backdrop, panel, Esc + backdrop close).
 */

import { useEffect } from 'react';
import { buildAgentCommands } from './connectAgentCommands';
import { useMcpConnectionInfo } from './useMcpConnectionInfo';
import './ConnectAgentModal.css';

export interface ConnectAgentModalProps {
  open: boolean;
  onClose: () => void;
}

export function ConnectAgentModal({ open, onClose }: ConnectAgentModalProps): JSX.Element | null {
  // Esc closes — mirrors ReportIssueModal / SettingsPanel / AboutDialog pattern.
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const { data } = useMcpConnectionInfo(open);

  if (!open) return null;

  const commands = data ? buildAgentCommands(data) : [];

  async function copyCommand(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard may be unavailable in some sandbox configurations */
    }
  }

  return (
    <div
      className="tux-about-backdrop"
      data-testid="connect-agent-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-about-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Connect an AI Agent"
        data-testid="connect-agent-panel"
        onClick={(e) => e.stopPropagation()}
        style={{ width: 'min(560px, calc(100vw - 48px))' }}
      >
        {/* Header */}
        <div className="tux-about-header">
          <h2 className="tux-about-title">Connect an AI Agent</h2>
          <button
            type="button"
            className="tux-about-close"
            data-testid="connect-agent-close"
            aria-label="Close Connect an AI Agent dialog"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        {/* Body */}
        <div className="tux-about-body">
          <p style={{ margin: '0 0 16px', fontSize: 13, lineHeight: 1.5 }}>
            Point an AI assistant at this station. Pick your agent, copy the command, run it once.
          </p>

          {data && data.serverRunning === false && (
            <p className="tux-connect-agent-server-warn">
              {"Tuxlink's MCP server starts automatically with the app."}
            </p>
          )}

          {commands.map((cmd) => (
            <div
              key={cmd.id}
              className="tux-connect-agent-section"
              data-testid={`connect-agent-${cmd.id}`}
            >
              <p className="tux-connect-agent-label">{cmd.label}</p>
              <div className="tux-connect-agent-command-row">
                <pre className="tux-connect-agent-pre">{cmd.command}</pre>
                <button
                  type="button"
                  className="tux-connect-agent-copy-btn"
                  aria-label={`Copy ${cmd.label} command`}
                  onClick={() => void copyCommand(cmd.command)}
                >
                  Copy
                </button>
              </div>
            </div>
          ))}

          <p className="tux-connect-agent-security-note">
            {
              'This lets the agent read and diagnose your station. Transmitting or changing settings still needs you to arm "Agent send" on the dashboard.'
            }
          </p>
        </div>

        {/* Actions */}
        <div className="tux-about-actions">
          <button
            type="button"
            className="tux-about-button"
            data-testid="connect-agent-close-btn"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
