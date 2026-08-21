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
import type { SyncStatus } from "$lib/services/cloud-sync";

/**
 * How long a sync has to run before the UI admits it is running.
 *
 * A sync is triggered after every local write and by every notification from
 * another device, and most of them finish in well under this. Announcing each one
 * made the header icon strobe and the settings panel reflow every few seconds, for
 * work the user has no reason to watch.
 */
const SYNCING_VISIBLE_AFTER_MS = 600;

/**
 * A sync status suitable for display: identical to the real one, except that
 * "syncing" only surfaces once the sync has lasted {@link SYNCING_VISIBLE_AFTER_MS}.
 * A sync that finishes sooner never appears at all - the previous state simply
 * stands until the new one replaces it.
 *
 * Every other state passes straight through, so failures are never hidden or
 * delayed. Use this for anything the user looks at; keep using the raw status for
 * logic, such as disabling the manual sync button.
 *
 * Must be called during component initialisation - it owns an `$effect`.
 *
 * @param getStatus Reader for the live status, e.g. `() => syncStatus`
 */
export function createSyncDisplay(getStatus: () => SyncStatus): {
    readonly current: SyncStatus;
} {
    let shown = $state<SyncStatus>(getStatus());

    $effect(() => {
        const next = getStatus();

        if (next !== "syncing") {
            shown = next;
            return;
        }

        // Held back on purpose. If the sync settles first this effect re-runs, the
        // teardown below drops the timer, and the spinner never appears.
        const timer = setTimeout(() => {
            shown = "syncing";
        }, SYNCING_VISIBLE_AFTER_MS);

        return () => clearTimeout(timer);
    });

    return {
        get current() {
            return shown;
        },
    };
}
