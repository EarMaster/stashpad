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
import { installKindFor, updateHintFor, storeNameFor, type InstallKind } from '../installation';

describe('installKindFor', () => {
    const cases: Array<[string, InstallKind]> = [
        ['standalone', 'self-update'],
        ['appimage', 'self-update'],
        ['homebrew', 'package-manager'],
        ['scoop', 'package-manager'],
        ['winget', 'package-manager'],
        ['windowsapps', 'package-manager'],
        ['macappstore', 'app-store'],
        ['msstore', 'app-store'],
    ];

    for (const [source, expected] of cases) {
        it(`maps ${source} to ${expected}`, () => {
            expect(installKindFor(source)).toBe(expected);
        });
    }

    it('never lets an unrecognised source self-update', () => {
        // The safety-critical default: overwriting a bundle owned by something else
        // breaks the install rather than updating it, so anything unknown gets advice
        // instead of an action.
        for (const source of ['', 'unknown', 'flatpak', 'snap', 'nix', 'chocolatey']) {
            expect(installKindFor(source)).toBe('package-manager');
        }
    });
});

describe('updateHintFor', () => {
    it('returns the command for each supported package manager', () => {
        expect(updateHintFor('homebrew')).toEqual({
            pmName: 'Homebrew',
            cmd: 'brew upgrade stashpad',
        });
        expect(updateHintFor('scoop')).toEqual({
            pmName: 'Scoop',
            cmd: 'scoop update stashpad',
        });
        expect(updateHintFor('winget')).toEqual({
            pmName: 'winget',
            cmd: 'winget upgrade stashpad',
        });
    });

    it('still understands the legacy windowsapps value', () => {
        expect(updateHintFor('windowsapps')).toEqual(updateHintFor('winget'));
    });

    it('has no hint for self-updating or store installs', () => {
        for (const source of ['standalone', 'appimage', 'macappstore', 'msstore', 'unknown']) {
            expect(updateHintFor(source)).toBeNull();
        }
    });
});

describe('storeNameFor', () => {
    it('names the two stores and nothing else', () => {
        expect(storeNameFor('macappstore')).toBe('Mac App Store');
        expect(storeNameFor('msstore')).toBe('Microsoft Store');
        expect(storeNameFor('standalone')).toBeNull();
        expect(storeNameFor('homebrew')).toBeNull();
    });
});
