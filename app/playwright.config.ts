import { defineConfig } from '@playwright/test';

// UI smoke tests: the Svelte UI in Chromium against the HTTP bridge (the same service the
// Tauri shell uses), on a freshly generated demo session.
export default defineConfig({
  testDir: 'tests',
  // one demo session serves every test: run them one after another
  workers: 1,
  timeout: 60_000,
  use: {
    baseURL: 'http://127.0.0.1:1421',
    viewport: { width: 1440, height: 900 },
    // a first-use tour would sit over every spec: tour.spec.ts takes this away
    storageState: {
      cookies: [],
      origins: [{ origin: 'http://127.0.0.1:1421', localStorage: [{ name: 'cutaway.tour', value: JSON.stringify(['options', 'arrange', 'edit', 'export', 'review']) }] }],
    },
  },
  webServer: [
    {
      command: 'node tests/epidemic-standin.mjs',
      url: 'http://127.0.0.1:7359/health',
      reuseExistingServer: false,
    },
    {
      command: 'bash tests/serve-demo.sh',
      url: 'http://127.0.0.1:7358/health',
      timeout: 600_000,
      reuseExistingServer: false,
    },
    {
      command: 'npx vite --host 127.0.0.1 --port 1421 --strictPort',
      url: 'http://127.0.0.1:1421',
      env: { VITE_BRIDGE_URL: 'http://127.0.0.1:7358' },
      reuseExistingServer: false,
    },
  ],
});
