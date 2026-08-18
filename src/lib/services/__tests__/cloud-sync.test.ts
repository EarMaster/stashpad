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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { CloudSyncService } from '../cloud-sync';
import type { IStorageService, Settings, CloudConfig, StashItem, Context } from '$lib/types';

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn().mockResolvedValue(() => {}),
}));

/** Minimal adapter double: only the methods CloudSyncService actually touches. */
function createAdapter(overrides: Partial<IStorageService> = {}) {
    return {
        getDeviceName: vi.fn().mockResolvedValue('test-device'),
        fetchCloudAccount: vi.fn().mockResolvedValue(cloudConfig()),
        saveSettings: vi.fn().mockResolvedValue(undefined),
        loadStashesForSync: vi.fn().mockResolvedValue([]),
        getContextsForSync: vi.fn().mockResolvedValue([]),
        syncStashesApi: vi.fn().mockResolvedValue({ synced: [], serverTime: '2026-08-18T12:00:00Z' }),
        syncContextsApi: vi.fn().mockResolvedValue({ synced: [], serverTime: '2026-08-18T12:00:00Z' }),
        importStashes: vi.fn().mockResolvedValue(undefined),
        importContexts: vi.fn().mockResolvedValue(undefined),
        uploadAttachmentToCloud: vi.fn().mockResolvedValue(undefined),
        downloadAttachmentFromCloud: vi.fn().mockResolvedValue('/cache/file.png'),
        connectWebSocket: vi.fn().mockResolvedValue(undefined),
        disconnectWebSocket: vi.fn().mockResolvedValue(undefined),
        ...overrides,
    } as unknown as IStorageService;
}

function cloudConfig(overrides: Partial<CloudConfig> = {}): CloudConfig {
    return {
        enabled: true,
        endpoint: 'https://api.example.test',
        userId: 'user-1',
        email: 'user@example.test',
        subscriptionTier: 'pro',
        ...overrides,
    } as CloudConfig;
}

function settingsWith(config: CloudConfig): Settings {
    return { cloudConfig: config } as unknown as Settings;
}

/**
 * Drain pending microtasks without touching the timer queue.
 *
 * `runOnlyPendingTimers` would also fire the 15-minute fallback interval, producing a
 * second sync and making call-count assertions meaningless.
 */
async function flushPromises(): Promise<void> {
    for (let i = 0; i < 5; i++) {
        await Promise.resolve();
    }
}

/** Debounce window used by `triggerSync`. */
const DEBOUNCE_MS = 2000;

describe('CloudSyncService', () => {
    beforeEach(() => {
        localStorage.clear();
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
        vi.restoreAllMocks();
    });

    describe('updateSettings transition detection', () => {
        it('starts syncing when the SAME settings object is mutated in place', async () => {
            // Regression test. Callers pass the same Svelte $state proxy on every call
            // and mutate it in place, so deriving the "before" state from
            // this.settings made before and after always identical - the
            // false -> true transition never fired and sync never started after an
            // in-session login.
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);

            // Start out authenticated but not yet entitled.
            const settings = settingsWith(
                cloudConfig({ subscriptionTier: undefined, enabled: false }),
            );

            service.updateSettings(settings);
            expect(adapter.syncStashesApi).not.toHaveBeenCalled();

            // Mutate the very same object, as Settings.svelte does.
            settings.cloudConfig!.enabled = true;
            settings.cloudConfig!.subscriptionTier = 'pro';
            service.updateSettings(settings);

            await flushPromises();

            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(1);
        });

        it('stops syncing when entitlement is revoked on the same object', async () => {
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);
            const settings = settingsWith(cloudConfig());

            service.updateSettings(settings);
            await flushPromises();

            settings.cloudConfig!.enabled = false;
            service.updateSettings(settings);

            expect(adapter.disconnectWebSocket).toHaveBeenCalled();
        });
    });

    describe('entitlement lookup on startup', () => {
        it('fetches the subscription tier during initialize so sync can start', async () => {
            // A device linked via the code flow never opens Settings, which was the only
            // place that populated subscriptionTier. Without it shouldSync() returned
            // false forever - silently, with no error and no status change.
            const adapter = createAdapter({
                fetchCloudAccount: vi.fn().mockResolvedValue(cloudConfig({ subscriptionTier: 'pro' })),
            });
            const service = new CloudSyncService(adapter);

            const settings = settingsWith(cloudConfig({ subscriptionTier: undefined }));
            await service.initialize(settings);
            await flushPromises();

            expect(adapter.fetchCloudAccount).toHaveBeenCalled();
            expect(adapter.syncStashesApi).toHaveBeenCalled();
        });

        it('keeps the cached tier when the account fetch fails', async () => {
            // An offline start must not read as a downgrade.
            const adapter = createAdapter({
                fetchCloudAccount: vi.fn().mockRejectedValue(new Error('network unreachable')),
            });
            const service = new CloudSyncService(adapter);

            const settings = settingsWith(cloudConfig({ subscriptionTier: 'pro' }));
            await service.initialize(settings);
            await flushPromises();

            expect(settings.cloudConfig!.subscriptionTier).toBe('pro');
            expect(adapter.syncStashesApi).toHaveBeenCalled();
        });

        it('does not sync for a tier without entitlement', async () => {
            const adapter = createAdapter({
                fetchCloudAccount: vi.fn().mockResolvedValue(cloudConfig({ subscriptionTier: 'free' })),
            });
            const service = new CloudSyncService(adapter);

            await service.initialize(settingsWith(cloudConfig({ subscriptionTier: 'free' })));
            await flushPromises();

            expect(adapter.syncStashesApi).not.toHaveBeenCalled();
        });
    });

    describe('outgoing payload', () => {
        it('sends local second-precision timestamps as ISO strings', async () => {
            const stash: StashItem = {
                id: 's1',
                content: 'hello',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512000,
                attachments: [],
            } as unknown as StashItem;

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([stash]),
            });
            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const payload = (adapter.syncStashesApi as any).mock.calls[0][0];
            expect(payload.stashes[0].updatedAt).toBe(new Date(1755512000 * 1000).toISOString());
        });

        it('includes the context description so the server can store it', async () => {
            // Without this the server had nothing to return and every pull blanked the
            // local description via INSERT OR REPLACE.
            const context: Context = {
                id: 'c1',
                name: 'Project',
                description: 'Rust + Svelte',
                rules: [],
                updatedAt: 1755512000,
            } as unknown as Context;

            const adapter = createAdapter({
                getContextsForSync: vi.fn().mockResolvedValue([context]),
            });
            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const payload = (adapter.syncContextsApi as any).mock.calls[0][0];
            expect(payload.contexts[0].description).toBe('Rust + Svelte');
        });

        it('sends lastSyncAt as the delta cursor on both endpoints', async () => {
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-17T00:00:00Z' })),
            );
            await flushPromises();

            expect((adapter.syncStashesApi as any).mock.calls[0][0].lastSyncAt).toBe(
                '2026-08-17T00:00:00Z',
            );
            expect((adapter.syncContextsApi as any).mock.calls[0][0].lastSyncAt).toBe(
                '2026-08-17T00:00:00Z',
            );
        });
    });

    describe('merging server data', () => {
        it('imports a server stash that is newer than the local copy', async () => {
            const local: StashItem = {
                id: 's1',
                content: 'old',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512000,
                attachments: [],
            } as unknown as StashItem;

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([local]),
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'new',
                            createdAt: '2026-08-18T10:00:00Z',
                            updatedAt: new Date(1755512500 * 1000).toISOString(),
                            attachments: [],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const imported = (adapter.importStashes as any).mock.calls[0][0];
            expect(imported).toHaveLength(1);
            expect(imported[0].content).toBe('new');
            // Stored back as Unix seconds for the local DB.
            expect(imported[0].updatedAt).toBe(1755512500);
        });

        it('ignores sub-second differences so identical records are not re-imported', async () => {
            // The local column holds whole seconds. Comparing raw milliseconds made a
            // freshly pushed record look perpetually newer on the server and every sync
            // re-imported what it had just sent.
            const local: StashItem = {
                id: 's1',
                content: 'same',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512000,
                attachments: [],
            } as unknown as StashItem;

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([local]),
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'same',
                            createdAt: '2026-08-18T10:00:00Z',
                            // Exactly the same second as the local copy, plus the
                            // sub-second precision only the server retains.
                            updatedAt: new Date(1755512000 * 1000 + 777).toISOString(),
                            attachments: [],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.importStashes).not.toHaveBeenCalled();
        });

        it('routes pulled contexts through importContexts, not saveContext', async () => {
            // saveContext is the local-edit path and stamps "now", which would make every
            // pulled record look locally modified and bounce back to the server.
            const adapter = createAdapter({
                syncContextsApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 'c1',
                            name: 'Remote',
                            description: 'from other device',
                            rules: [],
                            lastUsed: null,
                            updatedAt: '2026-08-18T11:00:00Z',
                            deletedAt: null,
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const imported = (adapter.importContexts as any).mock.calls[0][0];
            expect(imported[0].description).toBe('from other device');
        });

        it('keeps the local description when the server has none', async () => {
            const localCtx: Context = {
                id: 'c1',
                name: 'Project',
                description: 'local notes',
                rules: [],
                updatedAt: 1755510000,
            } as unknown as Context;

            const adapter = createAdapter({
                getContextsForSync: vi.fn().mockResolvedValue([localCtx]),
                syncContextsApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 'c1',
                            name: 'Project',
                            description: null,
                            rules: [],
                            lastUsed: null,
                            updatedAt: new Date(1755512000 * 1000).toISOString(),
                            deletedAt: null,
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const imported = (adapter.importContexts as any).mock.calls[0][0];
            expect(imported[0].description).toBe('local notes');
        });

        it('marks a stash deleted when the server returns a tombstone', async () => {
            const adapter = createAdapter({
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'gone',
                            createdAt: '2026-08-18T10:00:00Z',
                            updatedAt: '2026-08-18T11:00:00Z',
                            deletedAt: '2026-08-18T11:00:00Z',
                            attachments: [],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const imported = (adapter.importStashes as any).mock.calls[0][0];
            expect(imported[0].deleted).toBe(true);
        });
    });

    describe('attachments', () => {
        it('downloads bytes for a pulled attachment that has no local file', async () => {
            // Sync used to be upload-only, leaving the receiving device with attachment
            // rows whose filePath was empty - a file the UI listed but could never open.
            const adapter = createAdapter({
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'with file',
                            createdAt: '2026-08-18T10:00:00Z',
                            updatedAt: '2026-08-18T11:00:00Z',
                            attachments: [
                                { id: 'a1', fileName: 'shot.png', fileSize: 10, filePath: '' },
                            ],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.downloadAttachmentFromCloud).toHaveBeenCalledWith('a1');
        });

        it('does not re-download an attachment already present locally', async () => {
            const adapter = createAdapter({
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'with file',
                            createdAt: '2026-08-18T10:00:00Z',
                            updatedAt: '2026-08-18T11:00:00Z',
                            attachments: [
                                {
                                    id: 'a1',
                                    fileName: 'shot.png',
                                    fileSize: 10,
                                    filePath: '/cache/c/s/shot.png',
                                },
                            ],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.downloadAttachmentFromCloud).not.toHaveBeenCalled();
        });
    });

    describe('status reporting', () => {
        it('reports auth-error on a 401 so the re-login prompt is reachable', async () => {
            // setStatus was only ever called with syncing/success/error, so the
            // "session expired - log in again" UI could never appear.
            const adapter = createAdapter({
                syncContextsApi: vi
                    .fn()
                    .mockRejectedValue(new Error('Authentication expired. Please log in again.')),
            });

            const service = new CloudSyncService(adapter);
            const seen: string[] = [];
            service.addListener(status => seen.push(status));

            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(seen).toContain('auth-error');
        });
    });

    describe('triggerSync', () => {
        it('coalesces a burst of local mutations into a single sync', async () => {
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            (adapter.syncStashesApi as any).mockClear();

            service.triggerSync();
            service.triggerSync();
            service.triggerSync();
            await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 100);
            await flushPromises();

            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(1);
        });

        it('does nothing when the account is not entitled', async () => {
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);
            service.updateSettings(settingsWith(cloudConfig({ subscriptionTier: 'free' })));

            service.triggerSync();
            await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 100);
            await flushPromises();

            expect(adapter.syncStashesApi).not.toHaveBeenCalled();
        });
    });
});
