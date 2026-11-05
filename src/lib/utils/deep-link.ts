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
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { log } from "$lib/utils/log";

/**
 * `stashpad://stash/<id>` - open one stash in the app.
 *
 * The cloud's MCP server hands this link out: `show_stash` returns it to an agent in a
 * terminal, which cannot show the stash card, and the card's "Open in Stashpad" button
 * opens it. Either way the user lands on the stash in their own queue.
 *
 * Only a stash id travels in the link, never content, so a link pasted somewhere it should
 * not be reveals nothing; the stash has to exist on this device for anything to happen.
 */
const STASH_LINK =
    /^stashpad:\/\/stash\/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\/?$/;

/** The stash id a link points at, or null for any other link. */
export function parseStashLink(url: string): string | null {
    const match = STASH_LINK.exec(url.trim());
    return match ? match[1].toLowerCase() : null;
}

/**
 * Call `handler` for every stash link that opens the app: the one it was launched with,
 * if any, and each one after that while it runs. Returns the function that stops listening.
 *
 * A link that arrives while the app is running reaches `onOpenUrl` - on Windows and Linux
 * through the single-instance plugin, which also brings the window forward. One that
 * started the app is only available from `getCurrent`.
 */
export async function onStashLink(handler: (id: string) => void): Promise<() => void> {
    const take = (urls: string[] | null) => {
        for (const url of urls ?? []) {
            const id = parseStashLink(url);
            if (id) handler(id);
        }
    };

    const stop = await onOpenUrl(take);
    try {
        take(await getCurrent());
    } catch (err) {
        log.warn("[deep-link] Could not read the link the app was started with:", err);
    }
    return stop;
}
