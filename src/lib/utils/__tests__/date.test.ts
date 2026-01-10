// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025-2026 Nico Wiedemann
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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getRelativeTime } from '../date';

describe('date utilities', () => {
    describe('getRelativeTime', () => {
        let mockTranslate: ReturnType<typeof vi.fn>;
        let mockNow: Date;

        beforeEach(() => {
            // Mock translate function
            mockTranslate = vi.fn((key: string, options?: any) => {
                const count = options?.values?.count || 0;
                const translations: Record<string, string> = {
                    'contextSwitcher.time.justNow': 'just now',
                    'contextSwitcher.time.minute': `${count} minute ago`,
                    'contextSwitcher.time.minutes': `${count} minutes ago`,
                    'contextSwitcher.time.hour': `${count} hour ago`,
                    'contextSwitcher.time.hours': `${count} hours ago`,
                    'contextSwitcher.time.yesterday': 'yesterday',
                    'contextSwitcher.time.daysAgo': `${count} days ago`,
                };
                return translations[key] || key;
            });

            // Set consistent "now" time for testing
            mockNow = new Date('2026-01-08T12:00:00Z');
            vi.setSystemTime(mockNow);
        });

        it('should return empty string for empty dateString', () => {
            expect(getRelativeTime('', mockTranslate)).toBe('');
        });

        it('should return "just now" for timestamps less than 60 seconds ago', () => {
            const thirtySecondsAgo = new Date(mockNow.getTime() - 30 * 1000).toISOString();
            expect(getRelativeTime(thirtySecondsAgo, mockTranslate)).toBe('just now');
        });

        it('should return singular minute for 1 minute ago', () => {
            const oneMinuteAgo = new Date(mockNow.getTime() - 60 * 1000).toISOString();
            expect(getRelativeTime(oneMinuteAgo, mockTranslate)).toBe('1 minute ago');
        });

        it('should return plural minutes for multiple minutes ago', () => {
            const tenMinutesAgo = new Date(mockNow.getTime() - 10 * 60 * 1000).toISOString();
            expect(getRelativeTime(tenMinutesAgo, mockTranslate)).toBe('10 minutes ago');
        });

        it('should return singular hour for 1 hour ago', () => {
            const oneHourAgo = new Date(mockNow.getTime() - 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(oneHourAgo, mockTranslate)).toBe('1 hour ago');
        });

        it('should return plural hours for multiple hours ago', () => {
            const fiveHoursAgo = new Date(mockNow.getTime() - 5 * 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(fiveHoursAgo, mockTranslate)).toBe('5 hours ago');
        });

        it('should return "yesterday" for 1 day ago', () => {
            const oneDayAgo = new Date(mockNow.getTime() - 24 * 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(oneDayAgo, mockTranslate)).toBe('yesterday');
        });

        it('should return days ago for 2-6 days ago', () => {
            const threeDaysAgo = new Date(mockNow.getTime() - 3 * 24 * 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(threeDaysAgo, mockTranslate)).toBe('3 days ago');
        });

        it('should return formatted date (without year) for dates in same year but > 7 days ago', () => {
            // A date in the same year but more than 7 days ago
            const date = new Date('2026-01-01T12:00:00Z');
            const result = getRelativeTime(date.toISOString(), mockTranslate);
            // Should contain month and day but not year
            expect(result).toMatch(/Jan/);
            expect(result).not.toMatch(/2026/);
        });

        it('should return formatted date with year for dates in different year', () => {
            // A date in a different year
            const date = new Date('2025-12-01T12:00:00Z');
            const result = getRelativeTime(date.toISOString(), mockTranslate);
            // Should contain year - just check for the year itself
            expect(result).toContain('2025');
        });

        it('should handle edge case at exactly 60 seconds', () => {
            const sixtySecondsAgo = new Date(mockNow.getTime() - 60 * 1000).toISOString();
            expect(getRelativeTime(sixtySecondsAgo, mockTranslate)).toBe('1 minute ago');
        });

        it('should handle edge case at exactly 60 minutes', () => {
            const sixtyMinutesAgo = new Date(mockNow.getTime() - 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(sixtyMinutesAgo, mockTranslate)).toBe('1 hour ago');
        });

        it('should handle edge case at exactly 24 hours', () => {
            const twentyFourHoursAgo = new Date(mockNow.getTime() - 24 * 60 * 60 * 1000).toISOString();
            expect(getRelativeTime(twentyFourHoursAgo, mockTranslate)).toBe('yesterday');
        });
    });
});
