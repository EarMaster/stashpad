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

// Verifies that the manifests and lockfiles agree with each other.
//
//   node scripts/check-lockfiles.mjs            # the working tree
//   node scripts/check-lockfiles.mjs --staged   # the git index, for the commit preflight
//
// The app carries two dependency trees that have to move together, and each tree has a
// manifest and a lockfile that have to move together too. Nothing else compares them:
// the tests, tsc and `cargo check` all pass on a tree where they disagree. v1.12.2's release
// failed on all six platforms because `npm update` moved five @tauri-apps plugins to new
// minors while their Rust crates stayed behind, and `tauri build` - which only the release
// runs - refuses a pair whose major.minor differs. The same release had also left
// package-lock.json on the previous version number, because the bump never wrote it.
//
// Checks:
//   1. One version everywhere: package.json, both package-lock.json entries,
//      tauri.conf.json, Cargo.toml, and the stashpad entry in Cargo.lock.
//   2. package-lock.json was regenerated after the last package.json edit: its root entry
//      declares exactly the ranges package.json does.
//   3. Every @tauri-apps npm package has a Rust crate on the same major.minor - the check
//      `tauri build` makes, run here so it fails in seconds instead of after the matrix.
//
// Cargo.toml against Cargo.lock is Cargo's own job: CI runs cargo with --locked, which
// fails on a stale Cargo.lock instead of quietly re-resolving it on the runner.

import { readFileSync } from 'fs';
import { execFileSync } from 'child_process';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const staged = process.argv.includes('--staged');

const read = (path) =>
    staged
        ? execFileSync('git', ['show', `:${path}`], { cwd: root, encoding: 'utf8', maxBuffer: 64 << 20 })
        : readFileSync(join(root, path), 'utf8');

const errors = [];
const fail = (file, message) => {
    errors.push(message);
    // An annotation on a runner, a plain line everywhere else.
    console.error(process.env.GITHUB_ACTIONS ? `::error file=${file}::${message}` : `  ${file}: ${message}`);
};

const pkg = JSON.parse(read('package.json'));
const lock = JSON.parse(read('package-lock.json'));
const conf = JSON.parse(read('src-tauri/tauri.conf.json'));
const cargoToml = read('src-tauri/Cargo.toml');
const cargoLock = read('src-tauri/Cargo.lock');

/** name -> versions, from every [[package]] in a Cargo.lock. */
const crates = new Map();
for (const block of cargoLock.split('[[package]]').slice(1)) {
    const name = block.match(/^name = "(.+)"$/m)?.[1];
    const version = block.match(/^version = "(.+)"$/m)?.[1];
    if (name && version) crates.set(name, [...(crates.get(name) ?? []), version]);
}

// 1. One version everywhere.
const version = pkg.version;
const cargoVersion = cargoToml.match(/^\[package\][\s\S]*?^version = "(.+)"$/m)?.[1];
const versions = [
    ['package-lock.json', 'version', lock.version],
    ['package-lock.json', 'packages[""].version', lock.packages?.['']?.version],
    ['src-tauri/tauri.conf.json', 'version', conf.version],
    ['src-tauri/Cargo.toml', '[package] version', cargoVersion],
    ['src-tauri/Cargo.lock', 'stashpad', crates.get('stashpad')?.[0]],
];
for (const [file, field, found] of versions) {
    if (found !== version) {
        fail(file, `${field} is ${found ?? 'missing'}, but package.json is ${version}. ` +
            `A version bump has to write every file - see .claude/scripts/release-prepare.sh.`);
    }
}

// 2. package-lock.json regenerated after package.json changed.
const declared = lock.packages?.[''] ?? {};
for (const section of ['dependencies', 'devDependencies', 'optionalDependencies']) {
    const want = pkg[section] ?? {};
    const have = declared[section] ?? {};
    for (const name of new Set([...Object.keys(want), ...Object.keys(have)])) {
        if (want[name] !== have[name]) {
            fail('package-lock.json', `${section}.${name} is ${have[name] ?? 'absent'} in the lockfile but ` +
                `${want[name] ?? 'absent'} in package.json. Run npm install and commit both.`);
        }
    }
}

// 3. @tauri-apps npm packages against their crates.
const minor = (v) => v.split('.').slice(0, 2).join('.');
const crateFor = (npmName) => {
    const short = npmName.slice('@tauri-apps/'.length);
    if (short === 'api') return 'tauri';
    if (short.startsWith('plugin-')) return `tauri-plugin-${short.slice('plugin-'.length)}`;
    return null; // the CLI and its platform binaries have no crate counterpart
};
for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (!/^node_modules\/@tauri-apps\/[^/]+$/.test(path)) continue;
    const npmName = path.slice('node_modules/'.length);
    const crate = crateFor(npmName);
    if (!crate) continue;
    const crateVersions = crates.get(crate);
    if (!crateVersions) {
        fail('src-tauri/Cargo.lock', `${npmName} ${entry.version} is installed, but ${crate} is not in Cargo.lock.`);
        continue;
    }
    if (!crateVersions.some((v) => minor(v) === minor(entry.version))) {
        fail('src-tauri/Cargo.lock', `${npmName} is ${entry.version} but ${crate} is ${crateVersions.join(', ')}. ` +
            `tauri build refuses a pair whose major.minor differs - update both sides together.`);
    }
}

if (errors.length) {
    console.error(`\ncheck-lockfiles: ${errors.length} problem(s)${staged ? ' in the staged files' : ''}.`);
    process.exit(1);
}
console.log(`check-lockfiles: ${version} everywhere, package-lock.json matches package.json, ` +
    `every @tauri-apps package matches its crate${staged ? ' (staged files)' : ''}.`);
