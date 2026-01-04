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
export class DesktopStorageAdapter {
    async saveStash(stash) {
        await invoke('save_stash', { stash });
    }
    async saveStashes(stashesList) {
        await invoke('save_stashes', { stashesList });
    }
    async loadStashes() {
        return await invoke('load_stashes');
    }
    async saveAsset(file) {
        const buffer = await file.arrayBuffer();
        const bytes = new Uint8Array(buffer);
        return await invoke('save_asset', { name: file.name, data: bytes });
    }
    async getPreviousAppInfo() {
        return await invoke('get_previous_app_info');
    }
    async getSmartTransferTarget() {
        return await invoke('get_smart_transfer_target');
    }
    async copyToClipboard(text) {
        await invoke('copy_to_clipboard', { text });
    }
    async startDrag(text, files) {
        await invoke('start_drag', { text, files });
    }
    async saveAssetFromPath(path) {
        return await invoke('save_asset_from_path', { path });
    }
    /**
     * Reads a file and returns preview data based on its type.
     * Images return base64 data URI, videos return file path, text returns content.
     * @param path - Absolute path to the file
     * @returns Preview data including file type, content, and metadata
     */
    async readFileForPreview(path) {
        return await invoke('read_file_for_preview', { path });
    }
    async getSettings() {
        return await invoke('get_settings');
    }
    async saveSettings(settings) {
        await invoke('save_settings', { settings });
    }
    async deleteStash(id) {
        await invoke('delete_stash', { id });
    }
    async deleteCompletedStashes() {
        await invoke('delete_completed_stashes');
    }
}
