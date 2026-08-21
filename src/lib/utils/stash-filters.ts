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
import type { Attachment, StashItem } from "$lib/types";
import { getAttachmentKind, type AttachmentKind } from "./files";

/**
 * A single chip in the queue's attachment filter.
 *
 * "any"/"none" answer "does this stash carry files at all", the remaining values
 * narrow by {@link AttachmentKind}.
 */
export type AttachmentFilter = "any" | "none" | AttachmentKind;

/** All attachment filters, in the order the chips are rendered. */
export const ATTACHMENT_FILTERS: AttachmentFilter[] = [
    "any",
    "none",
    "image",
    "video",
    "text",
    "other",
];

/**
 * Attachments whose bytes still exist.
 *
 * Removing a file keeps its row so the deletion can sync, but blanks the path
 * (see `stashes.rs`). Those tombstones must not make a stash look like it still
 * has attachments.
 */
export function liveAttachments(item: StashItem): Attachment[] {
    return (item.attachments ?? []).filter((a) => a.filePath?.trim() !== "");
}

/** The distinct kinds of live attachment on a stash. */
export function attachmentKinds(item: StashItem): Set<AttachmentKind> {
    const kinds = new Set<AttachmentKind>();
    for (const attachment of liveAttachments(item)) {
        kinds.add(getAttachmentKind(attachment.fileName));
    }
    return kinds;
}

/**
 * Does a stash satisfy the selected attachment filters?
 *
 * OR logic within the group, matching how tag filters already behave: selecting
 * Images and Videos shows stashes carrying either. No selection matches everything.
 */
export function matchesAttachmentFilters(
    item: StashItem,
    filters: AttachmentFilter[],
): boolean {
    if (filters.length === 0) return true;

    const kinds = attachmentKinds(item);
    const hasAttachments = kinds.size > 0;

    return filters.some((filter) => {
        if (filter === "any") return hasAttachments;
        if (filter === "none") return !hasAttachments;
        return kinds.has(filter);
    });
}

/**
 * Does a stash carry any of the selected tags?
 *
 * Whole-word matching, so #bug does not match #bugfix. No selection matches
 * everything.
 */
export function matchesTagFilters(item: StashItem, tags: string[]): boolean {
    if (tags.length === 0) return true;
    return tags.some((tag) => new RegExp(`${tag}(?![\\w-])`).test(item.content));
}

/**
 * The queue's complete filter predicate: OR within each group, AND between them,
 * so #bug + Images means "tagged #bug *and* carrying an image".
 */
export function matchesFilters(
    item: StashItem,
    tags: string[],
    attachmentFilters: AttachmentFilter[],
): boolean {
    return (
        matchesTagFilters(item, tags) &&
        matchesAttachmentFilters(item, attachmentFilters)
    );
}

/**
 * How many of `items` each attachment filter would match.
 *
 * Only filters with at least one match are returned, so the queue never renders a
 * chip that would empty the list - except "none", which is meaningful precisely
 * when nothing matches "any".
 */
export function countAttachmentFilters(
    items: StashItem[],
): Map<AttachmentFilter, number> {
    const counts = new Map<AttachmentFilter, number>();
    for (const item of items) {
        for (const filter of ATTACHMENT_FILTERS) {
            if (matchesAttachmentFilters(item, [filter])) {
                counts.set(filter, (counts.get(filter) ?? 0) + 1);
            }
        }
    }
    return counts;
}
