import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// Theme application moved into index.html's inline <head> script
// (tuxlink-perf-coldstart) so it runs in the HTML parse phase instead of
// after the React bundle evaluates. applyColorScheme remains the runtime
// path for View → Color Scheme menu flips.

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
