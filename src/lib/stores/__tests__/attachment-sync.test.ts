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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { AttachmentSyncQueue } from '../attachment-sync.svelte';
import type { IStorageService } from '$lib/types';

/** A download whose completion the test controls. */
function deferred<T>() {
    let resolve!: (v: T) => void;
    let reject!: (e: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
    });
    return { promise, resolve, reject };
}

describe('attachmentSync queue', () => {
    let attachmentSync: AttachmentSyncQueue;

    beforeEach(() => {
        // A fresh instance per test: the queue and its in-flight set are private, so a
        // leaked download would starve the concurrency slots of every later test.
        attachmentSync = new AttachmentSyncQueue();
        vi.restoreAllMocks();
    });

    it('treats an attachment with no local path as pending', () => {
        expect(attachmentSync.isPending('a1', '')).toBe(true);
        expect(attachmentSync.isPending('a1', '   ')).toBe(true);
        expect(attachmentSync.isPending('a1', '/cache/x.png')).toBe(false);
    });

    it('prefers a path resolved this session over the stored one', async () => {
        const adapter = {
            downloadAttachmentFromCloud: vi.fn().mockResolvedValue('/cache/c/s/x.png'),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        await attachmentSync.request('a1', '');

        expect(attachmentSync.pathFor('a1', '')).toBe('/cache/c/s/x.png');
        expect(attachmentSync.isPending('a1', '')).toBe(false);
    });

    it('resolves immediately when the file is already local, without downloading', async () => {
        const adapter = {
            downloadAttachmentFromCloud: vi.fn(),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        const path = await attachmentSync.request('a1', '/cache/have-it.png');

        expect(path).toBe('/cache/have-it.png');
        expect(adapter.downloadAttachmentFromCloud).not.toHaveBeenCalled();
    });

    it('runs a requested attachment before an existing backlog', async () => {
        // The whole point of the queue: what the user just opened should not wait
        // behind files they never asked for.
        const gates = new Map<string, ReturnType<typeof deferred<string>>>();
        const started: string[] = [];

        const adapter = {
            downloadAttachmentFromCloud: vi.fn((id: string) => {
                started.push(id);
                const d = deferred<string>();
                gates.set(id, d);
                return d.promise;
            }),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        // Backlog of five. Concurrency is 2, so 'bulk-0' and 'bulk-1' start immediately.
        attachmentSync.enqueue(['bulk-0', 'bulk-1', 'bulk-2', 'bulk-3', 'bulk-4']);
        expect(started).toEqual(['bulk-0', 'bulk-1']);

        // User opens one that is sitting at the back of the queue.
        void attachmentSync.request('bulk-4', '');

        // Free a slot; the requested file must take it, ahead of bulk-2.
        gates.get('bulk-0')!.resolve('/cache/bulk-0');
        await Promise.resolve();
        await Promise.resolve();

        expect(started[2]).toBe('bulk-4');
    });

    it('does not enqueue the same attachment twice', () => {
        const adapter = {
            downloadAttachmentFromCloud: vi.fn(() => deferred<string>().promise),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        attachmentSync.enqueue(['a1']);
        attachmentSync.enqueue(['a1']);
        attachmentSync.enqueue(['a1']);

        expect((adapter.downloadAttachmentFromCloud as any).mock.calls.length).toBe(1);
    });

    it('skips attachments already downloaded this session', async () => {
        const adapter = {
            downloadAttachmentFromCloud: vi.fn().mockResolvedValue('/cache/x.png'),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        await attachmentSync.request('a1', '');
        (adapter.downloadAttachmentFromCloud as any).mockClear();

        attachmentSync.enqueue(['a1']);

        expect(adapter.downloadAttachmentFromCloud).not.toHaveBeenCalled();
    });

    it('marks a failed download so the UI can offer a retry', async () => {
        const adapter = {
            downloadAttachmentFromCloud: vi.fn().mockRejectedValue(new Error('offline')),
        } as unknown as IStorageService;
        attachmentSync.setAdapter(adapter);

        await expect(attachmentSync.request('a1', '')).rejects.toThrow('offline');
        expect(attachmentSync.status('a1')).toBe('error');
        expect(attachmentSync.isPending('a1', '')).toBe(true);
    });

    it('retries an attachment that previously failed', async () => {
        const download = vi
            .fn()
            .mockRejectedValueOnce(new Error('offline'))
            .mockResolvedValueOnce('/cache/x.png');
        attachmentSync.setAdapter({
            downloadAttachmentFromCloud: download,
        } as unknown as IStorageService);

        await expect(attachmentSync.request('a1', '')).rejects.toThrow('offline');
        await expect(attachmentSync.request('a1', '')).resolves.toBe('/cache/x.png');

        expect(download).toHaveBeenCalledTimes(2);
    });
});
