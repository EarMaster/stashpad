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

import { describe, it, expect } from 'vitest';
import {
    formatBytes,
    calculateTotalAttachmentSize,
    checkAttachmentSizeLimits,
    ATTACHMENT_SIZE_LIMITS,
} from '../format';

describe('format utilities', () => {
    describe('formatBytes', () => {
        it('should format 0 bytes', () => {
            expect(formatBytes(0)).toBe('0 B');
        });

        it('should format bytes correctly', () => {
            expect(formatBytes(500)).toBe('500 B');
            expect(formatBytes(1023)).toBe('1,023 B');
        });

        it('should format kilobytes correctly', () => {
            expect(formatBytes(1024)).toBe('1 KB');
            expect(formatBytes(1536)).toBe('1.5 KB');
            expect(formatBytes(2048)).toBe('2 KB');
        });

        it('should format megabytes correctly', () => {
            expect(formatBytes(1024 * 1024)).toBe('1 MB');
            expect(formatBytes(1024 * 1024 * 1.5)).toBe('1.5 MB');
            expect(formatBytes(1024 * 1024 * 20)).toBe('20 MB');
        });

        it('should format gigabytes correctly', () => {
            expect(formatBytes(1024 * 1024 * 1024)).toBe('1 GB');
            expect(formatBytes(1024 * 1024 * 1024 * 2.5)).toBe('2.5 GB');
        });

        it('should use locale-aware formatting (German)', () => {
            const result = formatBytes(1536, 'de');
            // In German locale, decimal separator is comma
            expect(result).toBe('1,5 KB');
        });

        it('should round to 1 decimal place', () => {
            expect(formatBytes(1024 * 1.456)).toBe('1.5 KB');
            expect(formatBytes(1024 * 1.123)).toBe('1.1 KB');
        });

        it('should handle very large files', () => {
            const result = formatBytes(1024 * 1024 * 1024 * 100);
            expect(result).toContain('GB');
        });
    });

    describe('calculateTotalAttachmentSize', () => {
        it('should return 0 for empty array', () => {
            expect(calculateTotalAttachmentSize([])).toBe(0);
        });

        it('should calculate total size correctly', () => {
            const attachments = [
                { fileSize: 1024 },
                { fileSize: 2048 },
                { fileSize: 512 },
            ];
            expect(calculateTotalAttachmentSize(attachments)).toBe(3584);
        });

        it('should handle missing fileSize properties', () => {
            const attachments = [
                { fileSize: 1024 },
                { fileSize: 0 },
                {} as any, // Missing fileSize
            ];
            expect(calculateTotalAttachmentSize(attachments)).toBe(1024);
        });

        it('should handle single attachment', () => {
            const attachments = [{ fileSize: 5000 }];
            expect(calculateTotalAttachmentSize(attachments)).toBe(5000);
        });
    });

    describe('checkAttachmentSizeLimits', () => {
        it('should pass when file is within all limits', () => {
            const result = checkAttachmentSizeLimits(1024 * 1024, 0); // 1 MB file, 0 current
            expect(result.exceedsSingleLimit).toBe(false);
            expect(result.exceedsStashLimit).toBe(false);
        });

        it('should detect when single file exceeds limit', () => {
            const result = checkAttachmentSizeLimits(
                ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE + 1,
                0
            );
            expect(result.exceedsSingleLimit).toBe(true);
        });

        it('should detect when stash total would exceed limit', () => {
            const currentTotal = ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL - 1024;
            const result = checkAttachmentSizeLimits(2048, currentTotal);
            expect(result.exceedsStashLimit).toBe(true);
        });

        it('should allow file at exact single limit', () => {
            const result = checkAttachmentSizeLimits(
                ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE,
                0
            );
            expect(result.exceedsSingleLimit).toBe(false);
        });

        it('should allow stash total at exact limit', () => {
            const fileSize = 1024;
            const currentTotal = ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL - fileSize;
            const result = checkAttachmentSizeLimits(fileSize, currentTotal);
            expect(result.exceedsStashLimit).toBe(false);
        });

        it('should return limit values', () => {
            const result = checkAttachmentSizeLimits(0, 0);
            expect(result.singleLimitBytes).toBe(ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE);
            expect(result.stashLimitBytes).toBe(ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL);
        });
    });

    describe('ATTACHMENT_SIZE_LIMITS', () => {
        it('should define single file limit as 20 MB', () => {
            expect(ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE).toBe(20 * 1024 * 1024);
        });

        it('should define stash total limit as 100 MB', () => {
            expect(ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL).toBe(100 * 1024 * 1024);
        });

        it('should define context total limit as 2 GB', () => {
            expect(ATTACHMENT_SIZE_LIMITS.MAX_CONTEXT_TOTAL).toBe(2 * 1024 * 1024 * 1024);
        });
    });
});
