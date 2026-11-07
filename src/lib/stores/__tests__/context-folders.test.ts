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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ContextFolder } from '$lib/types';

type Store = typeof import('../context-folders.svelte');

const folder = (contextId: string, exists = true): ContextFolder => ({
    contextId,
    path: `/work/${contextId}`,
    exists,
});

/**
 * Answer the backend commands the store calls. `links` is the backend's table, so a command
 * that changes it is visible to the next read - which is how sign-out is judged.
 */
function backend(links: ContextFolder[], files: string[] = []) {
    const calls: string[] = [];
    globalThis.mockTauriInvoke.mockReset();
    globalThis.mockTauriInvoke.mockImplementation(async (cmd: string, args?: any) => {
        calls.push(cmd);
        switch (cmd) {
            case 'get_context_folders':
                return links;
            case 'clear_context_folders':
                links.length = 0;
                return null;
            case 'unlink_context_folder': {
                const at = links.findIndex((l) => l.contextId === args.contextId);
                if (at >= 0) links.splice(at, 1);
                return null;
            }
            case 'list_context_files':
                return { files, truncated: false };
            default:
                return null;
        }
    });
    return calls;
}

describe('project folder store', () => {
    let store: Store;

    beforeEach(async () => {
        // The store keeps its links in module state, so each test gets a fresh one.
        vi.resetModules();
        store = await import('../context-folders.svelte');
    });

    describe('signing out', () => {
        it('has nothing to ask about when no folder is linked', async () => {
            backend([]);
            expect(await store.hasLinksToKeepOnSignOut()).toBe(false);
        });

        it('asks when a folder is linked, reading the links fresh', async () => {
            const links: ContextFolder[] = [];
            backend(links);
            await store.loadContextFolders();
            expect(store.hasContextFolders()).toBe(false);

            // Linked after the first load, as an import would.
            links.push(folder('a'));
            expect(await store.hasLinksToKeepOnSignOut()).toBe(true);
        });

        it('keeps every link when the user keeps them, without touching the backend', async () => {
            const links = [folder('a'), folder('b')];
            const calls = backend(links);
            await store.loadContextFolders();

            await store.applySignOutFolderChoice(true);

            expect(calls).not.toContain('clear_context_folders');
            expect(links).toHaveLength(2);
            expect(store.getContextFolder('a')).toBeDefined();
            expect(store.hasContextFolders()).toBe(true);
        });

        it('drops every link, here and in the backend, when the user does not keep them', async () => {
            const links = [folder('a'), folder('b')];
            const calls = backend(links);
            await store.loadContextFolders();

            await store.applySignOutFolderChoice(false);

            expect(calls).toContain('clear_context_folders');
            expect(links).toHaveLength(0);
            expect(store.getContextFolder('a')).toBeUndefined();
            expect(store.hasContextFolders()).toBe(false);
        });
    });

    describe('linking', () => {
        it('records the picked folder, and nothing when the dialog is cancelled', async () => {
            backend([]);
            globalThis.mockTauriInvoke.mockImplementation(async (cmd: string, args?: any) =>
                cmd === 'pick_context_folder'
                    ? args.contextId === 'picked'
                        ? folder('picked')
                        : null
                    : null,
            );

            expect(await store.linkContextFolder('picked', 'Choose')).toBe(true);
            expect(store.getContextFolder('picked')?.path).toBe('/work/picked');

            expect(await store.linkContextFolder('cancelled', 'Choose')).toBe(false);
            expect(store.getContextFolder('cancelled')).toBeUndefined();
        });

        it('forgets a context\'s link and files when it is unlinked', async () => {
            backend([folder('a')], ['src/app.ts']);
            await store.loadContextFolders();
            await store.loadContextFiles('a');
            expect(store.getContextFiles('a')).toEqual(['src/app.ts']);

            await store.unlinkContextFolder('a');

            expect(store.getContextFolder('a')).toBeUndefined();
            expect(store.getContextFiles('a')).toBeUndefined();
        });
    });

    describe('files and resolvers', () => {
        it('offers no files for a folder that is gone', async () => {
            const calls = backend([folder('gone', false)], ['src/app.ts']);
            await store.loadContextFolders();
            await store.loadContextFiles('gone');

            expect(calls).not.toContain('list_context_files');
            expect(store.getContextFiles('gone')).toBeUndefined();
            expect(store.getFileRefResolver('gone')).toBeUndefined();
        });

        it('checks paths against the folder, with one resolver shared per file list', async () => {
            backend([folder('a')], ['src/app.ts']);
            await store.loadContextFolders();
            await store.loadContextFiles('a');

            const resolve = store.getFileRefResolver('a');
            expect(resolve?.('src/app.ts').state).toBe('resolved');
            expect(resolve?.('src/missing.ts').state).toBe('unresolved');
            expect(store.getFileRefResolver('a')).toBe(resolve);
        });

        it('serves loaded files without walking again unless asked to refresh', async () => {
            const calls = backend([folder('a')], ['src/app.ts']);
            await store.loadContextFolders();
            await store.loadContextFiles('a');
            await store.loadContextFiles('a');
            expect(calls.filter((c) => c === 'list_context_files')).toHaveLength(1);

            await store.loadContextFiles('a', true);
            expect(calls.filter((c) => c === 'list_context_files')).toHaveLength(2);
        });
    });
});
