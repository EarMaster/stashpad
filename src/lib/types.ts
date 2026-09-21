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
    /** When the updater last completed a check, epoch milliseconds */
    lastUpdateCheckAt?: number;
    /** Newest version the updater has seen, used to restore the header notice on launch */
    latestKnownUpdateVersion?: string;
    /** Version the user chose to skip; the notice returns for anything newer */
    dismissedUpdateVersion?: string;
    /** "Remind me later": epoch milliseconds before which the notice stays hidden */
    updateRemindAfter?: number;
    /** Whether the periodic background update check runs at all */
    autoUpdateChecks?: boolean;
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
    /**
     * Delete an attachment: its row, and its file once no other row references it.
     * @param id - The attachment id; empty for one never written to the database
     * @param path - Absolute path to the file, which must lie inside the cache directory
     */
    deleteAsset(id: string, path: string): Promise<void>;
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
    /**
     * The stable identifier this installation is known by on the server.
     *
     * `migrateFrom` offers up whatever the webview still holds from before the id was
     * kept on disk, so an existing installation keeps the identity the server already
     * has for it instead of registering itself a second time.
     */
    getDeviceId(migrateFrom?: string): Promise<string>;

    // Context management
    getContexts(): Promise<Context[]>;
    saveContexts(contexts: Context[]): Promise<void>;
    saveContext(context: Context): Promise<void>;
    deleteContext(id: string): Promise<void>;
    setAutostart(enabled: boolean): Promise<void>;
    getAutostartEnabled(): Promise<boolean>;
    /** Forward a frontend error to the backend logger. */
    logFrontendError(message: string): Promise<void>;
    exchangeLinkCodeApi(token: string, deviceId?: string): Promise<CloudConfig>;
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

    // Device passphrase - only ever anything but "notNeeded" on a machine with no OS
    // credential store, where a typed passphrase is the only real protection available.
    localKeyStatus(): Promise<LocalKeyStatus>;
    localKeyIsRemembered(): Promise<boolean>;
    /** Resolves false when the passphrase was simply wrong. */
    unlockLocalKey(passphrase: string): Promise<boolean>;
    setLocalPassphrase(passphrase: string, remember: boolean): Promise<void>;
    setLocalKeyRemembered(remember: boolean): Promise<void>;
    declineLocalKey(): Promise<void>;

    // Content encryption. Every one of these is a no-op for an account that has not
    // turned it on, which is the default.
    e2eeStatus(): Promise<E2eeStatus>;
    e2eeRegisterDevice(): Promise<string>;
    /** Returns the recovery code, shown once and never retrievable again. */
    e2eeEnable(): Promise<E2eeEnableResult>;
    e2eeApproveDevice(deviceId: string, expectedFingerprint: string): Promise<void>;
    e2eeRecover(code: string): Promise<void>;
    e2eeAcknowledgeRecovery(): Promise<void>;
    /** Queues every local record for re-encryption. Returns how many. */
    e2eeStartConversion(): Promise<number>;
    /**
     * How much of that queue is left. Safe to poll — the sweep is carried by ordinary
     * syncs, so nothing notifies the interface when it advances or finishes.
     */
    e2eeConversionProgress(): Promise<E2eeConversionProgress>;
    e2eeSeal(): Promise<void>;
    /**
     * Mints an MCP access key on this machine and seals the content key to it. The secret
     * comes back once and cannot be retrieved afterwards.
     */
    e2eeCreateAccessKey(
        name: string,
        scope: "read" | "readwrite",
        expiresInDays?: number,
    ): Promise<CreatedAccessKey>;
}

export interface CreatedAccessKey {
    /** Show once. There is no way to get it again. */
    key: string;
    id: string;
    name: string;
    scope: string;
}

/** Where an account and this installation stand on encryption. */
export interface E2eeConversionProgress {
    /** Records the server has not acknowledged yet, in-flight ones included. */
    remaining: number;
    /**
     * Every live record. Counted fresh rather than remembered from when the sweep began,
     * so the figure is still right after the window is closed and reopened.
     */
    total: number;
}

export interface E2eeStatus {
    /** 0 when encryption has never been turned on. */
    epoch: number;
    /** "off", "migrating" or "sealed". */
    state: string;
    /** Whether this installation holds the content key right now. */
    unlocked: boolean;
    /** Whether the server holds a copy of the key for this installation. */
    enrolled: boolean;
    hasRecovery: boolean;
    recoveryAcknowledged: boolean;
    /** This installation's fingerprint, for the user to compare against another screen. */
    fingerprint: string;
    devices: E2eeDevice[];
}

export interface E2eeDevice {
    deviceId: string;
    publicKey: string;
    /**
     * Recomputed on this machine from the published key, never the server's stored copy -
     * comparing two numbers the server supplied would verify nothing.
     */
    fingerprint: string;
    /** "pending" or "active". */
    status: string;
    enrolledAt: string;
}

export interface E2eeEnableResult {
    recoveryCode: string;
    fingerprint: string;
}

/**
 * Where the device passphrase stands on this machine.
 *
 * - `notNeeded` - the OS credential store works, so no passphrase is involved
 * - `unset` - no credential store and the user has not chosen yet
 * - `locked` - a passphrase is set but has not been entered this session
 * - `unlocked` - the key is in memory
 * - `declined` - the user chose no passphrase; secrets are not written to disk
 */
export type LocalKeyStatus = "notNeeded" | "unset" | "locked" | "unlocked" | "declined";

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


