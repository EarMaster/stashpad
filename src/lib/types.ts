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
    contextId?: string;
    completed?: boolean;
    isDndShadowItem?: boolean; // Added by svelte-dnd-action during drag operations
}

export interface AppContext {
    windowTitle: string;
    processName: string;
    detectedContextId?: string;
}

export interface ContextRule {
    ruleType: 'process' | 'title';
    value: string;
    matchType: 'contains' | 'exact';
}

export interface Context {
    id: string;
    name: string;
    rules: ContextRule[];
    lastUsed?: string;
}

export interface Settings {
    autoContextDetection: boolean;
    visualEffectsEnabled?: boolean;
    contexts: Context[];
    activeContextId?: string | null;
    shortcuts: Record<string, string>;
    /** Locale preference: 'auto' for automatic detection or a specific locale code */
    locale?: 'auto' | string;
    /** Where to put new stashes and newly completed stashes */
    newStashPosition?: 'top' | 'bottom';
}

export interface IStorageService {
    saveStash(stash: StashItem): Promise<void>;
    saveStashes(stashes: StashItem[]): Promise<void>;
    loadStashes(): Promise<StashItem[]>;
    saveAsset(file: File): Promise<string>;
    getPreviousAppInfo(): Promise<AppContext>;
    getSmartTransferTarget(): Promise<'GUI' | 'CLI'>;
    copyToClipboard(text: string): Promise<void>;
    startDrag(text: string, files: string[]): Promise<void>;
    saveAssetFromPath(path: string): Promise<string>;
    getSettings(): Promise<Settings>;
    saveSettings(settings: Settings): Promise<void>;
    deleteStash(id: string): Promise<void>;
    deleteCompletedStashes(): Promise<void>;
}


