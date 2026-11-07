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

import { describe, it, expect } from 'vitest';
import type { ContextRule } from '$lib/types';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { FOLDER_VARIABLE, sameRules, usesFolderVariable } from '../rules';

const coil: ContextRule = {
    ruleType: 'title',
    value: '[coil]',
    matchType: 'contains',
    matchCase: false,
    useRegex: false,
};

describe('sameRules', () => {
    it('ignores the order of a rule\'s fields', () => {
        // What an export hands back: the same rule, keys sorted.
        const exported = JSON.parse(
            '{"matchCase":false,"matchType":"contains","ruleType":"title","useRegex":false,"value":"[coil]"}',
        ) as ContextRule;
        expect(sameRules([coil], [exported])).toBe(true);
    });

    it('treats a missing flag as false', () => {
        const bare: ContextRule = { ruleType: 'title', value: '[coil]', matchType: 'contains' };
        expect(sameRules([coil], [bare])).toBe(true);
    });

    it('treats a missing list as empty', () => {
        expect(sameRules(undefined, [])).toBe(true);
        expect(sameRules(undefined, [coil])).toBe(false);
    });

    it('still sees a real difference', () => {
        expect(sameRules([coil], [{ ...coil, value: '[other]' }])).toBe(false);
        expect(sameRules([coil], [{ ...coil, matchCase: true }])).toBe(false);
    });

    it('keeps the order of the rules significant', () => {
        const other: ContextRule = { ...coil, ruleType: 'process', value: 'Code.exe' };
        expect(sameRules([coil, other], [other, coil])).toBe(false);
    });
});

describe('the project folder variable', () => {
    it('is recognised anywhere in a rule value', () => {
        expect(usesFolderVariable({ value: '{folder}' })).toBe(true);
        expect(usesFolderVariable({ value: 'x - {folder} - code' })).toBe(true);
        expect(usesFolderVariable({ value: 'folder' })).toBe(false);
        expect(usesFolderVariable({ value: '{ folder }' })).toBe(false);
    });

    it('is spelled the way the backend expands it', () => {
        // FOLDER_VARIABLE in src-tauri/src/project_folders.rs. Changing one without the
        // other leaves rules that the editor warns about and the matcher ignores.
        const backend = readFileSync(
            resolve(__dirname, '../../../../src-tauri/src/project_folders.rs'),
            'utf-8',
        );
        expect(backend).toContain(`pub const FOLDER_VARIABLE: &str = "${FOLDER_VARIABLE}";`);
    });
});
