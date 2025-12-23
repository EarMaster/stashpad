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

export interface StashItem {
    id: string;
    content: string;
    files: string[];
    createdAt: string;
}

export interface AppContext {
    windowTitle: string;
    processName: string;
}

export interface IStorageService {
    saveStash(stash: StashItem): Promise<void>;
    loadStashes(): Promise<StashItem[]>;
    saveAsset(file: File): Promise<string>;
    getPreviousAppInfo(): Promise<AppContext>;
    getSmartTransferTarget(): Promise<'GUI' | 'CLI'>;
    copyToClipboard(text: string): Promise<void>;
    startDrag(text: string, files: string[]): Promise<void>;
    saveAssetFromPath(path: string): Promise<string>;
}
