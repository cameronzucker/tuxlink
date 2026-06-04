import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Mirror the vite.config.ts `define` so tests see the same APP_VERSION the
// production bundle sees. release-please maintains version.txt.
const APP_VERSION = readFileSync(resolve(__dirname, 'version.txt'), 'utf8').trim();

export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(APP_VERSION),
  },
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.{test,spec}.{ts,tsx}', 'scripts/**/*.{test,spec}.ts'],
    // Vitest defaults `css: false`, which makes ALL CSS imports return ''
    // — including `?raw` queries through Vite's CSS plugin. Opt CSS files
    // imported as raw text back in so tests can assert against the actual
    // CSS source (e.g. AppShell.test.tsx tuxlink-8rng chrome-width pins).
    // Pattern: import.meta.glob('./X.css', { eager: true, query: '?raw',
    // import: 'default' }) — the Vite-native swap for the node:fs
    // readFileSync that pitfall TEST-1 (docs/pitfalls/implementation-
    // pitfalls.md) forbids.
    css: { include: [/\.css\?raw$/] },
  },
});
