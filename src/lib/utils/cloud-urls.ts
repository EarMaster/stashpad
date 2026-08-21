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

/** Website origin used when the configured endpoint tells us nothing. */
export const DEFAULT_WEBSITE_ORIGIN = "https://stashpad.org";

/**
 * Derives the website origin from the configured cloud API endpoint.
 *
 * The API lives at api.stashpad.org and the site it belongs to at stashpad.org,
 * so the leading "api." subdomain is stripped. A self-hosted endpoint without
 * that prefix is returned as-is, which keeps a custom deployment pointing at its
 * own site.
 *
 * @returns The origin, or null when the endpoint is missing or unparseable -
 *          callers decide whether to fall back to the API or to the public site.
 */
export function websiteOriginFromEndpoint(endpoint?: string): string | null {
    if (!endpoint) return null;
    try {
        const url = new URL(endpoint);
        if (url.hostname.startsWith("api.")) {
            url.hostname = url.hostname.slice("api.".length);
        }
        return url.origin;
    } catch {
        return null;
    }
}

/**
 * URL of the cloud sync waitlist: the pricing section on the website's landing
 * page, which hosts the signup form.
 */
export function waitlistUrl(endpoint?: string): string {
    const origin = websiteOriginFromEndpoint(endpoint) ?? DEFAULT_WEBSITE_ORIGIN;
    return `${origin}/#stashpad-pro`;
}
