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
import { listen } from '@tauri-apps/api/event';
import { attachmentSync } from '$lib/stores/attachment-sync.svelte';
import type { IStorageService, Settings, CloudConfig, StashItem, Context } from '$lib/types';

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn().mockResolvedValue(() => {}),
}));

/** Minimal adapter double: only the methods CloudSyncService actually touches. */
function createAdapter(overrides: Partial<IStorageService> = {}) {
    const base = {
        getDeviceName: vi.fn().mockResolvedValue('test-device'),
        fetchCloudAccount: vi.fn().mockResolvedValue(cloudConfig()),
        saveSettings: vi.fn().mockResolvedValue(undefined),
        loadStashesForSync: vi.fn().mockResolvedValue([]),
        getContextsForSync: vi.fn().mockResolvedValue([]),
        syncStashesApi: vi.fn().mockResolvedValue({ synced: [], serverTime: '2026-08-18T12:00:00Z' }),
        syncContextsApi: vi.fn().mockResolvedValue({ synced: [], serverTime: '2026-08-18T12:00:00Z' }),
        importStashes: vi.fn().mockResolvedValue(undefined),
        importContexts: vi.fn().mockResolvedValue(undefined),
        uploadAttachmentToCloud: vi.fn().mockResolvedValue(false),
        downloadAttachmentFromCloud: vi.fn().mockResolvedValue('/cache/file.png'),
        connectWebSocket: vi.fn().mockResolvedValue(undefined),
        disconnectWebSocket: vi.fn().mockResolvedValue(undefined),
        markStashesSynced: vi.fn().mockResolvedValue(undefined),
        markContextsSynced: vi.fn().mockResolvedValue(undefined),
        claimPendingPositions: vi.fn().mockResolvedValue([]),
        markPositionsSynced: vi.fn().mockResolvedValue(undefined),
        importPositions: vi.fn().mockResolvedValue(0),
        ...overrides,
    } as Record<string, unknown>;

    // Default the incremental-push lists to the full local lists, so tests written
    // before incremental push still exercise the same records.
    if (!('claimPendingStashes' in overrides)) {
        base.claimPendingStashes = () => (base.loadStashesForSync as () => Promise<unknown>)();
    }
    if (!('claimPendingContexts' in overrides)) {
        base.claimPendingContexts = () => (base.getContextsForSync as () => Promise<unknown>)();
    }

    return base as unknown as IStorageService;
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
const REMOTE_FLOOR_MS = 5000;

describe('CloudSyncService', () => {
    beforeEach(() => {
        localStorage.clear();
        vi.useFakeTimers();
        // Downloads are delegated to the shared queue singleton; clear what an earlier
        // test left behind so ids can be reused.
        attachmentSync.statuses = {};
        attachmentSync.resolved = {};
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

    describe('incremental push', () => {
        const twoStashes = [
            {
                id: 'unchanged',
                content: 'old',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512000,
                attachments: [],
            },
            {
                id: 'changed',
                content: 'new',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512400,
                attachments: [],
            },
        ] as unknown as StashItem[];

        it('pushes everything on the first sync, when there is no cursor yet', async () => {
            // Without a cursor this device has never synced successfully, so the server
            // needs the full picture rather than whatever happens to be flagged.
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(twoStashes),
                claimPendingStashes: vi.fn().mockResolvedValue([twoStashes[1]]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig({ lastSyncAt: undefined })));
            await flushPromises();

            const payload = (adapter.syncStashesApi as any).mock.calls[0][0];
            expect(payload.stashes).toHaveLength(2);
            expect(adapter.claimPendingStashes).not.toHaveBeenCalled();
        });

        it('pushes only changed records once a cursor exists', async () => {
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(twoStashes),
                claimPendingStashes: vi.fn().mockResolvedValue([twoStashes[1]]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            const payload = (adapter.syncStashesApi as any).mock.calls[0][0];
            expect(payload.stashes).toHaveLength(1);
            expect(payload.stashes[0].id).toBe('changed');
        });

        it('acknowledges what the server accepted', async () => {
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(twoStashes),
                claimPendingStashes: vi.fn().mockResolvedValue([twoStashes[1]]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            expect(adapter.markStashesSynced).toHaveBeenCalledWith(['changed']);
        });

        it('leaves a rejected record queued', async () => {
            // A record the server refused is still unsynced, whatever the reason.
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(twoStashes),
                claimPendingStashes: vi.fn().mockResolvedValue(twoStashes),
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [],
                    serverTime: '2026-08-18T12:00:00Z',
                    rejected: [{ id: 'changed', reason: 'unparseable createdAt' }],
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            expect(adapter.markStashesSynced).toHaveBeenCalledWith(['unchanged']);
        });

        it('acknowledges nothing when the push fails', async () => {
            // Clearing flags after a failed push would drop those edits permanently.
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(twoStashes),
                claimPendingStashes: vi.fn().mockResolvedValue(twoStashes),
                syncStashesApi: vi.fn().mockRejectedValue(new Error('502 Bad Gateway')),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            expect(adapter.markStashesSynced).not.toHaveBeenCalled();
        });

        it('still pushes contexts incrementally', async () => {
            const contexts = [
                { id: 'c1', name: 'Kept', rules: [], updatedAt: 1755512000 },
                { id: 'c2', name: 'Edited', rules: [], updatedAt: 1755512400 },
            ] as unknown as Context[];

            const adapter = createAdapter({
                getContextsForSync: vi.fn().mockResolvedValue(contexts),
                claimPendingContexts: vi.fn().mockResolvedValue([contexts[1]]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            const payload = (adapter.syncContextsApi as any).mock.calls[0][0];
            expect(payload.contexts).toHaveLength(1);
            expect(payload.contexts[0].id).toBe('c2');
            expect(adapter.markContextsSynced).toHaveBeenCalledWith(['c2']);
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
        it('uploads at most a bounded number of attachments per sync cycle', async () => {
            // Uploads run one at a time, each bounded only by the 300 s transfer timeout,
            // so an unbounded loop held `isSyncing` - and with it stash and context sync -
            // for as long as the whole backlog took, with the header stuck on "syncing".
            const attachments = Array.from({ length: 25 }, (_, i) => ({
                id: `a${i}`,
                fileName: `shot${i}.png`,
                fileSize: 10,
                filePath: `/cache/ctx/s1/shot${i}.png`,
            }));

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([
                    {
                        id: 's1',
                        content: 'many files',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        attachments,
                    },
                ]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            // 10 is MAX_UPLOADS_PER_CYCLE; the rest are deferred to a later cycle rather
            // than attempted in this one.
            expect(adapter.uploadAttachmentToCloud).toHaveBeenCalledTimes(10);
        });

        it('clears a stale attachment error once there is nothing left to upload', async () => {
            // The early return for "no attachments" used to happen before the error was
            // cleared, so deleting the last failing attachment pinned the status to
            // 'error' permanently - nothing was left to retry and clear it.
            const failing = {
                id: 'a1',
                fileName: 'shot.png',
                fileSize: 10,
                filePath: '/cache/ctx/s1/shot.png',
            };
            const stashWith = (atts: unknown[]) => [
                {
                    id: 's1',
                    content: 'file',
                    createdAt: '2026-08-18T10:00:00Z',
                    updatedAt: 1755512000,
                    attachments: atts,
                },
            ];

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue(stashWith([failing])),
                uploadAttachmentToCloud: vi.fn().mockRejectedValue(new Error('R2 unreachable')),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();
            expect(service.getStatus()).toBe('error');

            // The attachment is gone now, so the next sync has nothing to upload.
            (adapter.loadStashesForSync as ReturnType<typeof vi.fn>).mockResolvedValue(
                stashWith([])
            );
            await service.sync();
            await flushPromises();

            expect(service.getStatus()).toBe('success');
        });

        it('downloads bytes for a pulled attachment that has no local file', async () => {
            // Sync used to be upload-only, leaving the receiving device with attachment
            // rows whose filePath was empty - a file the UI listed but could never open.
            // Downloads are driven off the local DB, so model the row having been
            // imported: an empty filePath means the bytes were never fetched.
            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([
                    {
                        id: 's1',
                        content: 'with file',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        attachments: [
                            { id: 'a1', fileName: 'shot.png', fileSize: 10, filePath: '' },
                        ],
                    },
                ]),
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

            // Sync hands downloads to the queue, so it needs the same adapter.
            attachmentSync.setAdapter(adapter);

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.downloadAttachmentFromCloud).toHaveBeenCalledWith('a1');
        });

        it('imports a stash whose attachments changed even when updatedAt is unchanged', async () => {
            // The bug that left attachments unsynced while stashes synced instantly.
            // Confirming an upload publishes the file but touches only the server-side
            // cursor, never the client clock last-write-wins compares - so the receiving
            // device saw an equal timestamp, rejected the record, and never learned the
            // attachment existed.
            const local: StashItem = {
                id: 's1',
                content: 'same content',
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
                            content: 'same content',
                            createdAt: '2026-08-18T10:00:00Z',
                            // Identical second - LWW alone would reject this.
                            updatedAt: new Date(1755512000 * 1000).toISOString(),
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

            expect(adapter.importStashes).toHaveBeenCalled();
            const imported = (adapter.importStashes as any).mock.calls[0][0];
            expect(imported[0].attachments).toHaveLength(1);
        });

        it('never drops a local attachment the server has not confirmed yet', async () => {
            // Data-loss regression. The server withholds attachments whose bytes are not
            // confirmed uploaded, so seconds after adding one the server's copy of that
            // stash legitimately lists no attachments. Adopting that list wholesale
            // destroyed the file the user had just attached, before it was ever uploaded.
            const local: StashItem = {
                id: 's1',
                content: 'note',
                createdAt: '2026-08-18T10:00:00Z',
                updatedAt: 1755512000,
                attachments: [
                    { id: 'a1', fileName: 'shot.png', fileSize: 10, filePath: '/cache/c/s/shot.png' },
                ],
            } as unknown as StashItem;

            const adapter = createAdapter({
                loadStashesForSync: vi.fn().mockResolvedValue([local]),
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [
                        {
                            id: 's1',
                            content: 'note edited elsewhere',
                            createdAt: '2026-08-18T10:00:00Z',
                            // Newer, so the server's content wins...
                            updatedAt: new Date(1755512500 * 1000).toISOString(),
                            // ...but it knows nothing of the unconfirmed attachment.
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
            expect(imported[0].content).toBe('note edited elsewhere');
            expect(imported[0].attachments).toHaveLength(1);
            expect(imported[0].attachments[0].id).toBe('a1');
            // The local path must survive; the server never sends one.
            expect(imported[0].attachments[0].filePath).toBe('/cache/c/s/shot.png');
        });

        it('does not upload attachments belonging to a deleted stash', async () => {
            // Otherwise they are retried on every sync forever: the files are still on
            // disk, so nothing ever marks them done, and each cycle re-sends megabytes
            // for records the user has thrown away.
            const adapter = createAdapter({
                uploadAttachmentToCloud: vi.fn().mockResolvedValue(true),
                loadStashesForSync: vi.fn().mockResolvedValue([
                    {
                        id: 'alive',
                        content: 'kept',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        deleted: false,
                        attachments: [
                            { id: 'keep-me', fileName: 'a.png', fileSize: 1, filePath: '/tmp/a.png' },
                        ],
                    },
                    {
                        id: 'tombstone',
                        content: 'deleted',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        deleted: true,
                        attachments: [
                            { id: 'skip-me', fileName: 'b.png', fileSize: 1, filePath: '/tmp/b.png' },
                        ],
                    },
                ]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.uploadAttachmentToCloud).toHaveBeenCalledWith('keep-me');
            expect(adapter.uploadAttachmentToCloud).not.toHaveBeenCalledWith('skip-me');
        });

        it('schedules a follow-up sync after uploading, so peers are notified', async () => {
            // Confirming an upload emits no WebSocket notification of its own; without a
            // second pass the other device waits for the 15-minute fallback poll.
            const adapter = createAdapter({
                uploadAttachmentToCloud: vi.fn().mockResolvedValue(true),
                loadStashesForSync: vi.fn().mockResolvedValue([
                    {
                        id: 's1',
                        content: 'has file',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        attachments: [
                            { id: 'a1', fileName: 'x.png', fileSize: 1, filePath: '/tmp/x.png' },
                        ],
                    },
                ]),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const afterFirst = (adapter.syncStashesApi as any).mock.calls.length;

            await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 100);
            await flushPromises();

            expect((adapter.syncStashesApi as any).mock.calls.length).toBeGreaterThan(
                afterFirst,
            );
        });

        it('does not re-download an attachment already present locally', async () => {
            const adapter = createAdapter({
                downloadAttachmentFromCloud: vi.fn().mockResolvedValue('/cache/c/s/shot.png'),
                loadStashesForSync: vi.fn().mockResolvedValue([
                    {
                        id: 's1',
                        content: 'with file',
                        createdAt: '2026-08-18T10:00:00Z',
                        updatedAt: 1755512000,
                        attachments: [
                            {
                                id: 'a1',
                                fileName: 'shot.png',
                                fileSize: 10,
                                filePath: '/cache/c/s/shot.png',
                            },
                        ],
                    },
                ]),
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

            attachmentSync.setAdapter(adapter);

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

    describe('remote sync notifications', () => {
        /**
         * Build a service and hand back a way to fire the WebSocket notification the
         * service subscribes to, so the coalescing can be driven directly.
         */
        async function withNotifications(adapter: IStorageService) {
            let handler: ((event: unknown) => void) | null = null;
            (listen as any).mockImplementation(
                (name: string, cb: (event: unknown) => void) => {
                    if (name === 'cloud:sync-notification') handler = cb;
                    return Promise.resolve(() => {});
                },
            );

            const service = new CloudSyncService(adapter);
            await service.initialize(
                settingsWith(cloudConfig({ lastSyncAt: '2026-08-18T11:00:00Z' })),
            );
            await flushPromises();

            return {
                service,
                notify(sourceDevice: string) {
                    handler?.({
                        payload: {
                            type: 'sync_available',
                            source_device: sourceDevice,
                            timestamp: '2026-08-20T10:00:00Z',
                        },
                    });
                },
            };
        }

        it('collapses a burst of notifications into a single sync', async () => {
            // Each notification used to call sync() directly. Combined with a server that
            // announced every request, two devices woke each other without pause and the
            // constant import work under the shared database lock froze the UI.
            localStorage.setItem('stashpad_device_id', 'device-a');
            const adapter = createAdapter();
            const { notify } = await withNotifications(adapter);

            (adapter.syncStashesApi as any).mockClear();

            for (let i = 0; i < 5; i++) notify('device-b');
            await vi.advanceTimersByTimeAsync(REMOTE_FLOOR_MS + 100);
            await flushPromises();

            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(1);
        });

        it('ignores a notification caused by this device', async () => {
            localStorage.setItem('stashpad_device_id', 'device-a');
            const adapter = createAdapter();
            const { notify } = await withNotifications(adapter);

            (adapter.syncStashesApi as any).mockClear();

            notify('device-a');
            await vi.advanceTimersByTimeAsync(REMOTE_FLOOR_MS + 100);
            await flushPromises();

            expect(adapter.syncStashesApi).not.toHaveBeenCalled();
        });

        it('defers a notification that arrives inside the rate floor instead of dropping it', async () => {
            // The floor must not lose notifications: the second one carries real changes
            // and has to be picked up once the window passes, or the device silently
            // stays behind until the next fallback poll.
            localStorage.setItem('stashpad_device_id', 'device-a');
            const adapter = createAdapter();
            const { notify } = await withNotifications(adapter);

            (adapter.syncStashesApi as any).mockClear();

            notify('device-b');
            await vi.advanceTimersByTimeAsync(REMOTE_FLOOR_MS + 100);
            await flushPromises();
            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(1);

            // Immediately afterwards, well inside the floor: it must not sync straight
            // away...
            notify('device-b');
            await vi.advanceTimersByTimeAsync(500);
            await flushPromises();
            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(1);

            // ...but it must still happen once the window passes.
            await vi.advanceTimersByTimeAsync(REMOTE_FLOOR_MS);
            await flushPromises();
            expect(adapter.syncStashesApi).toHaveBeenCalledTimes(2);
        });
    });


    describe('refresh signalling', () => {
        /** Collect the appliedRemoteChanges flag from every 'success' notification. */
        function successFlags(service: CloudSyncService) {
            const seen: (boolean | undefined)[] = [];
            service.addListener((status, _message, applied) => {
                if (status === 'success') seen.push(applied);
            });
            return seen;
        }

        it('does not signal a refresh when the sync imported nothing', async () => {
            // The UI reloads its entire stash list when it hears this - content and
            // attachments over IPC, then a full re-render. Most syncs pull nothing, so
            // doing that on every success was pure cost.
            const adapter = createAdapter();
            const service = new CloudSyncService(adapter);
            const flags = successFlags(service);

            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(flags.length).toBeGreaterThan(0);
            expect(flags.every((f) => f === false)).toBe(true);
            expect(adapter.importStashes).not.toHaveBeenCalled();
        });

        it('signals a refresh when the sync imported a stash', async () => {
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
                            content: 'new from another device',
                            createdAt: '2026-08-18T10:00:00Z',
                            updatedAt: new Date(1755512500 * 1000).toISOString(),
                            attachments: [],
                        },
                    ],
                    serverTime: '2026-08-18T12:00:00Z',
                }),
            });

            const service = new CloudSyncService(adapter);
            const flags = successFlags(service);

            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.importStashes).toHaveBeenCalled();
            expect(flags.some((f) => f === true)).toBe(true);
        });
    });


    describe('ordering', () => {
        const moved = [{ id: 's1', position: 3, positionUpdatedAt: 1755512000 }];

        it('sends claimed orderings apart from the records', async () => {
            // Order travels on its own channel so a reorder never carries content with it.
            // Riding the record meant a cosmetic move could overwrite text edited on
            // another device, because Last-Write-Wins resolves per record.
            const adapter = createAdapter({
                claimPendingPositions: vi.fn().mockResolvedValue(moved),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            const payload = (adapter.syncStashesApi as any).mock.calls[0][0];
            expect(payload.positions).toEqual(moved);
            expect(payload.stashes).toHaveLength(0);
        });

        it('applies orderings from other devices and refreshes the queue', async () => {
            const incoming = [{ id: 'elsewhere', position: 7, positionUpdatedAt: 1755512500 }];
            const adapter = createAdapter({
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [],
                    serverTime: '2026-08-18T12:00:00Z',
                    positions: incoming,
                }),
                importPositions: vi.fn().mockResolvedValue(1),
            });

            const seen: (boolean | undefined)[] = [];
            const service = new CloudSyncService(adapter);
            service.addListener((status, _m, applied) => {
                if (status === 'success') seen.push(applied);
            });

            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.importPositions).toHaveBeenCalledWith(incoming);
            expect(seen.some((f) => f === true)).toBe(true);
        });

        it('does not re-apply the ordering it just sent', async () => {
            // The server echoes back what it stored. Importing our own move would be
            // harmless but pointless work, and it would report a remote change that never
            // happened, forcing a full queue reload after every reorder.
            const adapter = createAdapter({
                claimPendingPositions: vi.fn().mockResolvedValue(moved),
                syncStashesApi: vi.fn().mockResolvedValue({
                    synced: [],
                    serverTime: '2026-08-18T12:00:00Z',
                    positions: moved,
                }),
            });

            const service = new CloudSyncService(adapter);
            await service.initialize(settingsWith(cloudConfig()));
            await flushPromises();

            expect(adapter.importPositions).not.toHaveBeenCalled();
            expect(adapter.markPositionsSynced).toHaveBeenCalledWith(['s1']);
        });
    });

});
