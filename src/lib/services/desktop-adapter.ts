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
import type { IStorageService, StashItem, AppContext, Settings, FilePreviewData, Context, Attachment, CloudConfig } from '../types';

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
    async saveAsset(file: File, contextId?: string, stashId?: string, syntax?: string): Promise<Attachment> {
        const buffer = await file.arrayBuffer();
        const bytes = new Uint8Array(buffer);
        return await invoke('save_asset', {
            name: file.name,
            data: Array.from(bytes),
            context_id: contextId ?? null,
            contextId: contextId ?? null,
            stash_id: stashId ?? null,
            stashId: stashId ?? null,
            syntax: syntax ?? null
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
    async saveAssetFromPath(path: string, contextId?: string, stashId?: string, syntax?: string): Promise<Attachment> {
        return await invoke('save_asset_from_path', {
            path,
            context_id: contextId ?? null,
            contextId: contextId ?? null,
            stash_id: stashId ?? null,
            stashId: stashId ?? null,
            syntax: syntax ?? null
        });
    }

    /**
     * Delete an asset file from the cache directory.
     * @param path - Absolute path to the file to delete
     */
    async deleteAsset(path: string): Promise<void> {
        await invoke('delete_asset', { path });
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

    async getContexts(): Promise<Context[]> {
        return await invoke('get_contexts');
    }

    async saveContexts(contexts: Context[]): Promise<void> {
        await invoke('save_contexts', { contexts });
    }

    async saveContext(context: Context): Promise<void> {
        return await invoke('save_context', { context });
    }

    async deleteContext(id: string): Promise<void> {
        return await invoke('delete_context', { id });
    }

    async setAutostart(enabled: boolean): Promise<void> {
        return await invoke('set_autostart', { enabled });
    }

    async getAutostartEnabled(): Promise<boolean> {
        return await invoke('get_autostart_enabled');
    }

    async startCloudAuth(): Promise<CloudConfig> {
        return await invoke('start_cloud_auth');
    }

    async exchangeLinkCodeApi(token: string): Promise<CloudConfig> {
        return await invoke('exchange_link_code_api', { token });
    }

    async connectWebSocket(): Promise<void> {
        return invoke('connect_websocket');
    }

    async disconnectWebSocket(): Promise<void> {
        return invoke('disconnect_websocket');
    }

    async fetchCloudAccount(): Promise<CloudConfig> {
        return await invoke('fetch_cloud_account');
    }

    async syncStashesApi(payload: unknown): Promise<unknown> {
        return await invoke('sync_stashes_api', { payload });
    }

    async syncContextsApi(payload: unknown): Promise<unknown> {
        return await invoke('sync_contexts_api', { payload });
    }

    /**
     * Checks if the app has macOS Screen Recording permission.
     * Returns true on non-macOS platforms where this permission is not needed.
     */
    async checkScreenRecordingPermission(): Promise<boolean> {
        return await invoke('check_screen_recording_permission');
    }

    /**
     * Opens macOS System Settings to the Screen Recording permission pane.
     * No-op on non-macOS platforms.
     */
    async openMacosScreenRecordingSettings(): Promise<void> {
        await invoke('open_macos_screen_recording_settings');
    }

    // Apple Intelligence
    async checkAppleIntelligenceAvailable(): Promise<boolean> {
        try {
            return await invoke<boolean>('check_apple_intelligence_available');
        } catch (e) {
            console.error('Failed to check Apple Intelligence availability:', e);
            return false;
        }
    }

    async appleIntelligenceEnhance(content: string, systemPrompt: string): Promise<string> {
        return invoke<string>('apple_intelligence_enhance', { content, systemPrompt });
    }

    async getSystemPrompt(): Promise<string> {
        return await invoke('get_system_prompt');
    }

    async getSystemPromptPath(): Promise<string> {
        return await invoke('get_system_prompt_path_str');
    }

    async checkSystemPromptExists(): Promise<boolean> {
        return await invoke('check_system_prompt_exists');
    }

    async createSystemPromptFile(): Promise<void> {
        return await invoke('create_system_prompt_file');
    }

    async createPromptFile(): Promise<void> {
        return await this.createSystemPromptFile();
    }

    async openSystemPromptFile(): Promise<void> {
        return await invoke('open_system_prompt_file');
    }
}

