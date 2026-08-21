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
import {
    DEFAULT_WEBSITE_ORIGIN,
    waitlistUrl,
    websiteOriginFromEndpoint,
} from '../cloud-urls';

describe('cloud urls', () => {
    describe('websiteOriginFromEndpoint', () => {
        it('strips the api subdomain', () => {
            expect(websiteOriginFromEndpoint('https://api.stashpad.org')).toBe(
                'https://stashpad.org',
            );
        });

        it('ignores any path on the endpoint', () => {
            expect(websiteOriginFromEndpoint('https://api.stashpad.org/v1')).toBe(
                'https://stashpad.org',
            );
        });

        it('leaves a self-hosted host without the prefix alone, port included', () => {
            expect(websiteOriginFromEndpoint('http://localhost:3000')).toBe(
                'http://localhost:3000',
            );
        });

        it('only strips a leading api label, not one in the middle', () => {
            expect(websiteOriginFromEndpoint('https://my.api.example.com')).toBe(
                'https://my.api.example.com',
            );
        });

        it('returns null for a missing or unparseable endpoint', () => {
            expect(websiteOriginFromEndpoint(undefined)).toBeNull();
            expect(websiteOriginFromEndpoint('')).toBeNull();
            expect(websiteOriginFromEndpoint('not a url')).toBeNull();
        });
    });

    describe('waitlistUrl', () => {
        it('anchors the pricing section on the site the endpoint belongs to', () => {
            expect(waitlistUrl('https://api.stashpad.org')).toBe(
                'https://stashpad.org/#stashpad-pro',
            );
        });

        it('falls back to the public site when the endpoint is unusable', () => {
            expect(waitlistUrl(undefined)).toBe(
                `${DEFAULT_WEBSITE_ORIGIN}/#stashpad-pro`,
            );
        });
    });
});
