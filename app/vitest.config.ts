import { defineConfig } from 'vitest/config';

// Unit tests of the pure logic: no browser, no Svelte.
export default defineConfig({
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});
