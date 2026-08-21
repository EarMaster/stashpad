// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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
 * CloudSyncService - Manages background synchronization with Stashpad Cloud
 *
 * Sync strategy:
 * - Real-time: a WebSocket notification from another device triggers an immediate sync
 * - On-demand: any local mutation schedules a debounced sync (see `triggerSync`)
 * - Fallback: a periodic poll in case the socket is down
 * - Conflict resolution: Last-Write-Wins on `updatedAt`
 *
 * Timestamp units are a recurring trap here. Locally `updatedAt` is **Unix seconds**;
 * the cloud API speaks **ISO-8601 strings**. Every conversion in this file is explicit,
 * and comparisons are done at second granularity so a round-trip through the server
 * cannot make a record look spuriously newer than its local copy.
 */

import type { IStorageService, StashItem, Context, CloudConfig, Settings, StashPosition } from '../types';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { attachmentSync } from '../stores/attachment-sync.svelte';

// Fallback polling interval, used when the WebSocket is unavailable.
const FALLBACK_SYNC_INTERVAL_MS = 15 * 60 * 1000;
const DEBOUNCE_DELAY_MS = 2000;

/**
 * Floor on how often a WebSocket notification may start a sync.
 *
 * A remote notification is a hint that something changed, not an instruction to sync
 * immediately. Without a floor, two devices notified by each other's syncs drove one
 * another in a permanent loop, and the constant import work made the app unresponsive.
 * The server no longer announces no-op requests, but this keeps a single misbehaving or
 * older server from being able to do it again.
 */
const REMOTE_SYNC_MIN_INTERVAL_MS = 5000;

/** First retry delay after a failed attachment upload; doubles with each failure. */
const UPLOAD_RETRY_BASE_MS = 60_000;

/** Ceiling on the upload retry delay. */
const UPLOAD_RETRY_MAX_MS = 30 * 60 * 1000;

/**
 * How many attachments one sync cycle will attempt.
 *
 * Uploads run one at a time and each is bounded only by the 300 s transfer timeout, so
 * a backlog of stalled files could hold `isSyncing` - and therefore stash and context
 * sync too - for hours, with the header stuck on "syncing" the whole time. Whatever is
 * left over goes out on the next cycle instead.
 */
const MAX_UPLOADS_PER_CYCLE = 10;

/**
 * Wall-clock ceiling on the attachment phase of a single sync.
 *
 * A cap on the count alone is not enough: ten files that each take the full transfer
 * timeout is still nearly an hour. Once this elapses the phase stops and the remainder
 * is picked up next cycle.
 */
const ATTACHMENT_PHASE_BUDGET_MS = 2 * 60 * 1000;

/** Sync status for UI feedback */
export type SyncStatus = 'idle' | 'syncing' | 'success' | 'error' | 'offline' | 'auth-error';

/** Sync event listener */
/**
 * `appliedRemoteChanges` says whether the sync actually wrote remote data into the local
 * database. The UI reloads its whole stash list when it hears 'success', which is far too
 * expensive to do after a sync that pulled nothing — and most syncs pull nothing.
 */
export type SyncListener = (
    status: SyncStatus,
    message?: string,
    appliedRemoteChanges?: boolean
) => void;

/** Attachment metadata exchanged with the cloud API */
interface SyncAttachmentInput {
    id: string;
    fileName: string;
    fileSize: number;
    mimeType: string | null;
    syntax: string | null;
}

/** Stash format expected by the cloud API */
interface SyncStashInput {
    id: string;
    contextId: string | null;
    content: string;
    enhancedContent: string | null;
    completed: boolean;
    completedAt: string | null;
    createdAt: string;
    updatedAt: string;
    deleted: boolean;
    attachments: SyncAttachmentInput[];
}

/** Cloud sync request payload */
interface SyncRequest {
    deviceId: string;
    deviceName: string | null;
    lastSyncAt: string | null;
    stashes: SyncStashInput[];
    /** Orderings this device changed, carried apart from the records. */
    positions: StashPosition[];
}

/** A record the server refused to apply, with the reason why */
interface RejectedRecord {
    id: string;
    reason: string;
}

/** Cloud sync response */
interface SyncResponse {
    synced: StashItem[];
    serverTime: string;
    rejected?: RejectedRecord[];
    /** True when `synced` only contains records changed since `lastSyncAt` */
    partial?: boolean;
    /** Orderings other devices changed since the cursor. */
    positions?: StashPosition[];
}

/** Context format for cloud API */
interface SyncContextInput {
    id: string;
    name: string;
    description: string | null;
    rules: unknown[];
    lastUsed: string | null;
    updatedAt: string;
    deleted: boolean;
}

/** Context sync request */
interface ContextSyncRequest {
    deviceId: string;
    deviceName: string | null;
    lastSyncAt: string | null;
    contexts: SyncContextInput[];
}

/** Cloud context from server */
interface SyncContext {
    id: string;
    name: string;
    description: string | null;
    rules: unknown[];
    lastUsed: string | null;
    updatedAt: string;
    deletedAt?: string | null;
}

/** Context sync response */
interface ContextSyncResponse {
    synced: SyncContext[];
    serverTime: string;
    rejected?: RejectedRecord[];
    partial?: boolean;
}

/** Subscription tiers entitled to cloud sync */
const SYNC_ENTITLED_TIERS = ['pro', 'enterprise'];

/**
 * Does the server know about attachments this device does not have?
 *
 * Deliberately one-directional. The server withholds attachments whose bytes are not
 * confirmed in storage yet, so a freshly added local attachment is legitimately absent
 * from the server's copy of the same stash. Treating "the server has fewer" as a change
 * worth importing destroys attachments the user just added, before they are ever
 * uploaded.
 *
 * Attachments therefore only ever arrive through a merge, never disappear through one;
 * local removal happens solely by explicit user action.
 */
function hasUnknownAttachments(
    serverStash: { attachments?: Array<{ id: string }> },
    localStash: { attachments?: Array<{ id: string }> }
): boolean {
    const localIds = new Set((localStash.attachments || []).map(a => a.id));
    return (serverStash.attachments || []).some(a => !localIds.has(a.id));
}

/**
 * Union of the local and server attachment lists, keyed by id.
 *
 * The local entry wins on conflict because it carries `filePath`, which the server has
 * no concept of and never sends back. Taking the server's entry would blank the path and
 * strand the file the device already holds.
 */
function mergeAttachments(
    localStash: { attachments?: StashItem['attachments'] },
    serverStash: { attachments?: StashItem['attachments'] }
): StashItem['attachments'] {
    const merged = new Map<string, StashItem['attachments'][number]>();
    for (const att of serverStash.attachments || []) merged.set(att.id, att);
    for (const att of localStash.attachments || []) merged.set(att.id, att);
    return [...merged.values()];
}

/**
 * Convert a local `updatedAt` (Unix seconds, or an ISO string on legacy records) to
 * milliseconds for comparison. Returns 0 when nothing usable is present.
 */
function localTimeToMs(value: string | number | undefined | null, fallback?: string): number {
    if (typeof value === 'number') return value * 1000;
    if (typeof value === 'string' && value.trim() !== '') {
        const parsed = new Date(value).getTime();
        if (!Number.isNaN(parsed)) return parsed;
    }
    if (fallback) {
        const parsed = new Date(fallback).getTime();
        if (!Number.isNaN(parsed)) return parsed;
    }
    return 0;
}

/**
 * Truncate to whole seconds.
 *
 * The local DB stores seconds while the server keeps sub-second precision. Comparing
 * the two directly made every server record look newer than its identical local copy,
 * so each sync re-imported everything it had just pushed.
 */
function toSeconds(ms: number): number {
    return Math.floor(ms / 1000);
}

/**
 * CloudSyncService manages automatic data synchronization
 */
/**
 * Get or create the identifier this installation is known by on the server.
 *
 * Shared with the account linking flow: the id is sent when a link code is exchanged
 * so the server can tie the token it issues to this installation, which is what lets
 * the account page revoke this one instance without touching the others. Linking and
 * syncing must therefore agree on the value, so both read it from here.
 */
export function getOrCreateDeviceId(): string {
    const key = 'stashpad_device_id';
    let deviceId = localStorage.getItem(key);
    if (!deviceId) {
        deviceId = crypto.randomUUID();
        localStorage.setItem(key, deviceId);
    }
    return deviceId;
}

export class CloudSyncService {
    private adapter: IStorageService;
    private settings: Settings | null = null;
    private syncInterval: ReturnType<typeof setInterval> | null = null;
    private debounceTimer: ReturnType<typeof setTimeout> | null = null;
    private listeners: Set<SyncListener> = new Set();
    private status: SyncStatus = 'idle';
    private deviceId: string;
    private deviceName: string | null = null;
    private isSyncing = false;
    private wsUnlisten: UnlistenFn | null = null;
    private initialized = false;
    /** Timer coalescing a burst of remote notifications into a single sync. */
    private remoteSyncTimer: ReturnType<typeof setTimeout> | null = null;
    /** When the last notification-driven sync started, for the rate floor. */
    private lastRemoteSyncAt = 0;
    /**
     * Last value `shouldSync()` returned.
     *
     * Held as a primitive on purpose. `updateSettings` used to derive the "before"
     * state by calling `shouldSync()` against `this.settings` — but callers pass the
     * same Svelte `$state` proxy every time and mutate it in place, so by the time the
     * effect re-ran, `this.settings` already reflected the new value. Before and after
     * were therefore always equal, the false -> true transition never fired, and sync
     * simply never started after an in-session login.
     */
    private lastShouldSync = false;
    /**
     * Most recent attachment upload failure, if the last sync had one.
     *
     * Attachment uploads are best-effort - one failing must not abort the sync - but
     * "best effort" previously meant a `console.warn` and nothing else, so uploads could
     * fail on every cycle while the UI reported success. Reported alongside the sync
     * status instead.
     */
    private lastAttachmentError: string | null = null;
    /** Consecutive upload failures per attachment, used to grow the retry delay. */
    private attachmentFailures = new Map<string, number>();
    /** Earliest time (epoch ms) a failed upload may be retried. */
    private attachmentRetryAfter = new Map<string, number>();

    constructor(adapter: IStorageService) {
        this.adapter = adapter;
        this.deviceId = this.getOrCreateDeviceId();
    }

    /** The identifier this installation is known by on the server. */
    getDeviceId(): string {
        return this.deviceId;
    }

    /**
     * Get or create a persistent device ID for sync tracking
     */
    private getOrCreateDeviceId(): string {
        return getOrCreateDeviceId();
    }

    /**
     * Initialize the sync service with current settings
     */
    async initialize(settings: Settings): Promise<void> {
        this.settings = settings;
        if (!this.deviceName) {
            try {
                this.deviceName = await this.adapter.getDeviceName();
            } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                console.error('Failed to get device name:', msg);
                this.deviceName = 'Unknown Device';
            }
        }

        // Entitlement must be known before the gate below is evaluated, otherwise a
        // device linked via the code flow never syncs until someone opens Settings.
        await this.refreshEntitlement();

        this.lastShouldSync = this.shouldSync();
        this.initialized = true;

        if (this.lastShouldSync) {
            this.startPeriodicSync();
            // Do an initial sync on startup
            await this.sync();
        }
    }

    /**
     * Refresh the cached subscription tier from the cloud.
     *
     * `shouldSync()` gates on `subscriptionTier`, which was only ever populated by the
     * Settings screen. A device linked without visiting it kept the tier `undefined`
     * and silently never synced — no error, no status change.
     *
     * A failed fetch leaves the cached tier untouched rather than clearing it: an
     * offline start must not look like a downgrade.
     */
    private async refreshEntitlement(): Promise<void> {
        const config = this.settings?.cloudConfig;
        if (!config?.enabled || !config.userId) return;

        try {
            const fresh = await this.adapter.fetchCloudAccount();
            // A malformed or empty response must not overwrite a working config -
            // spreading `undefined` here would silently strip `enabled` and the tier and
            // disable sync until the next restart.
            if (!fresh || typeof fresh !== 'object') {
                console.warn('[CloudSync] Ignoring empty account response');
                return;
            }
            if (this.settings) {
                // Preserve the local sync cursor; the account endpoint doesn't know it.
                this.settings.cloudConfig = { ...fresh, lastSyncAt: config.lastSyncAt };
                await this.adapter.saveSettings(this.settings);
            }
        } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            if (this.isAuthError(msg)) {
                this.setStatus('auth-error', 'Authentication expired. Please log in again.');
                return;
            }
            console.warn('[CloudSync] Could not refresh subscription tier:', msg);
        }
    }

    /**
     * Update settings and start/stop sync when entitlement changes
     */
    updateSettings(settings: Settings): void {
        this.settings = settings;
        const isEnabled = this.shouldSync();
        const wasEnabled = this.lastShouldSync;
        this.lastShouldSync = isEnabled;

        if (isEnabled && !wasEnabled) {
            this.startPeriodicSync();
            void this.sync(); // Trigger immediate sync upon enabling
        } else if (!isEnabled && wasEnabled) {
            this.stopPeriodicSync();
        }
    }

    /**
     * Called after a successful login so sync starts without waiting for a restart.
     */
    async onAuthenticated(settings: Settings): Promise<void> {
        this.settings = settings;
        await this.refreshEntitlement();
        this.updateSettings(this.settings);
        if (!this.shouldSync()) {
            this.setStatus(
                'error',
                'This account is not entitled to cloud sync. Check your subscription.'
            );
        }
    }

    /**
     * Check if sync should be active
     */
    private shouldSync(): boolean {
        if (!this.settings?.cloudConfig) return false;
        const config = this.settings.cloudConfig;

        return (
            config.enabled &&
            (SYNC_ENTITLED_TIERS.includes(config.subscriptionTier ?? '') ||
                config.enterpriseOwnerId != null)
        );
    }

    /**
     * Start periodic background sync
     */
    private startPeriodicSync(): void {
        if (this.syncInterval) return;

        // Start periodic sync (now functions as a fallback)
        this.syncInterval = setInterval(() => {
            void this.sync();
        }, FALLBACK_SYNC_INTERVAL_MS);

        // Connect to WebSocket for real-time sync notifications
        if (this.settings?.cloudConfig?.enabled) {
            void this.connectWebSocket();
        }

        console.log('[CloudSync] Periodic sync started');
    }

    /**
     * Stop periodic background sync
     */
    private stopPeriodicSync(): void {
        if (this.syncInterval) {
            clearInterval(this.syncInterval);
            this.syncInterval = null;
            console.log('[CloudSync] Periodic sync stopped');
        }
        void this.disconnectWebSocket();
    }

    /**
     * Trigger a debounced sync. Call after *any* local mutation.
     */
    triggerSync(): void {
        if (!this.shouldSync()) return;

        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }

        this.debounceTimer = setTimeout(() => {
            void this.sync();
        }, DEBOUNCE_DELAY_MS);
    }

    /**
     * Schedule a sync in response to a remote notification.
     *
     * Kept separate from `triggerSync` because the two have different failure modes. A
     * local mutation is trusted and only needs debouncing; a remote notification arrives
     * from another device and must additionally be rate-limited, or a pair of devices
     * notified by each other's syncs will drive one another without pause.
     *
     * A burst collapses into one sync, and consecutive syncs are held to
     * `REMOTE_SYNC_MIN_INTERVAL_MS` — a notification arriving inside that window is not
     * dropped but deferred to the end of it, so nothing is missed.
     */
    private scheduleRemoteSync(): void {
        if (!this.shouldSync()) return;

        // A burst is already pending; the sync it will run covers this notification too.
        if (this.remoteSyncTimer) return;

        const sinceLast = Date.now() - this.lastRemoteSyncAt;
        const wait = Math.max(DEBOUNCE_DELAY_MS, REMOTE_SYNC_MIN_INTERVAL_MS - sinceLast);

        this.remoteSyncTimer = setTimeout(() => {
            this.remoteSyncTimer = null;
            this.lastRemoteSyncAt = Date.now();
            void this.sync();
        }, wait);
    }

    /**
     * Perform a full sync with the cloud
     */
    async sync(): Promise<boolean> {
        if (!this.shouldSync() || this.isSyncing) {
            return false;
        }

        const config = this.settings!.cloudConfig!;

        this.isSyncing = true;
        this.setStatus('syncing');

        try {
            // Load local data
            const [localStashes, localContexts] = await Promise.all([
                this.adapter.loadStashesForSync(),
                this.adapter.getContextsForSync(),
            ]);

            // Push only what changed. Without a sync cursor this device has never
            // successfully synced, so send everything once to give the server a complete
            // picture - the same reason every row starts out flagged as pending.
            // Claiming marks these records in flight, so an edit made while the request
            // is running stays queued instead of being acknowledged away.
            const fullPush = !config.lastSyncAt;
            const [pushStashes, pushContexts] = fullPush
                ? [localStashes, localContexts]
                : await Promise.all([
                      this.adapter.claimPendingStashes(),
                      this.adapter.claimPendingContexts(),
                  ]);

            // Orderings are claimed separately from the records: a reorder must not drag
            // the record's content along, or it can overwrite an edit made elsewhere.
            const pushPositions = await this.adapter.claimPendingPositions();

            const sentStashIds = pushStashes.map(s => s.id);
            const sentContextIds = pushContexts.map(c => c.id);

            // Prepare stash sync payload
            const stashRequest: SyncRequest = {
                deviceId: this.deviceId,
                deviceName: this.deviceName,
                lastSyncAt: config.lastSyncAt || null,
                stashes: pushStashes.map(stash => ({
                    id: stash.id,
                    contextId: stash.contextId || null,
                    content: stash.content,
                    enhancedContent: stash.enhancedContent || null,
                    completed: !!stash.completed,
                    completedAt: stash.completedAt || null,
                    createdAt: stash.createdAt,
                    updatedAt: new Date(
                        localTimeToMs(stash.updatedAt, stash.createdAt)
                    ).toISOString(),
                    deleted: !!stash.deleted,
                    attachments: (stash.attachments || []).map(att => ({
                        id: att.id,
                        fileName: att.fileName,
                        fileSize: att.fileSize,
                        mimeType: att.mimeType || null,
                        syntax: att.syntax || null,
                    })),
                })),
                positions: pushPositions,
            };

            // Prepare context sync payload
            const contextRequest: ContextSyncRequest = {
                deviceId: this.deviceId,
                deviceName: this.deviceName,
                lastSyncAt: config.lastSyncAt || null,
                contexts: pushContexts.map(ctx => ({
                    id: ctx.id,
                    name: ctx.name,
                    // Without this the server has no description to return, and every
                    // pull overwrote the local one with nothing.
                    description: ctx.description || null,
                    rules: ctx.rules || [],
                    lastUsed: ctx.lastUsed || null,
                    updatedAt: new Date(
                        localTimeToMs(ctx.updatedAt, ctx.lastUsed) || Date.now()
                    ).toISOString(),
                    deleted: !!ctx.deleted,
                })),
            };

            // Sync sequentially: contexts first, then stashes, then attachments.
            // This prevents foreign key constraint database errors on the server.
            const contextResponse = await this.callContextSyncApi(config, contextRequest);
            const stashResponse = await this.callStashSyncApi(config, stashRequest);

            let stashCount = 0;
            let contextCount = 0;

            // Tracks whether the server actually gave us something to write, so the UI
            // can skip its full reload when it did not.
            let appliedRemoteChanges = false;

            if (contextResponse) {
                appliedRemoteChanges =
                    (await this.mergeServerContexts(contextResponse.synced, localContexts)) ||
                    appliedRemoteChanges;
                contextCount = contextResponse.synced.length;
                this.reportRejected('contexts', contextResponse.rejected);
                await this.acknowledgePush('contexts', sentContextIds, contextResponse.rejected);
            }

            if (stashResponse) {
                appliedRemoteChanges =
                    (await this.mergeServerStashes(stashResponse.synced, localStashes)) ||
                    appliedRemoteChanges;
                stashCount = stashResponse.synced.length;
                this.reportRejected('stashes', stashResponse.rejected);
                await this.acknowledgePush('stashes', sentStashIds, stashResponse.rejected);

                // Orderings the server accepted are cleared by id. Only rows still in
                // flight are cleared, so a reorder made during the push stays queued.
                if (pushPositions.length > 0) {
                    await this.adapter.markPositionsSynced(pushPositions.map(p => p.id));
                }

                // Orderings from other devices. Counted as a remote change so the queue
                // reloads - a sync that moved rows but changed no text still reorders
                // what the user is looking at.
                const incoming = (stashResponse.positions || []).filter(
                    p => !pushPositions.some(sent => sent.id === p.id)
                );
                if (incoming.length > 0) {
                    const moved = await this.adapter.importPositions(incoming);
                    if (moved > 0) appliedRemoteChanges = true;
                }
            }

            // Upload after merging so attachments pulled in this cycle are already
            // marked as present locally and are not immediately pushed back up.
            const uploadedAttachments = await this.uploadPendingAttachments(localStashes);

            // Update last sync timestamp
            if (this.settings?.cloudConfig && (stashResponse || contextResponse)) {
                this.settings.cloudConfig.lastSyncAt =
                    stashResponse?.serverTime ||
                    contextResponse?.serverTime ||
                    new Date().toISOString();
                await this.adapter.saveSettings(this.settings);
            }

            // Stashes syncing while attachments silently fail is not a success.
            if (this.lastAttachmentError) {
                // `appliedRemoteChanges` is passed here too: a failed *upload* says
                // nothing about the data this sync pulled *down*. Omitting it meant a
                // sync that had written remote stashes locally never refreshed the
                // queue, so the new stashes stayed invisible until something else
                // happened to trigger a reload.
                this.setStatus(
                    'error',
                    `Attachments could not be uploaded: ${this.lastAttachmentError}`,
                    appliedRemoteChanges
                );
            } else {
                this.setStatus(
                    'success',
                    `Synced ${stashCount} stashes, ${contextCount} contexts`,
                    appliedRemoteChanges
                );
            }
            console.log(`[CloudSync] Synced ${stashCount} stashes, ${contextCount} contexts`);

            // Confirming an upload publishes the file server-side but emits no WebSocket
            // notification of its own, so other devices would not hear about it until
            // their next fallback poll. One more pass makes the stash endpoint broadcast.
            // This terminates: the second pass uploads nothing, so it does not re-arm.
            if (uploadedAttachments) {
                console.log('[CloudSync] Attachments uploaded - notifying other devices');
                this.triggerSync();
            }

            return true;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            console.error('[CloudSync] Sync failed:', message);
            this.setStatus(this.isAuthError(message) ? 'auth-error' : 'error', message);
            return false;
        } finally {
            this.isSyncing = false;
        }
    }

    /**
     * Clear the pending flag for records the server took.
     *
     * Only reached when the call succeeded: a failed push leaves every flag set, so
     * nothing is lost, it simply goes out again next time. Records the server rejected
     * keep their flag too - they are still unsynced, whatever the reason.
     *
     * Only records still in the in-flight state are cleared, so one edited while the
     * request was running stays queued rather than being silently skipped.
     */
    private async acknowledgePush(
        kind: 'stashes' | 'contexts',
        sentIds: string[],
        rejected?: RejectedRecord[]
    ): Promise<void> {
        if (sentIds.length === 0) return;

        const refused = new Set((rejected || []).map(r => r.id));
        const accepted = sentIds.filter(id => !refused.has(id));
        if (accepted.length === 0) return;

        try {
            if (kind === 'stashes') {
                await this.adapter.markStashesSynced(accepted);
            } else {
                await this.adapter.markContextsSynced(accepted);
            }
        } catch (e) {
            // Leaving the flags set only costs a redundant push next cycle.
            console.warn(`[CloudSync] Could not clear pending flags for ${kind}:`, e);
        }
    }

    /** Does this error message indicate an expired or rejected session? */
    private isAuthError(message: string): boolean {
        return message.includes('Authentication expired') || message.includes('401');
    }

    /**
     * Surface records the server refused. These are silent data loss otherwise.
     */
    private reportRejected(kind: string, rejected?: RejectedRecord[]): void {
        if (!rejected?.length) return;
        console.warn(
            `[CloudSync] Server rejected ${rejected.length} ${kind}:`,
            rejected.map(r => `${r.id}: ${r.reason}`).join('; ')
        );
    }

    /**
     * Call the stash sync API
     */
    private async callStashSyncApi(
        config: CloudConfig,
        request: SyncRequest
    ): Promise<SyncResponse | null> {
        try {
            const response = await this.adapter.syncStashesApi(request);
            return response as SyncResponse;
        } catch (error) {
            const msg = error instanceof Error ? error.message : String(error);
            if (this.isAuthError(msg)) {
                this.setStatus('auth-error', 'Authentication expired. Please log in again.');
                return null;
            }
            throw new Error(`Stash sync failed: ${msg}`);
        }
    }

    /**
     * Call the context sync API
     */
    private async callContextSyncApi(
        config: CloudConfig,
        request: ContextSyncRequest
    ): Promise<ContextSyncResponse | null> {
        try {
            const response = await this.adapter.syncContextsApi(request);
            return response as ContextSyncResponse;
        } catch (error) {
            const msg = error instanceof Error ? error.message : String(error);
            if (this.isAuthError(msg)) {
                this.setStatus('auth-error', 'Authentication expired. Please log in again.');
                return null;
            }
            throw new Error(`Context sync failed: ${msg}`);
        }
    }

    /**
     * Merge server stashes with local data using LWW
     */
    /** @returns whether anything was actually written locally. */
    private async mergeServerStashes(
        serverStashes: StashItem[],
        localStashes: StashItem[]
    ): Promise<boolean> {
        const localMap = new Map(localStashes.map(s => [s.id, s]));
        const toSave: StashItem[] = [];

        for (const serverStash of serverStashes) {
            const localStash = localMap.get(serverStash.id);

            const serverMs = localTimeToMs(serverStash.updatedAt, serverStash.createdAt);
            // Convert to Unix seconds for the local DB.
            const stashToSave = {
                ...serverStash,
                updatedAt: toSeconds(serverMs),
                deleted: !!(serverStash as any).deletedAt || !!serverStash.deleted,
            } as StashItem;

            if (!localStash) {
                toSave.push(stashToSave);
                continue;
            }

            const localMs = localTimeToMs(localStash.updatedAt, localStash.createdAt);
            // Second granularity on both sides: the local column only stores seconds,
            // so comparing raw milliseconds made every record look perpetually stale.
            if (toSeconds(serverMs) > toSeconds(localMs)) {
                // The server's content wins, but its attachment list is not authoritative
                // - it omits anything not yet confirmed uploaded - so union the two.
                toSave.push({
                    ...stashToSave,
                    attachments: mergeAttachments(localStash, stashToSave),
                });
                continue;
            }

            // Attachment metadata can change without the stash's own updatedAt moving.
            // Confirming an upload publishes the file to other devices but touches only
            // the server-side cursor, never the client clock that last-write-wins
            // compares. Without this the receiving device rejects the record as "not
            // newer" and the attachment never arrives, even though the stash itself
            // syncs perfectly.
            if (hasUnknownAttachments(stashToSave, localStash)) {
                // Merge rather than replace: the server's list omits anything not yet
                // confirmed uploaded, so adopting it wholesale would drop attachments
                // this device added moments ago and has not finished uploading.
                toSave.push({
                    ...stashToSave,
                    attachments: mergeAttachments(localStash, stashToSave),
                });
            }
        }

        const imported = toSave.length > 0;
        if (imported) {
            await this.adapter.importStashes(toSave);
        }

        // Queue bytes for every attachment we know of but do not hold. Built from the
        // data already in hand: re-reading the whole stash list here meant loading every
        // stash three times per sync, which is a visible stall once there are a few
        // hundred of them.
        const knownPaths = new Map<string, string>();
        for (const stash of localStashes) {
            for (const att of stash.attachments || []) knownPaths.set(att.id, att.filePath || '');
        }
        for (const stash of serverStashes) {
            for (const att of stash.attachments || []) {
                if (!knownPaths.has(att.id)) knownPaths.set(att.id, '');
            }
        }

        attachmentSync.enqueue(
            [...knownPaths.entries()]
                .filter(([id, path]) => attachmentSync.isPending(id, path))
                .map(([id]) => id)
        );

        return imported;
    }

    /**
     * Merge server contexts with local data using LWW
     *
     * @returns whether anything was actually written locally.
     */
    private async mergeServerContexts(
        serverContexts: SyncContext[],
        localContexts: Context[]
    ): Promise<boolean> {
        const localMap = new Map(localContexts.map(c => [c.id, c]));
        const toSave: Context[] = [];

        for (const serverCtx of serverContexts) {
            const localCtx = localMap.get(serverCtx.id);
            const serverMs = localTimeToMs(serverCtx.updatedAt, serverCtx.lastUsed ?? undefined);
            const isDeleted = !!serverCtx.deletedAt || !!(serverCtx as any).deleted;

            const candidate: Context = {
                id: serverCtx.id,
                name: serverCtx.name,
                // Fall back to the local value: an older server record that predates
                // description syncing must not blank out what this device already has.
                description: serverCtx.description ?? localCtx?.description,
                rules: serverCtx.rules as Context['rules'],
                lastUsed: serverCtx.lastUsed || undefined,
                updatedAt: toSeconds(serverMs),
                deleted: isDeleted,
            };

            if (!localCtx) {
                toSave.push(candidate);
                continue;
            }

            const localMs = localTimeToMs(localCtx.updatedAt, localCtx.lastUsed);
            if (toSeconds(serverMs) > toSeconds(localMs)) {
                toSave.push(candidate);
            }
        }

        if (toSave.length === 0) return false;

        // importContexts, not saveContext: the latter is the local-edit path and
        // stamps the current time, which would make every pulled record look
        // locally modified and bounce straight back to the server.
        await this.adapter.importContexts(toSave);
        return true;
    }

    /**
     * Add a status listener
     */
    addListener(listener: SyncListener): () => void {
        this.listeners.add(listener);
        // Immediately notify of current status
        listener(this.status);
        return () => this.listeners.delete(listener);
    }

    /**
     * Set status and notify listeners
     */
    private setStatus(
        status: SyncStatus,
        message?: string,
        appliedRemoteChanges = false
    ): void {
        this.status = status;
        this.listeners.forEach(listener => listener(status, message, appliedRemoteChanges));
    }

    /**
     * Get current sync status
     */
    getStatus(): SyncStatus {
        return this.status;
    }

    private async connectWebSocket() {
        try {
            await this.adapter.connectWebSocket();

            // Listen for sync notifications from the Rust backend
            if (!this.wsUnlisten) {
                this.wsUnlisten = await listen<{ type: string, source_device: string, timestamp: string }>('cloud:sync-notification', (event) => {
                    // Do not sync if the notification came from our own device (loop prevention)
                    if (event.payload.source_device !== this.deviceId) {
                        console.debug('[CloudSyncService] Received sync notification from', event.payload.source_device, '- scheduling sync');
                        this.scheduleRemoteSync();
                    }
                });
            }
        } catch (error) {
            console.error('[CloudSyncService] Failed to connect WebSocket:', error);
        }
    }

    private async disconnectWebSocket() {
        if (this.wsUnlisten) {
            this.wsUnlisten();
            this.wsUnlisten = null;
        }
        // Drop any sync a notification had queued: the socket is going away, and on a
        // logout path the deferred sync would otherwise fire against cleared credentials.
        if (this.remoteSyncTimer) {
            clearTimeout(this.remoteSyncTimer);
            this.remoteSyncTimer = null;
        }
        try {
            await this.adapter.disconnectWebSocket();
        } catch (error) {
            console.error('[CloudSyncService] Failed to disconnect WebSocket:', error);
        }
    }

    /**
     * Clean up resources
     */
    dispose(): void {
        this.stopPeriodicSync();
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }
        if (this.remoteSyncTimer) {
            clearTimeout(this.remoteSyncTimer);
            this.remoteSyncTimer = null;
        }
        this.listeners.clear();
    }

    /**
     * Push any attachment whose bytes aren't in the cloud yet.
     *
     * The Rust command is idempotent: it returns immediately for attachments already
     * marked uploaded, and for metadata-only rows pulled from another device whose
     * file hasn't been downloaded yet.
     */
    private async uploadPendingAttachments(stashes: StashItem[]): Promise<boolean> {
        // Only attachments this device actually holds bytes for, belonging to stashes
        // that still exist. Metadata-only rows pulled from another device have nothing
        // to upload, and a deleted stash's files are not worth sending - without this
        // they are retried on every sync forever, which for a handful of screenshots is
        // megabytes of pointless transfer per cycle.
        const attachments = stashes
            .filter(s => !s.deleted)
            .flatMap(s => s.attachments || [])
            .filter(att => att.filePath && att.filePath.trim() !== '');

        // Cleared here, not inside the loop below: returning early with a stale error
        // still set pinned the sync status to 'error' forever once the last failing
        // attachment was deleted, with nothing left to retry and clear it.
        this.lastAttachmentError = null;

        if (attachments.length === 0) return false;

        let uploaded = false;
        const failures: string[] = [];
        const now = Date.now();
        const deadline = now + ATTACHMENT_PHASE_BUDGET_MS;
        let attempted = 0;
        let deferred = 0;

        for (const att of attachments) {
            // Back off after a failure instead of re-reading and re-sending the whole
            // file on every sync. A permanently broken attachment would otherwise burn
            // its full size in upload bandwidth on every cycle, indefinitely.
            if (now < (this.attachmentRetryAfter.get(att.id) ?? 0)) continue;

            // Bounded per cycle, by count and by wall clock. Uploads are serial and each
            // one can take the full transfer timeout, so an unbounded loop held the sync
            // lock - and with it stash and context sync - for as long as the backlog took.
            if (attempted >= MAX_UPLOADS_PER_CYCLE || Date.now() >= deadline) {
                deferred++;
                continue;
            }
            attempted++;

            try {
                if (await this.adapter.uploadAttachmentToCloud(att.id)) {
                    uploaded = true;
                }
                this.attachmentFailures.delete(att.id);
                this.attachmentRetryAfter.delete(att.id);
            } catch (e) {
                const msg = e instanceof Error ? e.message : String(e);
                console.warn(`[CloudSync] Attachment upload failed for ${att.id}:`, msg);
                failures.push(msg);

                const attempts = (this.attachmentFailures.get(att.id) ?? 0) + 1;
                this.attachmentFailures.set(att.id, attempts);
                this.attachmentRetryAfter.set(
                    att.id,
                    Date.now() +
                        Math.min(
                            UPLOAD_RETRY_BASE_MS * 2 ** (attempts - 1),
                            UPLOAD_RETRY_MAX_MS
                        )
                );
            }
        }

        // Say so rather than looking like everything was covered.
        if (deferred > 0) {
            console.log(
                `[CloudSync] ${deferred} attachment(s) deferred to the next cycle (per-cycle limit reached)`
            );
            // More work is waiting and nothing will announce it, so come back for it.
            this.triggerSync();
        }

        // A swallowed console.warn was the only sign of this failing, so uploads could
        // break indefinitely while the app still reported a healthy sync. Surface it.
        if (failures.length > 0) {
            this.lastAttachmentError = failures[0];
            console.error(
                `[CloudSync] ${failures.length} attachment upload(s) failed. First error: ${failures[0]}`
            );
        }

        return uploaded;
    }

}
