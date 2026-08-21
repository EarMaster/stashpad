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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { UpdateChecker, type UpdaterSettingsPatch, type PendingUpdate } from '../updater.svelte';
import type { Settings } from '$lib/types';

const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;

/**
 * A checker wired to fakes, plus handles on everything the test needs to steer.
 *
 * `now` is a plain number the test advances by hand rather than reading the real clock,
 * so the 48h and 7-day deadlines can be crossed without waiting for them.
 */
function harness(options: { update?: PendingUpdate | null; source?: string } = {}) {
    let now = 1_700_000_000_000;
    const settings: Settings = {
        autoContextDetection: true,
        shortcuts: {},
    };

    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    const update: PendingUpdate | null =
        options.update === undefined
            ? { version: '2.0.0', body: 'Fixes', downloadAndInstall }
            : options.update;

    const check = vi.fn().mockImplementation(async () => update);
    const installationSource = vi.fn().mockResolvedValue(options.source ?? 'standalone');
    const relaunch = vi.fn().mockResolvedValue(undefined);
    const persist = vi.fn((patch: UpdaterSettingsPatch) => Object.assign(settings, patch));

    const checker = new UpdateChecker();
    checker.configure({
        check,
        installationSource,
        relaunch,
        now: () => now,
        persist,
    });

    return {
        checker,
        settings,
        check,
        installationSource,
        relaunch,
        persist,
        downloadAndInstall,
        get now() {
            return now;
        },
        /**
         * Move both clocks forward.
         *
         * Async on purpose: the synchronous `advanceTimersByTime` cannot flush the
         * microtasks an in-flight check is waiting on, so a check started by an earlier
         * tick would only settle later and stamp itself with the wrong `now`.
         */
        async advance(ms: number) {
            now += ms;
            await vi.advanceTimersByTimeAsync(ms);
        },
    };
}

function baseSettings(overrides: Partial<Settings> = {}): Settings {
    return { autoContextDetection: true, shortcuts: {}, ...overrides };
}

describe('UpdateChecker.check', () => {
    it('records an available update and persists what it found', async () => {
        const h = harness();
        await h.checker.check();

        expect(h.checker.lastResult).toBe('available');
        expect(h.checker.available).toMatchObject({
            version: '2.0.0',
            notes: 'Fixes',
            source: 'standalone',
            kind: 'self-update',
        });
        expect(h.settings.lastUpdateCheckAt).toBe(h.now);
        expect(h.settings.latestKnownUpdateVersion).toBe('2.0.0');
    });

    it('reports up-to-date and clears the remembered version', async () => {
        const h = harness({ update: null });
        await h.checker.check();

        expect(h.checker.lastResult).toBe('up-to-date');
        expect(h.checker.available).toBeNull();
        expect(h.settings.lastUpdateCheckAt).toBe(h.now);
        expect(h.settings.latestKnownUpdateVersion).toBeUndefined();
    });

    it('does not persist a timestamp when the check fails', async () => {
        // A failed check is not a check. Recording one would lock a briefly-offline user
        // out of updates for the next 48 hours.
        const h = harness();
        h.check.mockRejectedValue(new Error('offline'));
        vi.spyOn(console, 'error').mockImplementation(() => {});

        await h.checker.check();

        expect(h.checker.lastResult).toBe('error');
        expect(h.checker.lastError).toBe('offline');
        expect(h.settings.lastUpdateCheckAt).toBeUndefined();
        expect(h.checker.status).toBe('idle');
    });

    it('passes a timeout so a hung endpoint cannot wedge the store', async () => {
        const h = harness();
        await h.checker.check();
        expect(h.check).toHaveBeenCalledWith({ timeout: expect.any(Number) });
    });

    it('collapses concurrent calls into one request', async () => {
        const h = harness();
        await Promise.all([h.checker.check(), h.checker.check(), h.checker.check()]);
        expect(h.check).toHaveBeenCalledTimes(1);
    });

    it('an explicit check clears both sit-out fields', async () => {
        const h = harness();
        h.checker.hydrate(
            baseSettings({ dismissedUpdateVersion: '2.0.0', updateRemindAfter: h.now + DAY }),
            'v1.0.0',
        );

        await h.checker.check({ interactive: true });

        expect(h.checker.dismissedVersion).toBeNull();
        expect(h.checker.remindAfter).toBeNull();
        expect(h.settings.dismissedUpdateVersion).toBeUndefined();
        expect(h.settings.updateRemindAfter).toBeUndefined();
        expect(h.checker.showIndicator).toBe(true);
    });
});

describe('UpdateChecker sit-out modes', () => {
    it('skipVersion hides that version but not a newer one', async () => {
        const h = harness();
        await h.checker.check();
        expect(h.checker.showIndicator).toBe(true);

        h.checker.skipVersion();
        expect(h.checker.showIndicator).toBe(false);
        expect(h.settings.dismissedUpdateVersion).toBe('2.0.0');

        // Something newer arrives: the skip was for 2.0.0 only.
        h.check.mockResolvedValue({
            version: '2.1.0',
            body: '',
            downloadAndInstall: vi.fn(),
        });
        await h.checker.check();
        expect(h.checker.showIndicator).toBe(true);
    });

    it('remindLater hides the same version for a week, then shows it again', async () => {
        const h = harness();
        await h.checker.check();

        h.checker.remindLater();
        expect(h.checker.showIndicator).toBe(false);
        expect(h.settings.updateRemindAfter).toBe(h.now + 7 * DAY);

        await h.advance(6 * DAY);
        expect(h.checker.showIndicator).toBe(false);

        await h.advance(2 * DAY);
        expect(h.checker.showIndicator).toBe(true);
    });

    it('showAgain undoes both modes', async () => {
        const h = harness();
        await h.checker.check();
        h.checker.skipVersion();
        h.checker.remindLater();
        expect(h.checker.showIndicator).toBe(false);

        h.checker.showAgain();
        expect(h.checker.showIndicator).toBe(true);
        expect(h.settings.dismissedUpdateVersion).toBeUndefined();
        expect(h.settings.updateRemindAfter).toBeUndefined();
    });
});

describe('UpdateChecker.hydrate', () => {
    it('restores the indicator from settings without a network call', () => {
        const h = harness();
        h.checker.hydrate(baseSettings({ latestKnownUpdateVersion: '2.0.0' }), 'v1.0.0');

        expect(h.checker.knownVersion).toBe('2.0.0');
        expect(h.checker.indicatorVersion).toBe('2.0.0');
        expect(h.checker.showIndicator).toBe(true);
        expect(h.check).not.toHaveBeenCalled();
    });

    it('does not invent an UpdateInfo from persisted state', () => {
        // A synthetic one put three untruths on screen: an install method that had not
        // been detected, a "update it there" line with no command, and an install button
        // with nothing downloaded behind it. Only a real check may populate `available`.
        const h = harness();
        h.checker.hydrate(baseSettings({ latestKnownUpdateVersion: '2.0.0' }), 'v1.0.0');
        expect(h.checker.available).toBeNull();
    });

    it('a sit-out still works on a version restored from settings', () => {
        const h = harness();
        h.checker.hydrate(baseSettings({ latestKnownUpdateVersion: '2.0.0' }), 'v1.0.0');

        h.checker.skipVersion();
        expect(h.settings.dismissedUpdateVersion).toBe('2.0.0');
        expect(h.checker.showIndicator).toBe(false);
    });

    it('clears the sit-out fields once the update has been installed', () => {
        const h = harness();
        h.checker.hydrate(
            baseSettings({
                latestKnownUpdateVersion: '2.0.0',
                dismissedUpdateVersion: '2.0.0',
                updateRemindAfter: h.now + DAY,
            }),
            'v2.0.0',
        );

        expect(h.checker.available).toBeNull();
        expect(h.checker.knownVersion).toBeNull();
        expect(h.checker.showIndicator).toBe(false);
        expect(h.settings.latestKnownUpdateVersion).toBeUndefined();
        expect(h.settings.dismissedUpdateVersion).toBeUndefined();
        expect(h.settings.updateRemindAfter).toBeUndefined();
    });
});

describe('UpdateChecker automatic schedule', () => {
    it('checks shortly after start, then once per 48h', async () => {
        const h = harness();
        h.checker.hydrate(baseSettings(), 'v1.0.0');
        h.checker.startAutoChecks();

        expect(h.check).not.toHaveBeenCalled();

        await h.advance(30_000);
        await vi.waitFor(() => expect(h.check).toHaveBeenCalledTimes(1));

        await h.advance(47 * HOUR);
        expect(h.check).toHaveBeenCalledTimes(1);

        await h.advance(2 * HOUR);
        await vi.waitFor(() => expect(h.check).toHaveBeenCalledTimes(2));
    });

    it('does not re-check on launch when the last check is recent', async () => {
        const h = harness();
        h.checker.hydrate(baseSettings({ lastUpdateCheckAt: h.now - HOUR }), 'v1.0.0');
        h.checker.startAutoChecks();

        await h.advance(30_000);
        expect(h.check).not.toHaveBeenCalled();
    });

    it('retries two hours after a failure rather than waiting the full interval', async () => {
        const h = harness();
        vi.spyOn(console, 'error').mockImplementation(() => {});
        h.check.mockRejectedValue(new Error('offline'));
        h.checker.hydrate(baseSettings(), 'v1.0.0');
        h.checker.startAutoChecks();

        await h.advance(30_000);
        await vi.waitFor(() => expect(h.check).toHaveBeenCalledTimes(1));

        await h.advance(HOUR);
        expect(h.check).toHaveBeenCalledTimes(1);

        await h.advance(2 * HOUR);
        await vi.waitFor(() => expect(h.check).toHaveBeenCalledTimes(2));
    });

    it('stays quiet when automatic checks are switched off', async () => {
        const h = harness();
        h.checker.hydrate(baseSettings({ autoUpdateChecks: false }), 'v1.0.0');
        h.checker.startAutoChecks();

        await h.advance(3 * DAY);
        expect(h.check).not.toHaveBeenCalled();
    });

    it('dispose stops the heartbeat', async () => {
        const h = harness();
        h.checker.hydrate(baseSettings(), 'v1.0.0');
        h.checker.startAutoChecks();
        h.checker.dispose();

        await h.advance(3 * DAY);
        expect(h.check).not.toHaveBeenCalled();
    });
});

describe('UpdateChecker.install', () => {
    it('installs and relaunches a standalone build', async () => {
        const h = harness();
        await h.checker.check();
        await h.checker.install();

        expect(h.downloadAndInstall).toHaveBeenCalledTimes(1);
        expect(h.relaunch).toHaveBeenCalledTimes(1);
    });

    it('refuses to touch a package-manager install', async () => {
        const h = harness({ source: 'homebrew' });
        await h.checker.check();

        await expect(h.checker.install()).rejects.toThrow();
        expect(h.downloadAndInstall).not.toHaveBeenCalled();
        expect(h.relaunch).not.toHaveBeenCalled();
    });

    it('refuses to touch an app-store install', async () => {
        // Rewriting a signed, sandboxed bundle does not update it - it gets the app
        // killed on next launch.
        const h = harness({ source: 'macappstore' });
        await h.checker.check();

        await expect(h.checker.install()).rejects.toThrow();
        expect(h.downloadAndInstall).not.toHaveBeenCalled();
    });

    it('refuses to install off a version restored from settings', async () => {
        // Nothing has been downloaded and the source is unverified, so there is nothing
        // legitimate to install here.
        const h = harness();
        h.checker.hydrate(baseSettings({ latestKnownUpdateVersion: '2.0.0' }), 'v1.0.0');

        await expect(h.checker.install()).rejects.toThrow();
        expect(h.downloadAndInstall).not.toHaveBeenCalled();
    });
});

beforeEach(() => {
    vi.useFakeTimers();
});

afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
});
