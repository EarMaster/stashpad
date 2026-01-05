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

import { writable } from 'svelte/store';

/**
 * Global drag state store.
 * Tracks whether a stash is currently being dragged to prevent
 * stash-to-stash or stash-to-editor attachment drops.
 */

/**
 * Indicates whether a stash drag operation is currently in progress.
 * When true, drop zones on stash cards should be disabled to prevent
 * attaching one stash to another (or to itself).
 */
export const isStashDragging = writable(false);

/**
 * Start a stash drag operation.
 * Call this when initiating a stash drag via the hand icon.
 */
export function startStashDrag() {
    isStashDragging.set(true);
}

/**
 * End a stash drag operation.
 * Call this when the drag operation completes or is cancelled.
 */
export function endStashDrag() {
    isStashDragging.set(false);
}
