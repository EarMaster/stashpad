// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

// Verifies CHANGELOG.md has release notes for the version in package.json.
//
//   node scripts/check-changelog.mjs [ref]
//
// The release body and the in-app update notes are both extracted from that section by
// scripts/release-notes.sh, so a missing one means shipping a release that says nothing
// about what changed - which is what happened to every release from v1.6.2 to v1.6.8.
// Run it before tagging; release.yml runs it as a gate job in front of the build matrix.

import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..');

// `::error file=...::` rather than a bare message: on a runner this becomes an annotation
// on the run summary and on CHANGELOG.md itself, so the failure is readable without
// opening the logs.
const fail = (msg) => {
  console.log(`::error file=CHANGELOG.md::${msg}`);
  console.log(`❌ ${msg}`);
  process.exit(1);
};

const warn = (msg) => {
  console.log(`::warning file=CHANGELOG.md::${msg}`);
  console.log(`⚠️  ${msg}`);
};

const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;

// The tag is a cross-check, never the source. tauri-action substitutes __VERSION__ in
// releaseName from package.json, as do every asset filename and latest.json's own version
// field, so notes taken from a tag that disagreed would describe a different build than
// the one attached. Skipped when the ref is not a version tag - workflow_dispatch runs on
// a branch, where github.ref_name is "main".
const ref = (process.argv[2] ?? '').replace(/^refs\/tags\//, '');
if (/^v?\d+\.\d+\.\d+/.test(ref) && ref.replace(/^v/, '') !== version) {
  fail(
    `Tag ${ref} does not match package.json version ${version}. ` +
      `The tag is probably on the wrong commit.`
  );
}

const lines = readFileSync(join(root, 'CHANGELOG.md'), 'utf8')
  .replace(/\r\n/g, '\n')
  .split('\n');

const head = `## [${version}]`;
const start = lines.findIndex((l) => l.startsWith(head));
if (start === -1) {
  fail(
    `CHANGELOG.md has no '${head} - YYYY-MM-DD' section. The release body and the ` +
      `in-app update notes are extracted from it, so releasing now ships empty notes.`
  );
}

const rest = lines.slice(start + 1);
const end = rest.findIndex((l) => l.startsWith('## ['));
const section = end === -1 ? rest : rest.slice(0, end);
const body = section.filter((l) => l.trim() !== '');

if (body.length === 0) {
  fail(`'${head}' in CHANGELOG.md is empty. Move the '## [Unreleased]' entries into it before tagging.`);
}
if (!body.some((l) => /^### /.test(l))) {
  warn(`'${head}' has no '### Added' / '### Fixed' / ... subsection.`);
}
if (!lines.some((l) => l.startsWith('## [Unreleased]'))) {
  warn(`No '## [Unreleased]' section - the next change has nowhere to go.`);
}

console.log(`✅ CHANGELOG.md has ${body.length} lines of release notes for ${version}:\n`);
console.log(section.join('\n').trim());
