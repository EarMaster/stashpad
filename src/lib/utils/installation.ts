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

/**
 * How Stashpad was installed, and what that means for updating it.
 *
 * The value comes from the `get_installation_source` Rust command, which sniffs the
 * executable's path and environment. This module turns that string into the one thing
 * the updater actually needs to know: who is allowed to replace the binary.
 *
 * Deliberately free of Tauri imports so it stays unit-testable.
 */

/** Raw values `get_installation_source` can return. */
export type InstallSource =
    | 'standalone'
    | 'appimage'
    | 'homebrew'
    | 'scoop'
    | 'winget'
    | 'msstore'
    | 'macappstore';

/**
 * Who performs the update.
 *
 * - `self-update` - we may download and swap the bundle ourselves.
 * - `package-manager` - tell the user which command to run.
 * - `app-store` - the store updates the app; we only mention that a version exists.
 */
export type InstallKind = 'self-update' | 'package-manager' | 'app-store';

/**
 * Map an installation source onto who may update the app.
 *
 * An unrecognised source maps to `package-manager`, never `self-update`. Overwriting a
 * bundle that something else owns - a signed and sandboxed App Store copy, a read-only
 * package mount - breaks the install rather than updating it, so the safe default is to
 * tell the user instead of acting.
 */
export function installKindFor(source: string): InstallKind {
    switch (source) {
        case 'standalone':
        // An AppImage is a single file we own, so it can be swapped like a standalone build.
        case 'appimage':
            return 'self-update';
        case 'macappstore':
        case 'msstore':
            return 'app-store';
        default:
            return 'package-manager';
    }
}

/**
 * The command that updates a package-manager install, or `null` when we do not know one.
 *
 * `windowsapps` is the value older builds returned before Microsoft Store and winget
 * were told apart; it is still accepted so a stale value does not lose its hint.
 */
export function updateHintFor(source: string): { pmName: string; cmd: string } | null {
    switch (source) {
        case 'homebrew':
            return { pmName: 'Homebrew', cmd: 'brew upgrade stashpad' };
        case 'scoop':
            return { pmName: 'Scoop', cmd: 'scoop update stashpad' };
        case 'winget':
        case 'windowsapps':
            return { pmName: 'winget', cmd: 'winget upgrade stashpad' };
        default:
            return null;
    }
}

/** Display name of the store that owns this install, or `null` if it is not a store. */
export function storeNameFor(source: string): string | null {
    switch (source) {
        case 'macappstore':
            return 'Mac App Store';
        case 'msstore':
            return 'Microsoft Store';
        default:
            return null;
    }
}
