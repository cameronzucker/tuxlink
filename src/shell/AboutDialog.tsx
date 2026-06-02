/**
 * AboutDialog — inline (in-webview) About Tuxlink dialog (tuxlink-35g0).
 *
 * Opened from Help → About Tuxlink. Shows the product name, version, build,
 * license, and links to the repository / issue tracker / license. The
 * version comes from __APP_VERSION__ (vite.config.ts injects from
 * version.txt — release-please's canonical bump target).
 *
 * NOT a separate OS window — inline overlay per feedback_inline_ui_no_window_clutter.
 *
 * Outbound links use `@tauri-apps/plugin-shell::open` (already a project dep
 * for the wizard's "Register" link + the ARDOP panel's external resources)
 * so clicks open in the operator's default browser rather than navigating
 * the in-app webview (which would lose application state).
 */

import { useEffect } from 'react';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import './AboutDialog.css';

const APP_VERSION = `v${__APP_VERSION__}`;
const REPO_URL = 'https://github.com/cameronzucker/tuxlink';
const ISSUES_URL = `${REPO_URL}/issues`;
const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;
const CHANGELOG_URL = `${REPO_URL}/blob/main/CHANGELOG.md`;

export interface AboutDialogProps {
  open: boolean;
  onClose: () => void;
}

export function AboutDialog({ open, onClose }: AboutDialogProps) {
  // Esc closes (matches SettingsPanel + ThemeDesigner).
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  function openLink(url: string) {
    return (e: React.MouseEvent) => {
      e.preventDefault();
      void shellOpen(url).catch(() => {
        /* the link rendering itself is harmless if shell-open fails;
         * we could surface a toast but the operator clearly meant to
         * open the URL — they'll notice the no-op. */
      });
    };
  }

  return (
    <div
      className="tux-about-backdrop"
      data-testid="about-backdrop"
      onClick={onClose}
    >
      <div
        className="tux-about-panel"
        role="dialog"
        aria-modal="true"
        aria-label="About Tuxlink"
        data-testid="about-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="tux-about-header">
          <h2 className="tux-about-title">About Tuxlink</h2>
          <button
            type="button"
            className="tux-about-close"
            data-testid="about-close"
            aria-label="Close About dialog"
            onClick={onClose}
          >
            ×
          </button>
        </div>

        <div className="tux-about-body">
          <div className="tux-about-product">
            <div className="tux-about-product-name">Tuxlink</div>
            <div className="tux-about-product-version" data-testid="about-version">
              {APP_VERSION}
            </div>
          </div>

          <p className="tux-about-tagline">
            A native Linux desktop Winlink client for amateur-radio
            emergency communications.
          </p>

          <p className="tux-about-prealpha" role="note">
            <strong>Pre-alpha.</strong> Version tags are produced automatically
            by release-please from conventional-commit activity — they reflect
            repository velocity, not release readiness. Do not rely on this
            build for live emergency communications.
          </p>

          <dl className="tux-about-meta">
            <dt>License</dt>
            <dd>
              <a
                href={LICENSE_URL}
                onClick={openLink(LICENSE_URL)}
                data-testid="about-license-link"
              >
                MIT
              </a>
            </dd>
            <dt>Source</dt>
            <dd>
              <a
                href={REPO_URL}
                onClick={openLink(REPO_URL)}
                data-testid="about-repo-link"
              >
                github.com/cameronzucker/tuxlink
              </a>
            </dd>
            <dt>Changelog</dt>
            <dd>
              <a
                href={CHANGELOG_URL}
                onClick={openLink(CHANGELOG_URL)}
                data-testid="about-changelog-link"
              >
                CHANGELOG.md
              </a>
            </dd>
            <dt>Report an issue</dt>
            <dd>
              <a
                href={ISSUES_URL}
                onClick={openLink(ISSUES_URL)}
                data-testid="about-issues-link"
              >
                {ISSUES_URL.replace('https://', '')}
              </a>
            </dd>
          </dl>

          <p className="tux-about-credit">
            Built with Rust, Tauri, and React. © Cameron Zucker; released
            under the MIT License.
          </p>
        </div>

        <div className="tux-about-actions">
          <button
            type="button"
            className="tux-about-button"
            data-testid="about-ok"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
