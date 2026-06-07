/**
 * LoggingView — root component mounted at /logging in a separate Tauri webview
 * window (label "logging"). Three vertical sections: Export, Settings,
 * Environment probes. Flat layout (no tabs, no cards) per spec §8.2.
 *
 * tuxlink-qjgx alpha-logging plan Task 7.
 */
import { LoggingExportSection } from './LoggingExportSection';
import { LoggingSettingsSection } from './LoggingSettingsSection';
import { LoggingProbesSection } from './LoggingProbesSection';
import { LoggingTitleBar } from './LoggingTitleBar';
import { ResizeHandles } from '../shell/chrome/ResizeHandles';
import { useHelpTheme } from './useHelpTheme';
import './LoggingView.css';

export function LoggingView() {
  useHelpTheme();
  return (
    <div className="logging-view" data-testid="logging-view-root">
      <ResizeHandles />
      <LoggingTitleBar />
      <header className="logging-view-header">
        <h1 className="logging-view-title">Logging</h1>
      </header>
      <main className="logging-view-main">
        <LoggingExportSection />
        <LoggingSettingsSection />
        <LoggingProbesSection />
      </main>
    </div>
  );
}
