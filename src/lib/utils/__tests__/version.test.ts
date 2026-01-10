// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025-2026 Nico Wiedemann
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

import { describe, it, expect } from 'vitest';
import { APP_VERSION } from '../version';

describe('version utilities', () => {
    describe('APP_VERSION', () => {
        it('should be defined', () => {
            expect(APP_VERSION).toBeDefined();
        });

        it('should start with "v"', () => {
            expect(APP_VERSION).toMatch(/^v/);
        });

        it('should contain a version number in semver format', () => {
            // Match pattern like v1.0.8 or v1.2.3
            expect(APP_VERSION).toMatch(/^v\d+\.\d+\.\d+/);
        });

        it('should match package.json version', () => {
            // Import package.json to verify
            const packageJson = require('../../../../package.json');
            expect(APP_VERSION).toBe(`v${packageJson.version}`);
        });
    });
});
