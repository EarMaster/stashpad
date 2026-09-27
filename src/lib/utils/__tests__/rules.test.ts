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
import { sameRules } from '../rules';

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
