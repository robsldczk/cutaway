#!/usr/bin/env node
// The third-party license list (D156, D161).
//
//   node scripts/licenses.mjs           writes the files
//   node scripts/licenses.mjs --check   fails when they are stale or a license needs a decision
//
// Writes two files from Cargo.lock and app/package-lock.json:
// - app/src/legal/third-party-licenses.json: what Settings > Licenses shows, license texts and all.
// - THIRD_PARTY_LICENSES.md: the same, readable in the repository, plus the development tools.
//
// What the app ships: the Rust crates the desktop app and the `cutaway` binary depend on (normal
// dependencies, every platform), the npm packages that aren't development tools, and EXTRAS
// (work that isn't a package: the speech model and the libraries inside the prebuilt
// sherpa-onnx library). A shipped package's license must be in ALLOWED, or the check fails: a new
// kind of license is Rob's decision first. Every package, development tools included, is also
// checked for copyleft, as CLAUDE.md asks.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const MD = join(root, 'THIRD_PARTY_LICENSES.md');
const JSON_OUT = join(root, 'app/src/legal/third-party-licenses.json');
const STANDARD = join(root, 'scripts/licenses');
const checkOnly = process.argv.includes('--check');

/** The binaries people run: their dependencies are what ships. */
const SHIPPED_CRATES = ['cutaway-app', 'apc-cli'];

// Each entry needs a matching entry in docs/DECISIONS.md, as CLAUDE.md asks: `name` -> reason.
const ALLOWED_COPYLEFT = {};

/** Licenses Cutaway may ship (SPDX). None of them makes Cutaway's own code open source. */
const ALLOWED = new Set([
  'MIT', 'MIT-0', '0BSD', 'ISC', 'BSD-2-Clause', 'BSD-3-Clause', 'Zlib', 'Apache-2.0',
  'Apache-2.0 WITH LLVM-exception', 'Unlicense', 'CC0-1.0', 'BSL-1.0', 'Unicode-3.0',
  'Unicode-DFS-2016', 'OFL-1.1', 'bzip2-1.0.6',
  // file-level copyleft: changes to the package's own files would be published; used unchanged
  'MPL-2.0',
  // the speech model: credit to its authors, which this list gives
  'CC-BY-4.0',
  // the list of root certificates HTTPS checks against (webpki-roots): permissive, for data
  'CDLA-Permissive-2.0',
]);

/** Which license to take when a package offers a choice: the fewest obligations first. */
const PREFERRED = ['MIT', 'MIT-0', '0BSD', 'ISC', 'Unlicense', 'CC0-1.0', 'BSD-2-Clause', 'BSD-3-Clause', 'Zlib', 'BSL-1.0', 'Apache-2.0', 'Apache-2.0 WITH LLVM-exception', 'Unicode-3.0', 'Unicode-DFS-2016', 'OFL-1.1', 'bzip2-1.0.6', 'MPL-2.0', 'CDLA-Permissive-2.0', 'CC-BY-4.0'];

/** What each license's text says, to find it among a package's files. */
const SAYS = {
  MIT: /Permission is hereby granted, free of charge/i,
  'MIT-0': /Permission is hereby granted, free of charge/i,
  '0BSD': /Permission to use, copy, modify, and\/or distribute this software for any purpose with or without fee/i,
  ISC: /Permission to use, copy, modify, and(\/or)? distribute this software for any purpose/i,
  'BSD-2-Clause': /Redistribution and use in source and binary forms/i,
  'BSD-3-Clause': /Redistribution and use in source and binary forms/i,
  Zlib: /provided ['‘"]?as-is['’"]?, without any express or implied/i,
  'Apache-2.0': /Apache License,?\s+Version 2\.0/i,
  'Apache-2.0 WITH LLVM-exception': /Apache License,?\s+Version 2\.0/i,
  Unlicense: /This is free and unencumbered software released into the public domain/i,
  'CC0-1.0': /CC0 1\.0 Universal|Creative Commons Legal Code/i,
  'BSL-1.0': /Boost Software License/i,
  'Unicode-3.0': /UNICODE LICENSE V3|UNICODE,? INC\.? LICENSE AGREEMENT/i,
  'Unicode-DFS-2016': /UNICODE,? INC\.? LICENSE AGREEMENT/i,
  'OFL-1.1': /SIL OPEN FONT LICENSE/i,
  'bzip2-1.0.6': /bzip2.*Julian Seward|Julian R Seward/i,
  'MPL-2.0': /Mozilla Public License,? (Version|v\.) 2\.0/i,
  'CDLA-Permissive-2.0': /Community Data License Agreement - Permissive - Version 2\.0/i,
};

/** Work by others that isn't a Rust or npm package. Add to it when the app ships more. */
const EXTRAS = [
  {
    name: 'Parakeet TDT 0.6B v3 (int8, sherpa-onnx version)',
    version: 'v3',
    license: 'CC-BY-4.0',
    repository: 'https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3',
    authors: 'NVIDIA (model); k2-fsa (sherpa-onnx conversion)',
    note: 'The local speech recognition. Downloaded on first use from https://github.com/k2-fsa/sherpa-onnx/releases (asr-models), used as published, without changes. License: https://creativecommons.org/licenses/by/4.0/',
  },
  {
    name: 'ONNX Runtime',
    version: 'in sherpa-onnx 2.0.1',
    license: 'MIT',
    repository: 'https://github.com/microsoft/onnxruntime',
    authors: 'Microsoft Corporation',
    note: 'Inside the prebuilt sherpa-onnx library the speech recognition links.',
  },
  {
    name: 'Kaldi decoder, Kaldi native fbank, kaldifst',
    version: 'in sherpa-onnx 2.0.1',
    license: 'Apache-2.0',
    repository: 'https://github.com/k2-fsa',
    authors: 'Xiaomi Corporation and the k2-fsa authors',
    note: 'Inside the prebuilt sherpa-onnx library.',
  },
  {
    name: 'OpenFst',
    version: 'in sherpa-onnx 2.0.1',
    license: 'Apache-2.0',
    repository: 'https://www.openfst.org',
    authors: 'Google Inc. and the OpenFst authors',
    note: 'Inside the prebuilt sherpa-onnx library.',
  },
  {
    name: 'SentencePiece',
    version: 'in sherpa-onnx 2.0.1',
    license: 'Apache-2.0',
    repository: 'https://github.com/google/sentencepiece',
    authors: 'Google Inc.',
    note: 'Inside the prebuilt sherpa-onnx library.',
  },
  {
    name: 'KISS FFT',
    version: 'in sherpa-onnx 2.0.1',
    license: 'BSD-3-Clause',
    repository: 'https://github.com/mborgerding/kissfft',
    authors: 'Mark Borgerding',
    note: 'Inside the prebuilt sherpa-onnx library.',
  },
  {
    name: 'utf8proc',
    version: 'in sherpa-onnx 2.0.1',
    license: 'MIT',
    repository: 'https://github.com/JuliaStrings/utf8proc',
    authors: 'Steven G. Johnson, Jiahao Chen, Peter Colberg, Tony Kelman, Scott P. Jones, and other contributors; Public Software Group',
    note: 'Inside the prebuilt sherpa-onnx library.',
  },
  {
    name: 'PortAudio',
    version: 'in sherpa-onnx 2.0.1',
    license: 'MIT',
    repository: 'https://www.portaudio.com',
    authors: 'Ross Bencina, Phil Burk',
    note: 'Inside the prebuilt sherpa-onnx library. The PortAudio license is the MIT license with a request (not a condition) to send changes back.',
  },
];

// ---- reading the parts lists ----

const tidy = (t) =>
  t
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((l) => l.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();

function person(p) {
  if (!p) return undefined;
  const name = typeof p === 'string' ? p.replace(/\s*[<(].*$/, '').trim() : p.name;
  return name || undefined;
}

function repoUrl(r) {
  const url = typeof r === 'string' ? r : r?.url;
  if (!url) return undefined;
  return url
    .replace(/^git\+/, '')
    .replace(/^git:\/\//, 'https://')
    .replace(/^ssh:\/\/git@/, 'https://')
    .replace(/^github:/, 'https://github.com/')
    .replace(/\.git$/, '')
    .replace(/^([\w-]+\/[\w.-]+)$/, 'https://github.com/$1');
}

function rust() {
  const meta = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 }),
  );
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const shipped = new Set();
  const stack = meta.packages.filter((p) => SHIPPED_CRATES.includes(p.name) && p.source === null).map((p) => p.id);
  while (stack.length) {
    const id = stack.pop();
    if (shipped.has(id)) continue;
    shipped.add(id);
    for (const d of nodes.get(id)?.deps ?? []) if (d.dep_kinds.some((k) => k.kind === null)) stack.push(d.pkg);
  }
  return meta.packages
    .filter((p) => p.source !== null)
    .map((p) => ({
      kind: 'rust',
      name: p.name,
      version: p.version,
      declared: p.license ?? '',
      repository: p.repository ?? p.homepage ?? undefined,
      authors: (p.authors ?? []).map(person).filter(Boolean).join(', ') || undefined,
      dir: dirname(p.manifest_path),
      shipped: shipped.has(p.id),
    }));
}

function npm() {
  const app = join(root, 'app');
  const lock = JSON.parse(readFileSync(join(app, 'package-lock.json'), 'utf8'));
  return (
    Object.entries(lock.packages)
      // an optional peer isn't installed, and npm versions disagree on listing it
      .filter(([path, p]) => path !== '' && !(p.optional && p.peer))
      .map(([path, p]) => {
        const dir = join(app, path);
        let pkg = {};
        try {
          pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
        } catch {
          // not installed (another platform's optional binary): its lockfile entry is enough
        }
        return {
          kind: 'npm',
          name: path.slice(path.lastIndexOf('node_modules/') + 'node_modules/'.length),
          version: p.version,
          declared: p.license ?? '',
          repository: repoUrl(pkg.repository) ?? pkg.homepage,
          authors: person(pkg.author),
          dir,
          shipped: !p.dev && !p.devOptional,
        };
      })
  );
}

// ---- licenses ----

/** An SPDX expression as its choices: each choice is the licenses that apply together. */
export function choices(expr) {
  const tokens = expr
    .replace(/\s*\/\s*/g, ' OR ')
    .replace(/\(/g, ' ( ')
    .replace(/\)/g, ' ) ')
    .split(/\s+/)
    .filter(Boolean);
  let i = 0;
  const atom = () => {
    if (tokens[i] === '(') {
      i++;
      const inner = or();
      i++; // ')'
      return inner;
    }
    let id = tokens[i++] ?? '';
    if (tokens[i] === 'WITH') {
      id = `${id} WITH ${tokens[i + 1]}`;
      i += 2;
    }
    return [[id]];
  };
  const and = () => {
    let left = atom();
    while (tokens[i] === 'AND') {
      i++;
      const right = atom();
      left = left.flatMap((a) => right.map((b) => [...a, ...b]));
    }
    return left;
  };
  const or = () => {
    let out = and();
    while (tokens[i] === 'OR') {
      i++;
      out = [...out, ...and()];
    }
    return out;
  };
  return tokens.length ? or() : [];
}

const rank = (choice) => Math.max(...choice.map((id) => (PREFERRED.includes(id) ? PREFERRED.indexOf(id) : PREFERRED.length)));

function licenseFiles(dir) {
  if (!existsSync(dir)) return [];
  const out = [];
  const named = (n) => /^(licen[cs]e|copying|notice|copyright|unlicense|ofl)([-_.].*)?$/i.test(n);
  for (const n of readdirSync(dir).sort()) {
    const p = join(dir, n);
    const st = statSync(p);
    if (st.isFile() && named(n)) out.push({ name: n, text: readFileSync(p, 'utf8') });
    else if (st.isDirectory() && /^licen[cs]es$/i.test(n)) {
      for (const m of readdirSync(p).sort()) if (statSync(join(p, m)).isFile()) out.push({ name: `${n}/${m}`, text: readFileSync(join(p, m), 'utf8') });
    }
  }
  return out;
}

function standard(id) {
  const file = join(STANDARD, `${id.split(' WITH ')[0]}.txt`);
  return existsSync(file) ? readFileSync(file, 'utf8') : undefined;
}

const texts = {};
function keep(text) {
  const t = tidy(text);
  const key = createHash('sha256').update(t).digest('hex').slice(0, 12);
  texts[key] = t;
  return key;
}

const problems = [];

function licensed(p) {
  const allowed = choices(p.declared)
    .filter((c) => c.every((id) => ALLOWED.has(id)))
    .sort((a, b) => rank(a) - rank(b));
  if (!allowed.length) {
    problems.push(`${p.name} ${p.version} (${p.kind}): "${p.declared || 'no license stated'}" isn't in ALLOWED`);
    return null;
  }
  const files = licenseFiles(p.dir);
  const notice = (f) => /^notice/i.test(f.name);
  const says = (f, id) => SAYS[id]?.test(f.text.replace(/\s+/g, ' ')) ?? false;
  let chosen;
  let picked = [];
  for (const c of allowed) {
    picked = files.filter((f) => !notice(f) && c.some((id) => says(f, id)));
    if (picked.length) {
      chosen = c;
      break;
    }
  }
  let note;
  if (chosen) {
    // LICENSE-MIT beside LICENSE-APACHE: only the one taken
    if (picked.length > 1 && chosen.length === 1) {
      const word = chosen[0].split(/[- ]/)[0].toLowerCase();
      const byName = picked.filter((f) => f.name.toLowerCase().includes(word));
      if (byName.length) picked = byName;
    }
  } else {
    chosen = allowed.find((c) => c.every((id) => standard(id)));
    if (!chosen) {
      problems.push(`${p.name} ${p.version} (${p.kind}): no text for "${p.declared}" in the package, and none in scripts/licenses/`);
      return null;
    }
    picked = chosen.map((id) => ({ name: id, text: standard(id).replace('<copyright holders>', p.authors ?? `the ${p.name} authors`) }));
    note = 'The package names its license without its text: this is the standard text.';
  }
  // Apache-2.0 asks for a NOTICE file to be passed on
  if (chosen.some((id) => id.startsWith('Apache-2.0'))) picked.push(...files.filter(notice));
  if (chosen.includes('MPL-2.0')) {
    const src = p.kind === 'rust' ? `https://crates.io/crates/${p.name}/${p.version}` : `https://www.npmjs.com/package/${p.name}/v/${p.version}`;
    note = [note, `Used unchanged. Its source code is at ${src}.`].filter(Boolean).join(' ');
  }
  return {
    kind: p.kind,
    name: p.name,
    version: p.version,
    declared: p.declared,
    license: chosen.join(' AND '),
    ...(p.repository ? { repository: p.repository } : {}),
    ...(p.authors ? { authors: p.authors } : {}),
    ...(note ? { note } : {}),
    texts: [...new Set(picked.map((f) => keep(f.text)))],
  };
}

const uniq = (list) => [...new Map(list.map((p) => [`${p.kind}:${p.name}@${p.version}`, p])).values()];
const all = uniq([...rust(), ...npm()]);

for (const p of all) {
  if (!p.declared) problems.push(`${p.name} ${p.version}: no license in the lockfile or crate metadata`);
  else if (/\b(A?GPL|LGPL)/i.test(p.declared) && !/\bOR\b|\//i.test(p.declared) && !(p.name in ALLOWED_COPYLEFT)) {
    problems.push(`${p.name} ${p.version}: ${p.declared} needs a DECISIONS.md entry and ALLOWED_COPYLEFT`);
  }
}

const packages = all.filter((p) => p.shipped).map(licensed).filter(Boolean);
for (const x of EXTRAS) {
  const text = standard(x.license);
  if (!text) problems.push(`${x.name}: no standard text for ${x.license} in scripts/licenses/`);
  else packages.push({ kind: 'other', declared: x.license, ...x, texts: [keep(text.replace('<copyright holders>', x.authors))] });
}
const order = { other: 0, npm: 1, rust: 2 };
packages.sort((a, b) => order[a.kind] - order[b.kind] || a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en'));
const used = new Set(packages.flatMap((p) => p.texts));
const shownTexts = Object.fromEntries(Object.entries(texts).filter(([k]) => used.has(k)).sort(([a], [b]) => (a < b ? -1 : 1)));

const json = `${JSON.stringify({ packages, texts: shownTexts }, null, 1)}\n`;

const tools = all.filter((p) => !p.shipped).sort((a, b) => a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en'));
const number = new Map(Object.keys(shownTexts).map((k, i) => [k, i + 1]));
const groups = { other: 'Models and libraries that are not packages', npm: 'npm packages', rust: 'Rust crates' };
const lines = [
  '# Third-party licenses',
  '',
  'Generated by `scripts/licenses.mjs` from `Cargo.lock` and `app/package-lock.json`. Do not edit it. To update it, run `node scripts/licenses.mjs`.',
  '',
  'Cutaway is proprietary software (see LICENSE.md). It includes the following work by others, each used under the license named. The same list is in the app under Settings > Licenses.',
];
for (const kind of ['other', 'npm', 'rust']) {
  const g = packages.filter((p) => p.kind === kind);
  if (!g.length) continue;
  lines.push('', `## ${groups[kind]} (${g.length})`, '');
  for (const p of g) {
    const refs = p.texts.map((t) => `[text ${number.get(t)}](#text-${number.get(t)})`).join(', ');
    const bits = [`**${p.name}** ${p.version}`, p.license + (p.declared !== p.license ? ` (offered as ${p.declared})` : ''), refs];
    if (p.authors) bits.push(p.authors);
    if (p.repository) bits.push(p.repository);
    lines.push(`- ${bits.join(' · ')}`);
    if (p.note) lines.push(`  ${p.note}`);
  }
}
lines.push('', `## Development tools, not part of the app (${tools.length})`, '', '| Package | Version | License |', '|---|---|---|');
for (const p of tools) lines.push(`| ${p.name} | ${p.version} | ${p.declared.replaceAll('|', '\\|')} |`);
lines.push('', '## License texts');
for (const [k, t] of Object.entries(shownTexts)) {
  const n = number.get(k);
  lines.push('', `### Text ${n}`, `<a id="text-${n}"></a>`, '', '```text', t.replace(/```/g, "'''"), '```');
}
const md = `${lines.join('\n')}\n`;

if (checkOnly) {
  const read = (f) => (existsSync(f) ? readFileSync(f, 'utf8').replaceAll('\r\n', '\n') : '');
  if (read(MD) !== md) problems.push('THIRD_PARTY_LICENSES.md is stale. Run: node scripts/licenses.mjs');
  if (read(JSON_OUT) !== json) problems.push('app/src/legal/third-party-licenses.json is stale. Run: node scripts/licenses.mjs');
} else if (!problems.length) {
  writeFileSync(MD, md);
  writeFileSync(JSON_OUT, json);
}

if (problems.length) {
  console.error(`licenses:\n  ${problems.join('\n  ')}`);
  process.exit(1);
}
const shipped = packages.length;
console.log(`licenses: ${shipped} shipped (${Object.keys(shownTexts).length} texts), ${tools.length} development tools${checkOnly ? ', in sync' : ', written'}`);
