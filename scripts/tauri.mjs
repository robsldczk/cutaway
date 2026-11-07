#!/usr/bin/env node
// `npm run tauri ...` in app/ comes here. `tauri dev` gets its own data folder
// (`<data dir>/<slug>-dev`), so a dev build can't change the real covers, show indexes,
// review packages or reviewer name. Set CUTAWAY_DATA_DIR to choose another folder,
// including the real one. Every other tauri command passes through unchanged.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const brand = JSON.parse(readFileSync(join(root, 'brand.json'), 'utf8'));
const args = process.argv.slice(2);
const env = { ...process.env };
const key = `${brand.slug.toUpperCase()}_DATA_DIR`;

// Same base folder as the `dirs::data_dir()` that crates/apc-sound/src/show_library.rs uses.
function dataDir() {
  if (process.platform === 'darwin') return join(homedir(), 'Library', 'Application Support');
  if (process.platform === 'win32') return env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
  return env.XDG_DATA_HOME ?? join(homedir(), '.local', 'share');
}

if (args[0] === 'dev' && !env[key]) {
  env[key] = join(dataDir(), `${brand.slug}-dev`);
  console.log(`tauri dev: dev data in ${env[key]} (set ${key} to change it)`);
}

const result = spawnSync('tauri', args, { stdio: 'inherit', env, shell: process.platform === 'win32' });
if (result.error) {
  console.error(`could not start tauri: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
