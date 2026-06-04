/// <reference types="vite/client" />

// Build-time constant injected by vite.config.ts's `define` block. Read from
// version.txt (release-please's canonical bump target).
declare const __APP_VERSION__: string;

// marked-extended-tables v2.0.1 ships no .d.ts and no `"types"` field.
declare module 'marked-extended-tables';
