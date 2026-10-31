#!/usr/bin/env node
// Syncs files that cannot import brand.json. `--check` only reports drift (used by check.sh).
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const checkOnly = process.argv.includes('--check');
const brand = JSON.parse(readFileSync(join(root, 'brand.json'), 'utf8'));
const drift = [];

function syncText(rel, transform) {
  const path = join(root, rel);
  const before = readFileSync(path, 'utf8');
  const after = transform(before);
  if (after !== before) {
    drift.push(rel);
    if (!checkOnly) writeFileSync(path, after);
  }
}

syncText('app/src-tauri/tauri.conf.json', (text) => {
  const conf = JSON.parse(text);
  conf.productName = brand.displayName;
  conf.mainBinaryName = brand.displayName;
  conf.identifier = brand.bundleId;
  for (const w of conf.app.windows) w.title = brand.displayName;
  conf.bundle.fileAssociations = [
    { ext: [brand.episodeExt], name: `${brand.displayName} Episode`, role: 'Editor' },
    { ext: [brand.showExt], name: `${brand.displayName} Show`, role: 'Editor' },
  ];
  return JSON.stringify(conf, null, 2) + '\n';
});

syncText('crates/apc-cli/Cargo.toml', (text) =>
  text.replace(/(\[\[bin\]\]\nname = ")[^"]*(")/, `$1${brand.cliName}$2`),
);

syncText('app/index.html', (text) => text.replace(/<title>[^<]*<\/title>/, `<title>${brand.displayName}</title>`));

if (drift.length === 0) {
  console.log(`brand: in sync (${brand.displayName})`);
} else if (checkOnly) {
  console.error(`brand: out of sync with brand.json: ${drift.join(', ')}. Run: node scripts/rebrand.mjs`);
  process.exit(1);
} else {
  console.log(`brand: updated ${drift.join(', ')}`);
}
