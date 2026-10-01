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

// Fails when anything in the tracked source reaches for an old date API instead of
// Temporal.
//
// Every date and time in the frontend goes through `src/lib/utils/time.ts`, which takes
// Temporal from the polyfill as a value, and every one in the backend through
// `src-tauri/src/time.rs`, on `temporal_rs`. Three things break that, and none of them
// shows up as an error:
//
// * A `Date` slips back in. It parses zoneless strings as local time in some engines and
//   rejects them in others, and its `toISOString()` writes a different width than the
//   backend, so two timestamps that look comparable as strings no longer are.
// * Something imports the polyfill on its own, or uses a global `Temporal`. The app would
//   then run native Temporal on Windows and the polyfill on macOS and Linux.
// * `chrono` or `SystemTime` comes back in the backend, with its own idea of the format.
//
// Run with `npm run check:temporal`. CI runs it with the frontend tests.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const TIME_MODULE = 'src/lib/utils/time.ts';
const RUST_TIME_MODULE = 'src-tauri/src/time.rs';

const RULES = [
    {
        what: '`Date` - use the helpers in src/lib/utils/time.ts',
        files: /\.(ts|js|mjs|svelte)$/,
        pattern: /\bnew Date\b|\bDate\.[A-Za-z]|\binstanceof Date\b|[:<|,(]\s*Date\b(?!\s*:)|\bglobalThis\.Date\b/,
    },
    {
        what: 'the Temporal polyfill imported outside src/lib/utils/time.ts',
        files: /\.(ts|js|mjs|svelte)$/,
        pattern: /["'](temporal-polyfill|@js-temporal\/polyfill)(\/[^"']*)?["']/,
        allow: TIME_MODULE,
    },
    {
        what: 'a global Temporal - import it from src/lib/utils/time.ts',
        files: /\.(ts|js|mjs|svelte)$/,
        pattern: /\bglobalThis\.Temporal\b/,
    },
    {
        what: '`chrono` - use src-tauri/src/time.rs',
        files: /\.rs$|Cargo\.toml$/,
        pattern: /\bchrono\b/,
    },
    {
        what: '`SystemTime` - use src-tauri/src/time.rs',
        files: /\.rs$/,
        pattern: /\bSystemTime\b|\bUNIX_EPOCH\b/,
        allow: RUST_TIME_MODULE,
    },
];

/** Comment lines, which may name what was replaced. */
const COMMENT = /^\s*(\/\/|\*|\/\*|<!--|#)/;

const files = execFileSync(
    'git',
    ['ls-files', '--cached', '--others', '--exclude-standard', '*.ts', '*.js', '*.mjs', '*.svelte', '*.rs', 'Cargo.toml'],
    { encoding: 'utf8' },
)
    .split('\n')
    .filter((f) => f && f !== 'scripts/check-temporal.mjs');

const findings = [];
for (const file of files) {
    const lines = readFileSync(file, 'utf8').split('\n');
    lines.forEach((line, i) => {
        if (COMMENT.test(line)) return;
        const code = line.replace(/\s(\/\/|#).*$/, '');
        for (const rule of RULES) {
            if (rule.allow === file || !rule.files.test(file)) continue;
            if (rule.pattern.test(code)) {
                findings.push(`  ${file}:${i + 1}: ${rule.what}\n      ${line.trim()}`);
            }
        }
    });
}

if (findings.length > 0) {
    console.error(`Found ${findings.length} use(s) of the old date APIs:\n`);
    console.error(findings.join('\n'));
    process.exit(1);
}
console.log(`check:temporal - ${files.length} files, no Date, chrono or SystemTime`);
