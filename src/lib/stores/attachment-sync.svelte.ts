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

/**
 * Download queue for attachments that exist as metadata but whose bytes are not on
 * this device yet.
 *
 * An attachment synced from another device arrives with an empty `filePath`: the row is
 * real, the file is not. Rather than blocking the sync cycle on what could be many
 * large downloads, sync enqueues them here and the queue drains in the background so
 * the UI can show each one as pending.
 *
 * The queue is priority-aware. Whatever the user is actually looking at should not wait
 * behind a backlog, so opening a pending attachment moves it to the front.
 */

import type { IStorageService } from '../types';

/** How many downloads run at once. Small, so a prioritised item starts promptly. */
const MAX_CONCURRENT = 2;

/** First retry delay after a failed download; doubles with each further failure. */
const RETRY_BASE_MS = 30_000;

/** Ceiling on the retry delay, so a broken attachment settles into a slow poll. */
const RETRY_MAX_MS = 15 * 60 * 1000;

export type AttachmentSyncStatus = 'queued' | 'downloading' | 'error';

/**
 * Exported so tests can work against a fresh instance. Application code should use the
 * `attachmentSync` singleton below.
 */
export class AttachmentSyncQueue {
    private adapter: IStorageService | null = null;
    private queue: string[] = [];
    private active = new Set<string>();
    private waiters = new Map<string, Array<{ resolve: (path: string) => void; reject: (e: unknown) => void }>>();
    /** Consecutive failures per attachment, used to grow the retry delay. */
    private failures = new Map<string, number>();
    /** Earliest time (epoch ms) a failed attachment may be retried. */
    private retryAfter: Record<string, number> = {};

    /** Status per attachment id. Absent means nothing in flight. */
    statuses = $state<Record<string, AttachmentSyncStatus>>({});

    /**
     * Paths resolved during this session, keyed by attachment id.
     *
     * Lets a chip flip from pending to ready immediately, without waiting for the next
     * full reload of the stash list.
     */
    resolved = $state<Record<string, string>>({});

    /** Wire up the storage adapter. Called once at app start. */
    setAdapter(adapter: IStorageService): void {
        this.adapter = adapter;
    }

    /** Current status of an attachment, or undefined when it is not being fetched. */
    status(id: string): AttachmentSyncStatus | undefined {
        return this.statuses[id];
    }

    /**
     * Resolve an attachment's usable local path: the one downloaded this session if we
     * have it, otherwise whatever the database recorded.
     */
    pathFor(id: string, storedPath: string): string {
        return this.resolved[id] || storedPath || '';
    }

    /** True when the bytes are not available locally yet. */
    isPending(id: string, storedPath: string): boolean {
        return this.pathFor(id, storedPath).trim() === '';
    }

    /** Queue any attachment we do not already hold. Ignores ones already in flight. */
    enqueue(ids: string[]): void {
        let added = false;
        const next = { ...this.statuses };
        const now = Date.now();

        for (const id of ids) {
            if (this.resolved[id] || this.active.has(id)) continue;
            if (this.queue.includes(id)) continue;

            const status = this.statuses[id];
            if (status === 'queued' || status === 'downloading') continue;

            // A previous failure is not permanent. Skipping anything with *any* status
            // made 'error' terminal for the session: a transient offline blip during one
            // download pass poisoned those attachments until the app restarted or the
            // user clicked each one. Retry once the backoff has elapsed instead, growing
            // the delay so a genuinely broken attachment is not hammered.
            if (status === 'error' && now < (this.retryAfter[id] ?? 0)) continue;

            this.queue.push(id);
            next[id] = 'queued';
            added = true;
        }

        if (added) {
            this.statuses = next;
            void this.drain();
        }
    }

    /**
     * Move an attachment to the front of the queue and resolve when its bytes land.
     *
     * Used when the user opens a pending attachment: their file should not sit behind
     * a backlog of ones they have not asked for.
     */
    request(id: string, storedPath: string): Promise<string> {
        const existing = this.pathFor(id, storedPath);
        if (existing.trim() !== '') return Promise.resolve(existing);

        // A previous attempt failed; clear the error so this one is retried.
        if (this.statuses[id] === 'error') {
            const { [id]: _discarded, ...rest } = this.statuses;
            this.statuses = rest;
            // An explicit request bypasses the backoff entirely - the user is waiting.
            this.failures.delete(id);
            const { [id]: _discardedRetry, ...remainingRetries } = this.retryAfter;
            this.retryAfter = remainingRetries;
        }

        if (!this.queue.includes(id) && !this.active.has(id)) {
            this.queue.unshift(id);
            this.statuses = { ...this.statuses, [id]: 'queued' };
        } else {
            const index = this.queue.indexOf(id);
            if (index > 0) {
                this.queue.splice(index, 1);
                this.queue.unshift(id);
            }
        }

        const promise = new Promise<string>((resolve, reject) => {
            const list = this.waiters.get(id) ?? [];
            list.push({ resolve, reject });
            this.waiters.set(id, list);
        });

        void this.drain();
        return promise;
    }

    /** Start downloads up to the concurrency limit. */
    private async drain(): Promise<void> {
        while (this.active.size < MAX_CONCURRENT && this.queue.length > 0) {
            const id = this.queue.shift();
            if (!id) break;
            void this.run(id);
        }
    }

    private async run(id: string): Promise<void> {
        if (!this.adapter) {
            this.fail(id, new Error('Storage adapter not configured'));
            return;
        }

        this.active.add(id);
        this.statuses = { ...this.statuses, [id]: 'downloading' };

        try {
            const path = await this.adapter.downloadAttachmentFromCloud(id);
            this.resolved = { ...this.resolved, [id]: path };

            // Reset the backoff so a later failure starts from the short delay again.
            this.failures.delete(id);
            const { [id]: _discardedRetry, ...remainingRetries } = this.retryAfter;
            this.retryAfter = remainingRetries;

            const { [id]: _discarded, ...rest } = this.statuses;
            this.statuses = rest;

            this.waiters.get(id)?.forEach(w => w.resolve(path));
            this.waiters.delete(id);
        } catch (e) {
            console.warn(`[AttachmentSync] Download failed for ${id}:`, e);
            this.fail(id, e);
        } finally {
            this.active.delete(id);
            void this.drain();
        }
    }

    private fail(id: string, error: unknown): void {
        // Exponential backoff, capped. A transient failure retries within seconds; one
        // that keeps failing settles at a slow poll rather than re-downloading the file
        // on every sync forever.
        const attempts = (this.failures.get(id) ?? 0) + 1;
        this.failures.set(id, attempts);
        const delay = Math.min(RETRY_BASE_MS * 2 ** (attempts - 1), RETRY_MAX_MS);
        this.retryAfter = { ...this.retryAfter, [id]: Date.now() + delay };

        this.statuses = { ...this.statuses, [id]: 'error' };
        this.waiters.get(id)?.forEach(w => w.reject(error));
        this.waiters.delete(id);
    }
}

export const attachmentSync = new AttachmentSyncQueue();
