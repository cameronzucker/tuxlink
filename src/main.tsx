import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyColorScheme, loadColorScheme } from "./shell/colorScheme";

// Apply the persisted color scheme before React mounts so there's no flash of
// the default theme on launch (tuxlink-8za).
applyColorScheme(loadColorScheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
