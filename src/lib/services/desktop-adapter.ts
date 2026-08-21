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

import { invoke } from '@tauri-apps/api/core';
import type { IStorageService, StashItem, AppContext, Settings, FilePreviewData, Context, Attachment, CloudConfig, ExportSummary, ImportPreview, CloudUsage, StashPosition } from '../types';

/** Called after any local write so cloud sync can be scheduled. */
type MutationListener = () => void;

let mutationListener: MutationListener | null = null;

/**
 * Register a callback fired after every local mutation.
 *
 * Every component constructs its own `DesktopStorageAdapter`, so this module-level
 * hook is the one place that sees all writes. Scheduling sync from here rather than
 * from individual call sites is deliberate: `triggerSync()` previously had a single
 * caller — new-stash creation — so editing, completing, deleting, reordering, moving
 * between contexts, context CRUD, and attaching a file to an existing stash never
 * scheduled a sync at all and waited on the 15-minute fallback poll.
 */
export function setLocalMutationListener(listener: MutationListener | null): void {
    mutationListener = listener;
}

/** Notify the listener, never letting a sync failure break the local write. */
function notifyMutation(): void {
    try {
        mutationListener?.();
    } catch (e) {
        console.warn('[DesktopAdapter] Mutation listener failed:', e);
    }
}

export class DesktopStorageAdapter implements IStorageService {
    async saveStash(stash: StashItem, options?: { invertPosition?: boolean }): Promise<void> {
        const invertPosition = options?.invertPosition ?? false;
        // Wrap in 'options' object to match Rust SaveOptions struct
        await invoke('save_stash', { options: { stash, invertPosition } });
        notifyMutation();
    }

    async saveStashes(stashesList: StashItem[]): Promise<void> {
        await invoke('save_stashes', { stashesList });
        notifyMutation();
    }

    async loadStashes(): Promise<StashItem[]> {
        return await invoke('load_stashes');
    }

    /**
     * Write a context's stashes to `destPath`.
     *
     * Rust reads the attachments from disk and compresses them, so their bytes never
     * cross IPC and the deflate work stays off the UI thread.
     */
    async exportContextArchive(
        contextId: string,
        stashIds: string[],
        includeAttachments: boolean,
        destPath: string,
    ): Promise<ExportSummary> {
        return await invoke<ExportSummary>('export_context_archive', {
            contextId,
            stashIds,
            includeAttachments,
            destPath,
        });
    }

    /** Read an archive and report what importing it would bring in. */
    async readImportArchive(path: string, contextId: string): Promise<ImportPreview> {
        return await invoke<ImportPreview>('read_import_archive', { path, contextId });
    }

    /** Write the selected stashes and their files, in one transaction. */
    async commitImport(
        contextId: string,
        stashes: StashItem[],
        token: string,
    ): Promise<number> {
        const count = await invoke<number>('commit_import', { contextId, stashes, token });
        notifyMutation();
        return count;
    }

    /** Drop the files an abandoned import had extracted. */
    async discardImport(token: string): Promise<void> {
        await invoke('discard_import', { token });
    }

    /**
     * Save an asset file to the cache directory.
     * Files are organized hierarchically: cache/<contextId>/<stashId>/<filename>
     */
    /**
     * Save a file's bytes into the cache directory.
     *
     * The bytes go over IPC as a **raw body**, not as JSON. This used to be
     * `Array.from(new Uint8Array(buffer))`, which turned a pasted screenshot into a
     * multi-million-element JavaScript array and then had to `JSON.stringify` it - all
     * synchronously on the webview's main thread, which is what made pasting a large
     * image freeze the window. Metadata rides along in one URI-encoded header, because
     * a filename is arbitrary Unicode and header values are not.
     */
    async saveAsset(file: File, contextId?: string, stashId?: string, syntax?: string): Promise<Attachment> {
        const buffer = await file.arrayBuffer();
        const meta = encodeURIComponent(
            JSON.stringify({
                name: file.name,
                contextId: contextId ?? null,
                stashId: stashId ?? null,
                syntax: syntax ?? null,
            })
        );
        const attachment: Attachment = await invoke(
            'save_asset',
            new Uint8Array(buffer),
            { headers: { 'x-stashpad-asset': meta } }
        );
        notifyMutation();
        return attachment;
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
        const attachment: Attachment = await invoke('save_asset_from_path', {
            path,
            context_id: contextId ?? null,
            contextId: contextId ?? null,
            stash_id: stashId ?? null,
            stashId: stashId ?? null,
            syntax: syntax ?? null
        });
        notifyMutation();
        return attachment;
    }

    /**
     * Delete an asset file from the cache directory.
     * @param path - Absolute path to the file to delete
     */
    async deleteAsset(path: string): Promise<void> {
        await invoke('delete_asset', { path });
        notifyMutation();
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
        notifyMutation();
    }

    async deleteCompletedStashes(contextId?: string): Promise<void> {
        await invoke('delete_completed_stashes', { contextId });
        notifyMutation();
    }

    /** @returns how many stashes were removed. */
    async triggerAutoCleanup(): Promise<number> {
        const removed = await invoke<number>('trigger_auto_cleanup');
        if (removed > 0) notifyMutation();
        return removed;
    }

    async isWindows10(): Promise<boolean> {
        return await invoke('is_windows_10');
    }

    async getContexts(): Promise<Context[]> {
        return await invoke('get_contexts');
    }

    async saveContexts(contexts: Context[]): Promise<void> {
        await invoke('save_contexts', { contexts });
        notifyMutation();
    }

    async saveContext(context: Context): Promise<void> {
        await invoke('save_context', { context });
        notifyMutation();
    }

    async deleteContext(id: string): Promise<void> {
        await invoke('delete_context', { id });
        notifyMutation();
    }

    async setAutostart(enabled: boolean): Promise<void> {
        return await invoke('set_autostart', { enabled });
    }

    async getAutostartEnabled(): Promise<boolean> {
        return await invoke('get_autostart_enabled');
    }

    /**
     * Forward a frontend error to the Rust logger.
     *
     * The webview console is discarded in a release build, so without this an
     * uncaught render error leaves no trace anywhere on disk.
     */
    async logFrontendError(message: string): Promise<void> {
        return await invoke('log_frontend_error', { message });
    }

    async exchangeLinkCodeApi(token: string, deviceId?: string): Promise<CloudConfig> {
        return await invoke('exchange_link_code_api', { token, deviceId });
    }

    async connectWebSocket(): Promise<void> {
        return invoke('connect_websocket');
    }

    async disconnectWebSocket(): Promise<void> {
        return invoke('disconnect_websocket');
    }

    /** What this account is storing in the cloud. Fetched on demand, never cached. */
    async fetchCloudUsage(): Promise<CloudUsage> {
        return await invoke<CloudUsage>('fetch_cloud_usage');
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

    async uploadAttachmentToCloud(attachmentId: string): Promise<boolean> {
        return await invoke('upload_attachment_to_cloud', { attachmentId });
    }

    async getDeviceName(): Promise<string> {
        return await invoke('get_device_name');
    }

    async loadStashesForSync(): Promise<StashItem[]> {
        return await invoke('load_stashes_for_sync');
    }

    async getContextsForSync(): Promise<Context[]> {
        return await invoke('get_contexts_for_sync');
    }

    async importStashes(stashes: StashItem[]): Promise<void> {
        await invoke('import_stashes', { stashesList: stashes });
    }

    /**
     * Apply contexts pulled from the cloud, preserving their server timestamps.
     * Distinct from `saveContext`, which is the local-edit path and stamps "now".
     */
    /** Stashes with local changes the server has not acknowledged yet. */
    async claimPendingStashes(): Promise<StashItem[]> {
        return await invoke('claim_pending_stashes');
    }

    /** Contexts with local changes the server has not acknowledged yet. */
    async claimPendingContexts(): Promise<Context[]> {
        return await invoke('claim_pending_contexts');
    }

    /** Clear the pending flag for stashes the server accepted. */
    async markStashesSynced(ids: string[]): Promise<void> {
        await invoke('mark_stashes_synced', { ids });
    }

    /** Orderings this device changed, marked in flight. */
    async claimPendingPositions(): Promise<StashPosition[]> {
        return await invoke<StashPosition[]>('claim_pending_positions');
    }

    /** Clear the pending flag for orderings the server accepted. */
    async markPositionsSynced(ids: string[]): Promise<void> {
        await invoke('mark_positions_synced', { ids });
    }

    /** Apply orderings from other devices; resolves to how many rows moved. */
    async importPositions(positions: StashPosition[]): Promise<number> {
        return await invoke<number>('import_positions', { positions });
    }

    /** Clear the pending flag for contexts the server accepted. */
    async markContextsSynced(ids: string[]): Promise<void> {
        await invoke('mark_contexts_synced', { ids });
    }

    async importContexts(contexts: Context[]): Promise<void> {
        await invoke('import_contexts', { contexts });
    }

    /** Fetch an attachment's bytes into the local cache; returns the file path. */
    async downloadAttachmentFromCloud(attachmentId: string): Promise<string> {
        return await invoke('download_attachment_from_cloud', { attachmentId });
    }

    /** Sign out of the cloud and erase the stored JWT from the OS keychain. */
    async cloudLogout(): Promise<void> {
        await invoke('cloud_logout');
    }
}

