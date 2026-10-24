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

import type { StashItem } from "$lib/types";

/**
 * The queue as it stood when it was last on screen, kept across mounts of `Queue`.
 *
 * Settings and the context manager replace the main view, so returning from either
 * mounts a fresh Queue, and starting that from an empty array showed "queue empty"
 * until its load came back. `main.ts` fills this before the first mount, while the
 * splash screen is still up, and Queue keeps it current after that; either way a mount
 * starts from it and reconciles with the database in the background.
 *
 * Null until something has actually read the stashes, so the empty state never claims a
 * queue nobody has loaded yet.
 */
export const queueCache: { stashes: StashItem[] | null } = { stashes: null };
