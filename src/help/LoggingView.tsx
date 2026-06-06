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
import './LoggingView.css';

export function LoggingView() {
  return (
    <div className="logging-view" data-testid="logging-view-root">
      <header className="logging-view-header">
        <h1>Logging</h1>
      </header>
      <main>
        <LoggingExportSection />
        <LoggingSettingsSection />
        <LoggingProbesSection />
      </main>
    </div>
  );
}
