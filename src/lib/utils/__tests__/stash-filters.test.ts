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
import type { Attachment, StashItem } from '../../types';
import {
    attachmentKinds,
    countAttachmentFilters,
    liveAttachments,
    matchesAttachmentFilters,
    matchesFilters,
    matchesTagFilters,
} from '../stash-filters';

function attachment(fileName: string, filePath?: string): Attachment {
    return {
        id: `att-${fileName}`,
        stashId: 'stash-1',
        filePath: filePath ?? `/cache/ctx/stash-1/${fileName}`,
        fileName,
        fileSize: 1024,
        createdAt: '2026-01-01T00:00:00.000Z',
    };
}

function stash(content: string, attachments: Attachment[] = []): StashItem {
    return {
        id: 'stash-1',
        content,
        files: [],
        attachments,
        createdAt: '2026-01-01T00:00:00.000Z',
        contextId: 'ctx',
    };
}

const plain = stash('just a note');
const withImage = stash('a screenshot', [attachment('shot.png')]);
const withCode = stash('a log', [attachment('trace_123.txt')]);
const withVideo = stash('a clip', [attachment('repro.mp4')]);
const withBinary = stash('a build', [attachment('app.zip')]);
const withImageAndCode = stash('both', [
    attachment('shot.png'),
    attachment('trace_123.txt'),
]);

describe('stash filters', () => {
    describe('liveAttachments', () => {
        it('keeps attachments that still have a path', () => {
            expect(liveAttachments(withImage)).toHaveLength(1);
        });

        it('drops tombstoned attachments, which keep their row but lose the path', () => {
            const tombstoned = stash('removed', [attachment('gone.png', '')]);
            expect(liveAttachments(tombstoned)).toEqual([]);
        });

        it('tolerates a stash with no attachments array', () => {
            const bare = { ...plain, attachments: undefined } as unknown as StashItem;
            expect(liveAttachments(bare)).toEqual([]);
        });
    });

    describe('attachmentKinds', () => {
        it('classifies by extension', () => {
            expect(attachmentKinds(withImage)).toEqual(new Set(['image']));
            expect(attachmentKinds(withVideo)).toEqual(new Set(['video']));
            expect(attachmentKinds(withCode)).toEqual(new Set(['text']));
            expect(attachmentKinds(withBinary)).toEqual(new Set(['other']));
        });

        it('collects every distinct kind on a stash', () => {
            expect(attachmentKinds(withImageAndCode)).toEqual(
                new Set(['image', 'text']),
            );
        });
    });

    describe('matchesAttachmentFilters', () => {
        it('matches everything when nothing is selected', () => {
            expect(matchesAttachmentFilters(plain, [])).toBe(true);
            expect(matchesAttachmentFilters(withImage, [])).toBe(true);
        });

        it('any matches stashes that carry files', () => {
            expect(matchesAttachmentFilters(withImage, ['any'])).toBe(true);
            expect(matchesAttachmentFilters(plain, ['any'])).toBe(false);
        });

        it('none matches stashes without files', () => {
            expect(matchesAttachmentFilters(plain, ['none'])).toBe(true);
            expect(matchesAttachmentFilters(withImage, ['none'])).toBe(false);
        });

        it('treats a stash whose only attachment was removed as having none', () => {
            const tombstoned = stash('removed', [attachment('gone.png', '')]);
            expect(matchesAttachmentFilters(tombstoned, ['none'])).toBe(true);
            expect(matchesAttachmentFilters(tombstoned, ['any'])).toBe(false);
            expect(matchesAttachmentFilters(tombstoned, ['image'])).toBe(false);
        });

        it('matches a single kind', () => {
            expect(matchesAttachmentFilters(withImage, ['image'])).toBe(true);
            expect(matchesAttachmentFilters(withImage, ['video'])).toBe(false);
            expect(matchesAttachmentFilters(withCode, ['text'])).toBe(true);
            expect(matchesAttachmentFilters(withBinary, ['other'])).toBe(true);
        });

        it('ORs several kinds together', () => {
            expect(matchesAttachmentFilters(withImage, ['image', 'video'])).toBe(true);
            expect(matchesAttachmentFilters(withVideo, ['image', 'video'])).toBe(true);
            expect(matchesAttachmentFilters(withCode, ['image', 'video'])).toBe(false);
        });

        it('matches a stash on any one of its kinds', () => {
            expect(matchesAttachmentFilters(withImageAndCode, ['text'])).toBe(true);
            expect(matchesAttachmentFilters(withImageAndCode, ['image'])).toBe(true);
            expect(matchesAttachmentFilters(withImageAndCode, ['video'])).toBe(false);
        });

        it('matches everything when any and none are both selected', () => {
            expect(matchesAttachmentFilters(plain, ['any', 'none'])).toBe(true);
            expect(matchesAttachmentFilters(withImage, ['any', 'none'])).toBe(true);
        });
    });

    describe('matchesTagFilters', () => {
        it('matches everything when nothing is selected', () => {
            expect(matchesTagFilters(stash('#bug'), [])).toBe(true);
        });

        it('matches whole tags only', () => {
            expect(matchesTagFilters(stash('a #bug here'), ['#bug'])).toBe(true);
            expect(matchesTagFilters(stash('a #bugfix here'), ['#bug'])).toBe(false);
        });

        it('ORs the selected tags', () => {
            expect(
                matchesTagFilters(stash('a #feature'), ['#bug', '#feature']),
            ).toBe(true);
        });
    });

    describe('matchesFilters', () => {
        it('ANDs the tag group with the attachment group', () => {
            const tagged = stash('#bug crash', [attachment('shot.png')]);
            expect(matchesFilters(tagged, ['#bug'], ['image'])).toBe(true);
            expect(matchesFilters(tagged, ['#bug'], ['video'])).toBe(false);
            expect(matchesFilters(tagged, ['#feature'], ['image'])).toBe(false);
        });

        it('falls back to everything when both groups are empty', () => {
            expect(matchesFilters(plain, [], [])).toBe(true);
        });
    });

    describe('countAttachmentFilters', () => {
        it('counts each filter across the list and omits filters with no match', () => {
            const counts = countAttachmentFilters([plain, withImage, withImageAndCode]);
            expect(counts.get('any')).toBe(2);
            expect(counts.get('none')).toBe(1);
            expect(counts.get('image')).toBe(2);
            expect(counts.get('text')).toBe(1);
            expect(counts.has('video')).toBe(false);
            expect(counts.has('other')).toBe(false);
        });

        it('reports only none when nothing has attachments', () => {
            const counts = countAttachmentFilters([plain, stash('another')]);
            expect(counts.get('none')).toBe(2);
            expect(counts.has('any')).toBe(false);
        });
    });
});
