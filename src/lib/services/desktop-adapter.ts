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
import type { IStorageService, StashItem, AppContext, Settings } from '../types';

export class DesktopStorageAdapter implements IStorageService {
    async saveStash(stash: StashItem): Promise<void> {
        await invoke('save_stash', { stash });
    }

    async saveStashes(stashesList: StashItem[]): Promise<void> {
        await invoke('save_stashes', { stashesList });
    }

    async loadStashes(): Promise<StashItem[]> {
        return await invoke('load_stashes');
    }

    async saveAsset(file: File): Promise<string> {
        const buffer = await file.arrayBuffer();
        const bytes = Array.from(new Uint8Array(buffer));
        return await invoke('save_asset', { name: file.name, data: bytes });
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

    async saveAssetFromPath(path: string): Promise<string> {
        return await invoke('save_asset_from_path', { path });
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

    async deleteCompletedStashes(): Promise<void> {
        await invoke('delete_completed_stashes');
    }
}
