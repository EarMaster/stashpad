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

/**
 * Update checking, minus the user interface.
 *
 * This store decides *whether* to check, remembers what it found, and knows who is
 * allowed to install it. It never opens a dialog - `App.svelte` owns all of that, so an
 * automatic check can stay silent while an explicit one is chatty without duplicating any
 * of the logic here.
 *
 * Every Tauri entry point arrives through `configure()`. That keeps the module free of
 * plugin imports and makes the whole thing testable against plain fakes.
 */

import type { Settings } from '../types';
import { installKindFor, type InstallKind } from '../utils/installation';

/** How long a successful check is good for. */
const CHECK_INTERVAL_MS = 48 * 60 * 60 * 1000;

/** Delay before the first check, so launch is not competing with the DB load and first sync. */
const STARTUP_DELAY_MS = 20_000;

/**
 * How often the deadline is reconsidered.
 *
 * A single 48h timer would be wrong twice over: webview timers are throttled in the
 * background, and a machine that sleeps through the deadline never fires it. Comparing
 * the clock against a persisted timestamp once an hour handles both.
 */
const HEARTBEAT_MS = 60 * 60 * 1000;

/** After a failed check, wait this long before trying again - not the full 48h. */
const RETRY_AFTER_ERROR_MS = 2 * 60 * 60 * 1000;

/** How long "remind me later" hides an update the user has not skipped outright. */
const REMIND_LATER_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * Ceiling on a single check.
 *
 * Without it a hung endpoint leaves `status` at `'checking'` forever, and the reentrancy
 * guard then blocks every later check for the lifetime of the process.
 */
const CHECK_TIMEOUT_MS = 30_000;

/** The subset of settings this store reads and writes. */
export type UpdaterSettingsPatch = Partial<
    Pick<
        Settings,
        | 'lastUpdateCheckAt'
        | 'latestKnownUpdateVersion'
        | 'dismissedUpdateVersion'
        | 'updateRemindAfter'
    >
>;

/** Minimal shape of what the updater plugin hands back. */
export interface PendingUpdate {
    version: string;
    body?: string | null;
    downloadAndInstall(): Promise<void>;
}

export interface UpdaterDeps {
    /** `check` from `@tauri-apps/plugin-updater`. Resolves `null` when up to date. */
    check(options: { timeout: number }): Promise<PendingUpdate | null>;
    /** The `get_installation_source` command. */
    installationSource(): Promise<string>;
    /** `relaunch` from `@tauri-apps/plugin-process`. */
    relaunch(): Promise<void>;
    now(): number;
    /** Merge a patch into the live settings object and persist it. */
    persist(patch: UpdaterSettingsPatch): void;
}

export interface UpdateInfo {
    version: string;
    notes: string;
    source: string;
    kind: InstallKind;
}

export type CheckResult = 'none' | 'up-to-date' | 'available' | 'error';

/**
 * Exported so tests can work against a fresh instance. Application code should use the
 * `updateChecker` singleton below.
 */
export class UpdateChecker {
    status = $state<'idle' | 'checking' | 'installing'>('idle');
    /** A verified update from a check this session: real source, real notes, installable. */
    available = $state<UpdateInfo | null>(null);
    /** A newer version remembered from a previous session. Version string only. */
    knownVersion = $state<string | null>(null);
    lastResult = $state<CheckResult>('none');
    lastError = $state<string | null>(null);
    lastCheckedAt = $state<number | null>(null);

    /** Set once the user skips a version or asks to be reminded later. */
    dismissedVersion = $state<string | null>(null);
    remindAfter = $state<number | null>(null);

    private deps: UpdaterDeps | null = null;
    private autoChecksEnabled = true;
    /** Error backoff. In memory only - a restart should retry immediately. */
    private nextAttemptAt = 0;
    private startupTimer: ReturnType<typeof setTimeout> | null = null;
    private heartbeat: ReturnType<typeof setInterval> | null = null;
    /** The handle from the last successful check, needed to install. */
    private pending: PendingUpdate | null = null;

    configure(deps: UpdaterDeps): void {
        this.deps = deps;
    }

    /**
     * Restore what the last session knew.
     *
     * Only the *version* is restored, never a synthetic `UpdateInfo`. An earlier revision
     * rebuilt `available` from settings with `source: 'unknown'` and a cautious
     * `package-manager` kind, which put three untruths on screen at once: it claimed an
     * install method it had not detected, printed "update it there" with no command to
     * print, and offered an install button with no downloaded update behind it. Knowing a
     * newer version exists and knowing what to do about it are different facts, so they
     * are held separately.
     */
    hydrate(settings: Settings, currentVersion: string): void {
        const running = normalizeVersion(currentVersion);
        this.autoChecksEnabled = settings.autoUpdateChecks ?? true;
        this.lastCheckedAt = settings.lastUpdateCheckAt ?? null;
        this.dismissedVersion = settings.dismissedUpdateVersion ?? null;
        this.remindAfter = settings.updateRemindAfter ?? null;

        const known = settings.latestKnownUpdateVersion;
        if (!known) return;

        if (normalizeVersion(known) === running) {
            // The user has since installed it. Clear the whole set, or the next update
            // would arrive already skipped.
            this.available = null;
            this.knownVersion = null;
            this.dismissedVersion = null;
            this.remindAfter = null;
            this.deps?.persist({
                latestKnownUpdateVersion: undefined,
                dismissedUpdateVersion: undefined,
                updateRemindAfter: undefined,
            });
            return;
        }

        this.knownVersion = known;
    }

    /**
     * The newer version the user should be told about, checked this session or not.
     *
     * `available` wins because it is the verified one; `knownVersion` carries the news
     * across a restart so the indicator does not vanish for the first 20 seconds.
     */
    get indicatorVersion(): string | null {
        return this.available?.version ?? this.knownVersion;
    }

    /** Whether the header should be showing an update notice right now. */
    get showIndicator(): boolean {
        const version = this.indicatorVersion;
        if (!version) return false;
        if (this.dismissedVersion && normalizeVersion(version) === normalizeVersion(this.dismissedVersion)) {
            return false;
        }
        const now = this.deps?.now() ?? Date.now();
        return now >= (this.remindAfter ?? 0);
    }

    /** Whether either sit-out mode is currently hiding something. */
    get isSittingOut(): boolean {
        return this.indicatorVersion !== null && !this.showIndicator;
    }

    /**
     * Begin the background schedule: one delayed check after launch, then an hourly
     * reconsideration of the deadline.
     */
    startAutoChecks(): void {
        if (!this.autoChecksEnabled) return;
        this.dispose();
        this.startupTimer = setTimeout(() => this.tick(), STARTUP_DELAY_MS);
        this.heartbeat = setInterval(() => this.tick(), HEARTBEAT_MS);
    }

    dispose(): void {
        if (this.startupTimer !== null) clearTimeout(this.startupTimer);
        if (this.heartbeat !== null) clearInterval(this.heartbeat);
        this.startupTimer = null;
        this.heartbeat = null;
    }

    /** Enable or disable the background schedule at runtime. */
    setAutoChecks(enabled: boolean): void {
        if (this.autoChecksEnabled === enabled) return;
        this.autoChecksEnabled = enabled;
        this.dispose();
        if (enabled) this.startAutoChecks();
    }

    private tick(): void {
        if (!this.autoChecksEnabled) return;
        const now = this.deps?.now() ?? Date.now();
        const due = Math.max((this.lastCheckedAt ?? 0) + CHECK_INTERVAL_MS, this.nextAttemptAt);
        if (now >= due) void this.check();
    }

    /**
     * Ask the endpoint what the newest version is.
     *
     * `interactive` marks a check the user asked for, which also clears both sit-out
     * fields: having explicitly pressed the button, they are evidently no longer sitting
     * this one out.
     */
    async check(opts: { interactive?: boolean } = {}): Promise<void> {
        const deps = this.deps;
        if (!deps) return;
        if (this.status !== 'idle') return;

        if (opts.interactive && (this.dismissedVersion !== null || this.remindAfter !== null)) {
            this.dismissedVersion = null;
            this.remindAfter = null;
            deps.persist({ dismissedUpdateVersion: undefined, updateRemindAfter: undefined });
        }

        this.status = 'checking';
        this.lastError = null;
        try {
            const update = await deps.check({ timeout: CHECK_TIMEOUT_MS });
            const checkedAt = deps.now();
            this.lastCheckedAt = checkedAt;
            this.nextAttemptAt = 0;

            if (!update) {
                this.available = null;
                this.knownVersion = null;
                this.pending = null;
                this.lastResult = 'up-to-date';
                deps.persist({
                    lastUpdateCheckAt: checkedAt,
                    latestKnownUpdateVersion: undefined,
                });
                return;
            }

            const source = await deps.installationSource().catch(() => 'unknown');
            this.available = {
                version: update.version,
                notes: update.body ?? '',
                source,
                kind: installKindFor(source),
            };
            this.knownVersion = update.version;
            this.pending = update;
            this.lastResult = 'available';
            deps.persist({
                lastUpdateCheckAt: checkedAt,
                latestKnownUpdateVersion: update.version,
            });
        } catch (e) {
            // Deliberately not persisting `lastUpdateCheckAt`: a failed check is not a
            // check. Recording it would lock a user who was briefly offline out of
            // updates for the next 48 hours.
            this.lastResult = 'error';
            this.lastError = e instanceof Error ? e.message : String(e);
            this.nextAttemptAt = deps.now() + RETRY_AFTER_ERROR_MS;
            console.error('Failed to check for updates:', e);
        } finally {
            this.status = 'idle';
        }
    }

    /**
     * Download and install, then restart.
     *
     * Refuses anything we do not own. Replacing a store-managed or package-managed bundle
     * does not update it, it breaks it - a signed macOS bundle rewritten in place is
     * killed by Gatekeeper on next launch.
     */
    async install(): Promise<void> {
        const deps = this.deps;
        if (!deps) throw new Error('Updater is not configured');
        if (this.available?.kind !== 'self-update') {
            throw new Error(`Updates for a ${this.available?.kind ?? 'unknown'} install are not ours to apply`);
        }
        if (!this.pending) throw new Error('No update has been downloaded');
        if (this.status !== 'idle') return;

        this.status = 'installing';
        try {
            await this.pending.downloadAndInstall();
            await deps.relaunch();
        } finally {
            this.status = 'idle';
        }
    }

    /** Hide this exact version for good. Anything newer shows up again. */
    skipVersion(): void {
        const version = this.indicatorVersion;
        if (!version) return;
        this.dismissedVersion = version;
        this.deps?.persist({ dismissedUpdateVersion: version });
    }

    /** Hide the notice for a week, then show it again even for the same version. */
    remindLater(): void {
        if (!this.indicatorVersion) return;
        const until = (this.deps?.now() ?? Date.now()) + REMIND_LATER_MS;
        this.remindAfter = until;
        this.deps?.persist({ updateRemindAfter: until });
    }

    /** Undo both sit-out modes, so a hidden update becomes visible again. */
    showAgain(): void {
        this.dismissedVersion = null;
        this.remindAfter = null;
        this.deps?.persist({ dismissedUpdateVersion: undefined, updateRemindAfter: undefined });
    }
}

/** Compare versions ignoring a leading `v`, which `APP_VERSION` carries and the manifest does not. */
function normalizeVersion(version: string): string {
    return version.trim().replace(/^v/i, '');
}

export const updateChecker = new UpdateChecker();
