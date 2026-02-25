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

import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Check whether the given URL string is an external link (http/https).
 */
function isExternalUrl(url: string): boolean {
    return url.startsWith("http://") || url.startsWith("https://");
}

/**
 * Walk up the DOM tree from the event target to find the nearest `<a>` element
 * within the action's root node.
 */
function findAnchorElement(
    target: EventTarget | null,
    root: HTMLElement,
): HTMLAnchorElement | null {
    let el = target as HTMLElement | null;
    while (el && el !== root) {
        if (el.tagName === "A") {
            return el as HTMLAnchorElement;
        }
        el = el.parentElement;
    }
    return null;
}

/**
 * Svelte action that intercepts clicks on external `<a>` links.
 *
 * - **Always** prevents the default navigation for external URLs (they should
 *   never navigate inside the Tauri webview).
 * - Opens the link in the system's default browser only when the platform
 *   modifier key is held (**Cmd** on macOS / **Ctrl** on Windows & Linux).
 *
 * Usage:
 * ```svelte
 * <div use:externalLinks>
 *   {@html renderedMarkdown}
 * </div>
 * ```
 */
export function externalLinks(node: HTMLElement) {
    /**
     * Handle click events on external links.
     * Prevents default navigation and opens the URL via Tauri opener when
     * the correct modifier key is held.
     */
    function handleClick(e: MouseEvent) {
        const anchor = findAnchorElement(e.target, node);
        if (!anchor) return;

        const href = anchor.getAttribute("href");
        if (!href || !isExternalUrl(href)) return;

        // Always prevent in-webview navigation for external links
        e.preventDefault();
        e.stopPropagation();

        // Open in default browser only when Cmd (macOS) / Ctrl (Win/Linux) is held
        if (e.metaKey || e.ctrlKey) {
            openUrl(href).catch((err) => {
                console.error("Failed to open external URL:", href, err);
            });
        }
    }

    node.addEventListener("click", handleClick);

    return {
        destroy() {
            node.removeEventListener("click", handleClick);
        },
    };
}
