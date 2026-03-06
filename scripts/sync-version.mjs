// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

import { readFileSync, writeFileSync } from 'fs';
import { join } from 'path';
import { fileURLToPath } from 'url';
import { dirname } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Syncs the version from package.json to src-tauri/tauri.conf.json
 */
function syncVersion() {
    try {
        // Read package.json
        const packageJsonPath = join(__dirname, '../package.json');
        const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
        const version = packageJson.version;

        if (!version) {
            console.error('❌ No version found in package.json');
            process.exit(1);
        }

        // Read tauri.conf.json
        const tauriConfPath = join(__dirname, '../src-tauri/tauri.conf.json');
        const tauriConfContent = readFileSync(tauriConfPath, 'utf8');
        const tauriConf = JSON.parse(tauriConfContent);

        // Update tauri.conf.json
        if (tauriConf.version !== version) {
            tauriConf.version = version;
            writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n', 'utf8');
            console.log(`✅ Synced version ${version} to tauri.conf.json`);
        } else {
            console.log(`✅ tauri.conf.json version already matches: ${version}`);
        }

        // Read Cargo.toml
        const cargoTomlPath = join(__dirname, '../src-tauri/Cargo.toml');
        let cargoTomlContent = readFileSync(cargoTomlPath, 'utf8');

        // Simple regex to find and replace version in [package] section
        const versionRegex = /^version\s*=\s*"[^"]*"/m;
        const newVersionLine = `version = "${version}"`;

        if (cargoTomlContent.match(versionRegex)) {
            const currentVersionMatch = cargoTomlContent.match(versionRegex)[0];
            if (currentVersionMatch !== newVersionLine) {
                cargoTomlContent = cargoTomlContent.replace(versionRegex, newVersionLine);
                writeFileSync(cargoTomlPath, cargoTomlContent, 'utf8');
                console.log(`✅ Synced version ${version} to Cargo.toml`);
            } else {
                console.log(`✅ Cargo.toml version already matches: ${version}`);
            }
        } else {
            console.warn('⚠️ Could not find version in Cargo.toml');
        }

    } catch (error) {
        console.error('❌ Error syncing version:', error.message);
        process.exit(1);
    }
}

syncVersion();
