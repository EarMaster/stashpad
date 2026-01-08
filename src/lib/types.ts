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

export interface Attachment {
    id: string;
    stashId: string;
    filePath: string;
    fileName: string;
    fileSize: number;
    mimeType?: string;
    syntax?: string;
    createdAt: string;
}

export interface StashItem {
    id: string;
    content: string;
    attachments: Attachment[];
    files?: string[]; // Deprecated, kept for backward compatibility during migration
    createdAt: string;
    contextId?: string;
    completed?: boolean;
    completedAt?: string; // ISO Date string
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
    // contexts moved to separate store
    activeContextId?: string | null;
    shortcuts: Record<string, string>;
    /** Locale preference: 'auto' for automatic detection or a specific locale code */
    locale?: 'auto' | string;
    /** Where to put new stashes and newly completed stashes */
    newStashPosition?: 'top' | 'bottom';
    theme?: 'light' | 'dark' | 'system';
    /** Scale of the UI: 1-5, default 3 */
    uiScale?: number;
    videoVolume?: number;
    videoMuted?: boolean;
    /** Strip #tags when copying to clipboard */
    stripTagsOnCopy?: boolean;
    /** Strategy for automatically clearing completed stashes */
    clearCompletedStrategy?: 'never' | 'on-close' | 'after-n-days';
    /** Number of days to keep completed stashes (if strategy is after-n-days) */
    clearCompletedDays?: number;
    /** Number of lines of pasted text before it becomes an attachment. 0 = ask user, default 8 */
    pasteAsAttachmentThreshold?: number;
    /** Last used timestamp for the default context */
    defaultContextLastUsed?: string;
    /** Launch Stashpad automatically on system startup */
    autostart?: boolean;
}

export interface IStorageService {
    saveStash(stash: StashItem, options?: { invertPosition?: boolean }): Promise<void>;
    saveStashes(stashes: StashItem[]): Promise<void>;
    loadStashes(): Promise<StashItem[]>;
    /**
     * Save an asset file to the cache directory.
     * Files are stored in a hierarchical structure: cache/<contextId>/<stashId>/<filename>
     * @param file - The file to save
     * @param contextId - The context ID for folder organization
     * @param stashId - The stash ID for folder organization
     * @param syntax - Optional detected syntax/language
     * @returns The saved attachment metadata
     */
    saveAsset(file: File, contextId?: string, stashId?: string, syntax?: string): Promise<Attachment>;
    getPreviousAppInfo(): Promise<AppContext>;
    getSmartTransferTarget(): Promise<'GUI' | 'CLI'>;
    copyToClipboard(text: string): Promise<void>;
    startDrag(text: string, files: string[]): Promise<void>;
    /**
     * Import an asset from an external file path into the cache directory.
     * Files are stored in a hierarchical structure: cache/<contextId>/<stashId>/<filename>
     * @param path - The source file path
     * @param contextId - The context ID for folder organization
     * @param stashId - The stash ID for folder organization
     * @param syntax - Optional detected syntax/language
     * @returns The saved attachment metadata
     */
    saveAssetFromPath(path: string, contextId?: string, stashId?: string, syntax?: string): Promise<Attachment>;
    readFileForPreview(path: string): Promise<FilePreviewData>;
    getSettings(): Promise<Settings>;
    saveSettings(settings: Settings): Promise<void>;
    deleteStash(id: string): Promise<void>;
    deleteCompletedStashes(contextId?: string): Promise<void>;
    triggerAutoCleanup(): Promise<void>;
    isWindows10(): Promise<boolean>;

    // Context management
    getContexts(): Promise<Context[]>;
    saveContexts(contexts: Context[]): Promise<void>;
    saveContext(context: Context): Promise<void>;
    deleteContext(id: string): Promise<void>;
    setAutostart(enabled: boolean): Promise<void>;
    getAutostartEnabled(): Promise<boolean>;
}

/**
 * Data structure for file preview information.
 * Returned by the readFileForPreview method.
 */
export interface FilePreviewData {
    /** Type of file: "image", "video", "text", or "unsupported" */
    fileType: 'image' | 'video' | 'text' | 'unsupported';
    /** 
     * Content varies by type:
     * - image: base64 data URI
     * - video: file path (convert to asset:// URL)
     * - text: file content (max 10KB)
     * - unsupported: empty string
     */
    content: string;
    /** Original file name */
    fileName: string;
    /** MIME type of the file */
    mimeType: string;
    /** File size in bytes */
    fileSize: number;
}


