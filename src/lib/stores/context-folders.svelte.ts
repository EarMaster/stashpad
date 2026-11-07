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
 * Each context's project folder on this device, and the files the `@path` picker offers.
 *
 * Kept apart from `Context` on purpose: contexts sync, and a folder path only means
 * something on the machine it was picked on.
 */

import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
import type { ContextFolder, FilePreviewData } from "$lib/types";
import { log } from "$lib/utils/log";
import { makeFileRefResolver, type FileRefResolver } from "$lib/utils/file-refs";

const adapter = new DesktopStorageAdapter();

let folders = $state<Record<string, ContextFolder>>({});
// Raw: lists are only ever replaced, and deep-proxying thousands of paths buys nothing.
let files = $state.raw<Record<string, string[]>>({});
let loaded = false;

/** Load every folder link once; later calls are no-ops unless `force` is set. */
export async function loadContextFolders(force = false): Promise<void> {
    if (loaded && !force) return;
    try {
        const list = await adapter.getContextFolders();
        folders = Object.fromEntries(list.map((f) => [f.contextId, f]));
        loaded = true;
    } catch (e) {
        log.error("Failed to load project folders:", e);
    }
}

export function getContextFolder(contextId: string | undefined): ContextFolder | undefined {
    return contextId ? folders[contextId] : undefined;
}

/** Whether any context has a folder linked on this device. */
export function hasContextFolders(): boolean {
    return Object.keys(folders).length > 0;
}

/**
 * Whether signing out has folder links to ask about. Reads them fresh, so a link made or
 * an import run a moment ago is counted; with none, there is nothing to keep or drop.
 */
export async function hasLinksToKeepOnSignOut(): Promise<boolean> {
    await loadContextFolders(true);
    return hasContextFolders();
}

/**
 * Apply the sign-out choice. The links belong to this device rather than to the account, so
 * keeping them is the default and the only thing that ever removes them is an explicit
 * "do not keep".
 */
export async function applySignOutFolderChoice(keep: boolean): Promise<void> {
    if (!keep) await clearContextFolders();
}

/**
 * The files of a context's linked folder, or `undefined` when it has none on this device
 * or they have not been loaded yet. Call `loadContextFiles` to fill it.
 */
export function getContextFiles(contextId: string | undefined): string[] | undefined {
    if (!contextId || !folders[contextId]?.exists) return undefined;
    return files[contextId];
}

// Previews already fetched, by context and path. Dropped with the context's file list, so a
// refresh, relink or unlink never shows a file as it was before.
const peeks = new Map<string, Promise<FilePreviewData>>();

function forgetPeeks(contextId?: string) {
    if (!contextId) return peeks.clear();
    for (const key of peeks.keys()) {
        if (key.startsWith(`${contextId}\n`)) peeks.delete(key);
    }
}

/** The preview of a file in the context's linked folder, fetched once per file list. */
export function peekContextFile(contextId: string, path: string): Promise<FilePreviewData> {
    const key = `${contextId}\n${path}`;
    let peek = peeks.get(key);
    if (!peek) {
        peek = adapter.previewContextFile(contextId, path);
        // A failure is not kept: the next hover asks again.
        peek.catch(() => peeks.delete(key));
        peeks.set(key, peek);
    }
    return peek;
}

// One resolver per file list, shared by every card rendering that context.
const resolvers = new WeakMap<string[], FileRefResolver>();

/**
 * Checks `@path`s against the context's linked folder, or `undefined` when there is none
 * on this device - then paths render as plain mentions, unchecked.
 */
export function getFileRefResolver(contextId: string | undefined): FileRefResolver | undefined {
    const list = getContextFiles(contextId);
    if (!list) return undefined;
    let resolver = resolvers.get(list);
    if (!resolver) {
        resolver = makeFileRefResolver(list);
        resolvers.set(list, resolver);
    }
    return resolver;
}

/** Walk the context's folder (again, with `refresh`) and keep the result. */
export async function loadContextFiles(contextId: string, refresh = false): Promise<void> {
    if (!folders[contextId]?.exists) return;
    if (!refresh && files[contextId]) return;
    try {
        const index = await adapter.listContextFiles(contextId, refresh);
        forgetPeeks(contextId);
        files = { ...files, [contextId]: index.files };
    } catch (e) {
        log.error("Failed to list the project folder:", e);
    }
}

/** Ask for a folder and link it. Resolves to false when the dialog was cancelled. */
export async function linkContextFolder(contextId: string, title: string): Promise<boolean> {
    const folder = await adapter.pickContextFolder(contextId, title);
    if (!folder) return false;
    folders = { ...folders, [contextId]: folder };
    forgetPeeks(contextId);
    const { [contextId]: _stale, ...rest } = files;
    files = rest;
    return true;
}

export async function unlinkContextFolder(contextId: string): Promise<void> {
    await adapter.unlinkContextFolder(contextId);
    forgetPeeks(contextId);
    const { [contextId]: _folder, ...remainingFolders } = folders;
    const { [contextId]: _files, ...remainingFiles } = files;
    folders = remainingFolders;
    files = remainingFiles;
}

/** Drop every link on this device. */
export async function clearContextFolders(): Promise<void> {
    await adapter.clearContextFolders();
    forgetPeeks();
    folders = {};
    files = {};
}
