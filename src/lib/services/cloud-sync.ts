// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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

import type { IStorageService, StashItem, Context, CloudConfig, Settings } from '../types';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// Fallback polling interval, used when the WebSocket is unavailable.
const FALLBACK_SYNC_INTERVAL_MS = 15 * 60 * 1000;
const DEBOUNCE_DELAY_MS = 2000;

/** Sync status for UI feedback */
export type SyncStatus = 'idle' | 'syncing' | 'success' | 'error' | 'offline' | 'auth-error';

/** Sync event listener */
export type SyncListener = (status: SyncStatus, message?: string) => void;

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

    constructor(adapter: IStorageService) {
        this.adapter = adapter;
        this.deviceId = this.getOrCreateDeviceId();
    }

    /**
     * Get or create a persistent device ID for sync tracking
     */
    private getOrCreateDeviceId(): string {
        const key = 'stashpad_device_id';
        let deviceId = localStorage.getItem(key);
        if (!deviceId) {
            deviceId = crypto.randomUUID();
            localStorage.setItem(key, deviceId);
        }
        return deviceId;
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

            // Prepare stash sync payload
            const stashRequest: SyncRequest = {
                deviceId: this.deviceId,
                deviceName: this.deviceName,
                lastSyncAt: config.lastSyncAt || null,
                stashes: localStashes.map(stash => ({
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
            };

            // Prepare context sync payload
            const contextRequest: ContextSyncRequest = {
                deviceId: this.deviceId,
                deviceName: this.deviceName,
                lastSyncAt: config.lastSyncAt || null,
                contexts: localContexts.map(ctx => ({
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

            if (contextResponse) {
                await this.mergeServerContexts(contextResponse.synced, localContexts);
                contextCount = contextResponse.synced.length;
                this.reportRejected('contexts', contextResponse.rejected);
            }

            if (stashResponse) {
                await this.mergeServerStashes(stashResponse.synced, localStashes);
                stashCount = stashResponse.synced.length;
                this.reportRejected('stashes', stashResponse.rejected);
            }

            // Upload after merging so attachments pulled in this cycle are already
            // marked as present locally and are not immediately pushed back up.
            await this.uploadPendingAttachments();

            // Update last sync timestamp
            if (this.settings?.cloudConfig && (stashResponse || contextResponse)) {
                this.settings.cloudConfig.lastSyncAt =
                    stashResponse?.serverTime ||
                    contextResponse?.serverTime ||
                    new Date().toISOString();
                await this.adapter.saveSettings(this.settings);
            }

            this.setStatus('success', `Synced ${stashCount} stashes, ${contextCount} contexts`);
            console.log(`[CloudSync] Synced ${stashCount} stashes, ${contextCount} contexts`);
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
    private async mergeServerStashes(
        serverStashes: StashItem[],
        localStashes: StashItem[]
    ): Promise<void> {
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
                toSave.push(stashToSave);
            }
        }

        if (toSave.length > 0) {
            await this.adapter.importStashes(toSave);
        }

        // Fetch the bytes for anything we now know about but don't hold locally.
        await this.downloadMissingAttachments(toSave);
    }

    /**
     * Merge server contexts with local data using LWW
     */
    private async mergeServerContexts(
        serverContexts: SyncContext[],
        localContexts: Context[]
    ): Promise<void> {
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

        if (toSave.length > 0) {
            // importContexts, not saveContext: the latter is the local-edit path and
            // stamps the current time, which would make every pulled record look
            // locally modified and bounce straight back to the server.
            await this.adapter.importContexts(toSave);
        }
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
    private setStatus(status: SyncStatus, message?: string): void {
        this.status = status;
        this.listeners.forEach(listener => listener(status, message));
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
                        console.debug('[CloudSyncService] Received sync notification from', event.payload.source_device, '- triggering sync');
                        void this.sync();
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
        this.listeners.clear();
    }

    /**
     * Push any attachment whose bytes aren't in the cloud yet.
     *
     * The Rust command is idempotent: it returns immediately for attachments already
     * marked uploaded, and for metadata-only rows pulled from another device whose
     * file hasn't been downloaded yet.
     */
    private async uploadPendingAttachments(): Promise<void> {
        const stashes = await this.adapter.loadStashesForSync();
        const attachments = stashes.flatMap(s => s.attachments || []);

        if (attachments.length === 0) return;

        for (const att of attachments) {
            try {
                await this.adapter.uploadAttachmentToCloud(att.id);
            } catch (e) {
                console.warn(`[CloudSync] Attachment upload failed for ${att.id}:`, e);
            }
        }
    }

    /**
     * Download the bytes for attachments this device knows about but doesn't hold.
     *
     * Sync used to be upload-only, so a receiving device ended up with attachment rows
     * whose `filePath` was empty — a file listed in the UI that could never be opened.
     */
    private async downloadMissingAttachments(stashes: StashItem[]): Promise<void> {
        for (const stash of stashes) {
            for (const att of stash.attachments || []) {
                if (att.filePath && att.filePath.trim() !== '') continue;
                try {
                    await this.adapter.downloadAttachmentFromCloud(att.id);
                } catch (e) {
                    console.warn(`[CloudSync] Attachment download failed for ${att.id}:`, e);
                }
            }
        }
    }
}
