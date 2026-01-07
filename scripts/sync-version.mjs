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

        // Check if version needs updating
        if (tauriConf.version === version) {
            console.log(`✅ Version already synced: ${version}`);
            return;
        }

        // Update version
        tauriConf.version = version;

        // Write back to tauri.conf.json with proper formatting
        // Preserve the original formatting by using 2 spaces
        writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n', 'utf8');

        console.log(`✅ Synced version ${version} from package.json to tauri.conf.json`);
    } catch (error) {
        console.error('❌ Error syncing version:', error.message);
        process.exit(1);
    }
}

syncVersion();
