import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyColorScheme, loadColorScheme } from "./shell/colorScheme";

// Apply the persisted color scheme before React mounts. tuxlink-k0q3 added an
// inline boot script in index.html that does this synchronously in the HTML
// parse phase for an earlier paint — but packaged Tauri's CSP
// (`default-src 'self'; …; style-src 'self' 'unsafe-inline'`, no `script-src`
// override / no nonce / no hash) blocks inline `<script>` under release, so
// the inline path only fires in dev. This bundle-side call is the
// production-correctness fallback (tuxlink-01vd): under packaged release the
// saved scheme applies the moment this module evaluates. Idempotent with the
// inline script when both run.
applyColorScheme(loadColorScheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
