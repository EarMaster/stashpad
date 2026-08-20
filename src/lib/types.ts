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
    /** AI-enhanced version of the content (if generated) */
    enhancedContent?: string;
    attachments: Attachment[];
    files?: string[]; // Deprecated, kept for backward compatibility during migration
    createdAt: string;
    contextId?: string;
    completed?: boolean;
    completedAt?: string; // ISO Date string
    updatedAt?: string | number; // ISO Date string (string) or Unix timestamp (number)
    isDndShadowItem?: boolean; // Added by svelte-dnd-action during drag operations
    deleted?: boolean;
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
    matchCase?: boolean;
    useRegex?: boolean;
}

export interface Context {
    id: string;
    name: string;
    /** Optional description for AI context (tech stack, project info) */
    description?: string;
    rules: ContextRule[];
    lastUsed?: string;
    updatedAt?: string | number;
    deleted?: boolean;
}

/** Configuration for an OpenAI-compatible API provider preset */
export interface AIProviderPreset {
    id: string;
    name: string;
    endpoint: string;
    defaultModel: string;
}

/** User's AI configuration for prompt enhancement */
export interface AIConfig {
    enabled: boolean;
    endpoint: string;
    apiKey: string;
    model: string;
    /** Which preset was used, if any */
    presetId?: string;
}

/** Configuration for Stashpad Cloud sync */
export interface CloudConfig {
    enabled: boolean;
    /** The root API endpoint for the cloud service */
    endpoint: string;
    /** The authenticated user's ID on the cloud service */
    userId?: string;
    /** The authenticated user's email on the cloud service */
    email?: string;
    /** The JWT token for authentication (stored in memory/secure storage) - No longer available in Frontend */
    accessToken?: never;
    /** Subscription tier: 'pro' */
    subscriptionTier?: string;
    /** Subscription status: 'active', 'canceled', etc. */
    subscriptionStatus?: string;
    /** When the current billing period ends */
    subscriptionPeriodEnd?: string;
    /** Enterprise owner ID if part of a team */
    enterpriseOwnerId?: string | null;
    /** Last sync timestamp */
    lastSyncAt?: string;
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
    /** Number of bytes of pasted text before it becomes an attachment. 0 = ask user, default 500 */
    pasteAsAttachmentThreshold?: number;
    /** Last used timestamp for the default context */
    defaultContextLastUsed?: string;
    /** Launch Stashpad automatically on system startup */
    autostart?: boolean;
    /** AI configuration for prompt enhancement */
    aiConfig?: AIConfig;
    resizeImages?: boolean;
    /** Cloud configuration for synchronization */
    cloudConfig?: CloudConfig;
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
    // WebSockets
    connectWebSocket(): Promise<void>;
    disconnectWebSocket(): Promise<void>;
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
    /** Runs the completed-stash cleanup; resolves to how many stashes were removed. */
    triggerAutoCleanup(): Promise<number>;

    /** Write a context to a file; the archive is built in Rust. */
    exportContextArchive(contextId: string, stashIds: string[], includeAttachments: boolean, destPath: string): Promise<ExportSummary>;
    /** Inspect an archive without importing it. */
    readImportArchive(path: string, contextId: string): Promise<ImportPreview>;
    /** Write the selected stashes and their files in one transaction. */
    commitImport(contextId: string, stashes: StashItem[], token: string): Promise<number>;
    /** Drop the files an abandoned import had extracted. */
    discardImport(token: string): Promise<void>;
    isWindows10(): Promise<boolean>;
    getDeviceName(): Promise<string>;

    // Context management
    getContexts(): Promise<Context[]>;
    saveContexts(contexts: Context[]): Promise<void>;
    saveContext(context: Context): Promise<void>;
    deleteContext(id: string): Promise<void>;
    setAutostart(enabled: boolean): Promise<void>;
    getAutostartEnabled(): Promise<boolean>;
    startCloudAuth(): Promise<CloudConfig>;
    exchangeLinkCodeApi(token: string): Promise<CloudConfig>;
    /** Fetch account info from cloud and update local subscription status */
    fetchCloudAccount(): Promise<CloudConfig>;
    /** What this account is storing in the cloud. */
    fetchCloudUsage(): Promise<CloudUsage>;

    // Cloud sync proxy methods
    syncStashesApi(payload: unknown): Promise<unknown>;
    syncContextsApi(payload: unknown): Promise<unknown>;
    loadStashesForSync(): Promise<StashItem[]>;
    getContextsForSync(): Promise<Context[]>;
    importStashes(stashes: StashItem[]): Promise<void>;
    /** Apply cloud contexts while preserving their server timestamps. */
    importContexts(contexts: Context[]): Promise<void>;
    /** Records changed locally, marked in flight so a concurrent edit stays queued. */
    claimPendingStashes(): Promise<StashItem[]>;
    claimPendingContexts(): Promise<Context[]>;
    /** Mark records the server accepted as synced. */
    markStashesSynced(ids: string[]): Promise<void>;
    markContextsSynced(ids: string[]): Promise<void>;
    /** Orderings this device changed, marked in flight. */
    claimPendingPositions(): Promise<StashPosition[]>;
    markPositionsSynced(ids: string[]): Promise<void>;
    /** Apply orderings from other devices; resolves to how many rows moved. */
    importPositions(positions: StashPosition[]): Promise<number>;
    /** Fetch an attachment's bytes into the local cache; resolves to the file path. */
    downloadAttachmentFromCloud(attachmentId: string): Promise<string>;
    /** Sign out of the cloud and erase the stored JWT from the OS keychain. */
    cloudLogout(): Promise<void>;

    // Apple Intelligence
    checkAppleIntelligenceAvailable(): Promise<boolean>;
    appleIntelligenceEnhance(content: string, systemPrompt: string): Promise<string>;
    // AI System Prompt management
    getSystemPrompt(): Promise<string>;
    getSystemPromptPath(): Promise<string>;
    checkSystemPromptExists(): Promise<boolean>;
    createSystemPromptFile(): Promise<void>;
    openSystemPromptFile(): Promise<void>;
    /** Uploads the attachment's bytes. Resolves true only if bytes were actually sent. */
    uploadAttachmentToCloud(attachmentId: string): Promise<boolean>;
}

/**
 * Data structure for file preview information.
 * Returned by the readFileForPreview method.
 */
/** Context metadata carried in an archive's YAML frontmatter. */
export interface ArchiveMetadata {
    name: string;
    description: string;
    rules: unknown[];
}

/** What an account is storing in the cloud. Attachment bytes are the only capped part. */
/**
 * One stash's place in the order.
 *
 * Travels apart from the record so a reorder never carries content with it: order and
 * content are merged independently, and a cosmetic move cannot overwrite an edit made
 * on another device.
 */
export interface StashPosition {
    id: string;
    position: number;
    /** Client clock, Unix seconds - the Last-Write-Wins discriminator for ordering. */
    positionUpdatedAt: number;
}

export interface CloudUsage {
    stashes: number;
    contexts: number;
    attachments: number;
    attachmentBytes: number;
    quotaBytes: number;
    overQuota: boolean;
}

export interface ExportSummary {
    stashes: number;
    attachments: number;
    path: string;
}

/** What an archive turned out to contain, for the conflict UI to act on. */
export interface ImportPreview {
    stashes: StashItem[];
    metadata: ArchiveMetadata;
    /** Ids of parsed stashes resembling something the context already holds. */
    duplicateIds: string[];
    /** Handle for the extracted files; pass back to commitImport or discardImport. */
    token: string;
    /**
     * How many stash headings carried a date that could not be read.
     *
     * Those stashes fall back to the import time. The previous importer did the same
     * silently, so an unreadable archive lost every creation date without a word.
     */
    unreadableDates: number;
}

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


