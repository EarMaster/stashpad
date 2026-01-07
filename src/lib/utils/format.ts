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

/**
 * Attachment size limits in bytes.
 */
export const ATTACHMENT_SIZE_LIMITS = {
    /** Maximum size for a single attachment (20 MB) */
    MAX_SINGLE_FILE: 20 * 1024 * 1024,
    /** Maximum total size for all attachments in a stash (100 MB) */
    MAX_STASH_TOTAL: 100 * 1024 * 1024,
    /** Maximum total size for all attachments in a context (2 GB) */
    MAX_CONTEXT_TOTAL: 2 * 1024 * 1024 * 1024,
} as const;

/**
 * Format bytes as human readable string with locale-aware number formatting.
 *
 * @param bytes - The number of bytes to format
 * @param locale - The locale to use for number formatting (default: "en")
 * @returns A human readable string like "1.4 KB" or "2,5 MB"
 */
export function formatBytes(bytes: number, locale: string = "en"): string {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    const value = bytes / Math.pow(k, i);
    // Use locale-aware number formatting (e.g., 1.4 in EN, 1,4 in DE)
    const formattedValue = value.toLocaleString(locale, {
        minimumFractionDigits: 0,
        maximumFractionDigits: 1,
    });
    return formattedValue + " " + sizes[i];
}

/**
 * Calculate total size of attachments.
 *
 * @param attachments - Array of attachments with fileSize property
 * @returns Total size in bytes
 */
export function calculateTotalAttachmentSize(
    attachments: { fileSize: number }[],
): number {
    return attachments.reduce((sum, a) => sum + (a.fileSize || 0), 0);
}

/**
 * Check if adding a file would exceed size limits.
 *
 * @param fileSize - Size of the file to add
 * @param currentTotal - Current total size of attachments in stash
 * @returns Object indicating if limits are exceeded
 */
export function checkAttachmentSizeLimits(
    fileSize: number,
    currentTotal: number,
): {
    exceedsSingleLimit: boolean;
    exceedsStashLimit: boolean;
    singleLimitBytes: number;
    stashLimitBytes: number;
} {
    return {
        exceedsSingleLimit: fileSize > ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE,
        exceedsStashLimit:
            currentTotal + fileSize > ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL,
        singleLimitBytes: ATTACHMENT_SIZE_LIMITS.MAX_SINGLE_FILE,
        stashLimitBytes: ATTACHMENT_SIZE_LIMITS.MAX_STASH_TOTAL,
    };
}
