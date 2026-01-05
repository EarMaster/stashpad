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

import { invoke } from '@tauri-apps/api/core';
import type { IStorageService, StashItem, AppContext, Settings, FilePreviewData } from '../types';

export class DesktopStorageAdapter implements IStorageService {
    async saveStash(stash: StashItem, options?: { invertPosition?: boolean }): Promise<void> {
        const invertPosition = options?.invertPosition ?? false;
        // Wrap in 'options' object to match Rust SaveOptions struct
        await invoke('save_stash', { options: { stash, invertPosition } });
    }

    async saveStashes(stashesList: StashItem[]): Promise<void> {
        await invoke('save_stashes', { stashesList });
    }

    async loadStashes(): Promise<StashItem[]> {
        return await invoke('load_stashes');
    }

    /**
     * Save an asset file to the cache directory.
     * Files are organized hierarchically: cache/<contextId>/<stashId>/<filename>
     */
    async saveAsset(file: File, contextId?: string, stashId?: string): Promise<string> {
        const buffer = await file.arrayBuffer();
        const bytes = new Uint8Array(buffer);
        return await invoke('save_asset', {
            name: file.name,
            data: bytes,
            contextId: contextId ?? null,
            stashId: stashId ?? null
        });
    }

    async getPreviousAppInfo(): Promise<AppContext> {
        return await invoke('get_previous_app_info');
    }

    async getSmartTransferTarget(): Promise<'GUI' | 'CLI'> {
        return await invoke('get_smart_transfer_target');
    }

    async copyToClipboard(text: string): Promise<void> {
        await invoke('copy_to_clipboard', { text });
    }

    async startDrag(text: string, files: string[]): Promise<void> {
        await invoke('start_drag', { text, files });
    }

    /**
     * Import an asset from an external file path into the cache directory.
     * Files are organized hierarchically: cache/<contextId>/<stashId>/<filename>
     */
    async saveAssetFromPath(path: string, contextId?: string, stashId?: string): Promise<string> {
        return await invoke('save_asset_from_path', {
            path,
            contextId: contextId ?? null,
            stashId: stashId ?? null
        });
    }

    /**
     * Reads a file and returns preview data based on its type.
     * Images return base64 data URI, videos return file path, text returns content.
     * @param path - Absolute path to the file
     * @returns Preview data including file type, content, and metadata
     */
    async readFileForPreview(path: string): Promise<FilePreviewData> {
        return await invoke('read_file_for_preview', { path });
    }

    async getSettings(): Promise<Settings> {
        return await invoke('get_settings');
    }

    async saveSettings(settings: Settings): Promise<void> {
        await invoke('save_settings', { settings });
    }

    async deleteStash(id: string): Promise<void> {
        await invoke('delete_stash', { id });
    }

    async deleteCompletedStashes(contextId?: string): Promise<void> {
        await invoke('delete_completed_stashes', { contextId });
    }

    async triggerAutoCleanup(): Promise<void> {
        await invoke('trigger_auto_cleanup');
    }

    async isWindows10(): Promise<boolean> {
        return await invoke('is_windows_10');
    }
}

