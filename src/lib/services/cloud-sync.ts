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
 * Sync Strategy:
 * - Periodic sync every 5 minutes when cloud is enabled
 * - On-demand sync after local changes (debounced)
 * - Uses Last-Write-Wins (LWW) conflict resolution
 */

import type { IStorageService, StashItem, Context, CloudConfig, Settings } from '../types';
import { jwtDecode } from 'jwt-decode';

/** Sync status for UI feedback */
export type SyncStatus = 'idle' | 'syncing' | 'success' | 'error' | 'offline' | 'auth-error';

/** Sync event listener */
export type SyncListener = (status: SyncStatus, message?: string) => void;

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
}

/** Cloud sync request payload */
interface SyncRequest {
    deviceId: string;
    lastSyncAt: string | null;
    stashes: SyncStashInput[];
}

/** Cloud sync response */
interface SyncResponse {
    synced: StashItem[];
    serverTime: string;
}

/** Context format for cloud API */
interface SyncContextInput {
    id: string;
    name: string;
    rules: unknown[];
    lastUsed: string | null;
    updatedAt: string;
    deleted: boolean;
}

/** Context sync request */
interface ContextSyncRequest {
    deviceId: string;
    contexts: SyncContextInput[];
}

/** Cloud context from server */
interface SyncContext {
    id: string;
    name: string;
    rules: unknown[];
    lastUsed: string | null;
    updatedAt: string;
}

/** Context sync response */
interface ContextSyncResponse {
    synced: SyncContext[];
    serverTime: string;
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
    private isSyncing = false;

    /** Sync interval in milliseconds (5 minutes) */
    private static readonly SYNC_INTERVAL_MS = 5 * 60 * 1000;
    /** Debounce delay for on-change sync (2 seconds) */
    private static readonly DEBOUNCE_DELAY_MS = 2000;

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

        if (this.shouldSync()) {
            this.startPeriodicSync();
            // Do an initial sync on startup
            await this.sync();
        }
    }

    /**
     * Update settings and restart sync if needed
     */
    updateSettings(settings: Settings): void {
        const wasEnabled = this.shouldSync();
        this.settings = settings;
        const isEnabled = this.shouldSync();

        if (isEnabled && !wasEnabled) {
            this.startPeriodicSync();
        } else if (!isEnabled && wasEnabled) {
            this.stopPeriodicSync();
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
            (config.subscriptionTier === 'pro' || config.subscriptionTier === 'enterprise')
        );
    }

    /**
     * Start periodic background sync
     */
    private startPeriodicSync(): void {
        if (this.syncInterval) return;

        this.syncInterval = setInterval(() => {
            this.sync();
        }, CloudSyncService.SYNC_INTERVAL_MS);

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
    }

    /**
     * Trigger a debounced sync (call after local changes)
     */
    triggerSync(): void {
        if (!this.shouldSync()) return;

        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }

        this.debounceTimer = setTimeout(() => {
            this.sync();
        }, CloudSyncService.DEBOUNCE_DELAY_MS);
    }

    /**
     * Perform a full sync with the cloud
     */
    async sync(): Promise<boolean> {
        if (!this.shouldSync() || this.isSyncing) {
            return false;
        }

        const config = this.settings!.cloudConfig!;

        // Token is stored in backend, so we rely on backend failure to detect expiration.

        this.isSyncing = true;
        this.setStatus('syncing');

        try {
            // Load local data
            const [localStashes, localContexts] = await Promise.all([
                this.adapter.loadStashes(),
                this.adapter.getContexts(),
            ]);

            // Prepare stash sync payload
            const stashRequest: SyncRequest = {
                deviceId: this.deviceId,
                lastSyncAt: config.lastSyncAt || null,
                stashes: localStashes.map(stash => ({
                    id: stash.id,
                    contextId: stash.contextId || null,
                    content: stash.content,
                    enhancedContent: stash.enhancedContent || null,
                    completed: !!stash.completed,
                    completedAt: stash.completedAt || null,
                    createdAt: stash.createdAt,
                    updatedAt: stash.updatedAt || stash.createdAt,
                    deleted: false,
                })),
            };

            // Prepare context sync payload
            const contextRequest: ContextSyncRequest = {
                deviceId: this.deviceId,
                contexts: localContexts.map(ctx => ({
                    id: ctx.id,
                    name: ctx.name,
                    rules: ctx.rules || [],
                    lastUsed: ctx.lastUsed || null,
                    updatedAt: ctx.lastUsed || new Date().toISOString(),
                    deleted: false,
                })),
            };

            // Sync both in parallel
            const [stashResponse, contextResponse] = await Promise.all([
                this.callStashSyncApi(config, stashRequest),
                this.callContextSyncApi(config, contextRequest),
            ]);

            let stashCount = 0;
            let contextCount = 0;

            if (stashResponse) {
                await this.mergeServerStashes(stashResponse.synced, localStashes);
                stashCount = stashResponse.synced.length;
            }

            if (contextResponse) {
                await this.mergeServerContexts(contextResponse.synced, localContexts);
                contextCount = contextResponse.synced.length;
            }

            // Update last sync timestamp
            if (this.settings?.cloudConfig && (stashResponse || contextResponse)) {
                this.settings.cloudConfig.lastSyncAt = stashResponse?.serverTime || contextResponse?.serverTime || new Date().toISOString();
                await this.adapter.saveSettings(this.settings);
            }

            this.setStatus('success', `Synced ${stashCount} stashes, ${contextCount} contexts`);
            console.log(`[CloudSync] Synced ${stashCount} stashes, ${contextCount} contexts`);
            return true;
        } catch (error) {
            const message = error instanceof Error ? error.message : 'Unknown error';
            console.error('[CloudSync] Sync failed:', message);
            this.setStatus('error', message);
            return false;
        } finally {
            this.isSyncing = false;
        }
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
            if (msg.includes('Authentication expired') || msg.includes('401')) {
                this.setStatus('error', 'Authentication expired. Please log in again.');
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
            if (msg.includes('Authentication expired') || msg.includes('401')) {
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

            if (!localStash) {
                toSave.push(serverStash);
            } else {
                const serverTime = new Date(serverStash.updatedAt || serverStash.createdAt).getTime();
                const localTime = new Date(localStash.updatedAt || localStash.createdAt).getTime();

                if (serverTime > localTime) {
                    toSave.push(serverStash);
                }
            }
        }

        if (toSave.length > 0) {
            await this.adapter.saveStashes(toSave);
        }
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

            if (!localCtx) {
                // New from server
                toSave.push({
                    id: serverCtx.id,
                    name: serverCtx.name,
                    rules: serverCtx.rules as Context['rules'],
                    lastUsed: serverCtx.lastUsed || undefined,
                });
            } else {
                const serverTime = new Date(serverCtx.updatedAt || serverCtx.lastUsed || 0).getTime();
                const localTime = new Date(localCtx.lastUsed || 0).getTime();

                if (serverTime > localTime) {
                    toSave.push({
                        ...localCtx,
                        name: serverCtx.name,
                        rules: serverCtx.rules as Context['rules'],
                        lastUsed: serverCtx.lastUsed || localCtx.lastUsed,
                    });
                }
            }
        }

        if (toSave.length > 0) {
            await this.adapter.saveContexts(toSave);
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
     * Decode JWT and check if it has expired
     * @returns true if valid, false if expired or invalid
     */
    private verifyAuth(token: string | null | undefined): boolean {
        // Obsolete: Token is now stored securely in the Rust backend
        return true;
    }
}
