<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann
-->

<script lang="ts">
    import { _, locale } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { save } from "@tauri-apps/plugin-dialog";
    import { stat } from "@tauri-apps/plugin-fs";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { Context, StashItem } from "$lib/types";
    import { getRelativeTime } from "$lib/utils/date";
    import { formatBytes } from "$lib/utils/format";
    import { Download, FileArchive, Square, CheckSquare } from "lucide-svelte";

    let {
        open = $bindable(false),
        context,
        stashes,
        onClose,
    } = $props<{
        open: boolean;
        context: Context;
        stashes: StashItem[];
        onClose: () => void;
    }>();

    // Track selected stashes - active stashes checked by default, completed unchecked
    const adapter = new DesktopStorageAdapter();

    let selectedIds = $state<Set<string>>(new Set());
    let includeAttachments = $state(false);
    let isExporting = $state(false);
    let totalAttachmentSize = $state(0);
    let isCalculatingSize = $state(false);

    // Initialize selection when dialog opens
    let hasOpened = $state(false);
    $effect(() => {
        if (open) {
            hasOpened = true;
            // Reset selection: check active stashes, uncheck completed
            selectedIds = new Set(
                stashes.filter((s) => !s.completed).map((s) => s.id),
            );
            includeAttachments = false;
            isExporting = false;
        } else if (hasOpened) {
            // Ensure parent state is synchronized when dialog is closed/dismissed
            // via internal mechanisms (Esc key, backdrop click)
            onClose();
        }
    });

    // Derived stats
    let activeStashes = $derived(
        stashes
            .filter((s) => !s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            ),
    );
    let completedStashes = $derived(
        stashes
            .filter((s) => s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            ),
    );
    let selectedStashes = $derived(
        stashes.filter((s) => selectedIds.has(s.id)),
    );
    // Rough size of the generated markdown. It used to build the whole document twice
    // on every render just to measure it; the document is written in Rust now, and an
    // estimate is all this label needs.
    let markdownEstimate = $derived(
        selectedStashes.reduce(
            (sum, s) => sum + s.content.length + 80,
            200,
        ),
    );
    let totalAttachments = $derived(
        selectedStashes.reduce(
            (sum, s) =>
                sum + (s.files?.length || 0) + (s.attachments?.length || 0),
            0,
        ),
    );

    // Calculate total file size when selection changes
    $effect(() => {
        // Get sizes from attachment objects (stored in DB)
        const attachmentSizes = selectedStashes.flatMap((s) =>
            (s.attachments || []).map((a) => a.fileSize || 0),
        );

        // Legacy files still need stat() calls
        const legacyFilePaths = selectedStashes.flatMap((s) => s.files || []);

        if (attachmentSizes.length === 0 && legacyFilePaths.length === 0) {
            totalAttachmentSize = 0;
            return;
        }

        // Sum attachment sizes immediately
        const attachmentTotal = attachmentSizes.reduce((a, b) => a + b, 0);

        if (legacyFilePaths.length === 0) {
            totalAttachmentSize = attachmentTotal;
            return;
        }

        // Only use stat() for legacy files
        isCalculatingSize = true;
        Promise.all(
            legacyFilePaths.map(async (filePath) => {
                try {
                    const info = await stat(filePath);
                    return info.size;
                } catch {
                    return 0;
                }
            }),
        ).then((sizes) => {
            totalAttachmentSize =
                attachmentTotal + sizes.reduce((a, b) => a + b, 0);
            isCalculatingSize = false;
        });
    });

    /**
     * Format bytes as human readable string with locale-aware number formatting.
     * Wrapper around the shared utility that uses the current locale.
     */
    function formatBytesLocalized(bytes: number): string {
        return formatBytes(bytes, $locale || "en");
    }

    /**
     * Toggle selection for a single stash
     */
    function toggleStash(id: string) {
        if (selectedIds.has(id)) {
            selectedIds.delete(id);
        } else {
            selectedIds.add(id);
        }
        // Trigger reactivity
        selectedIds = new Set(selectedIds);
    }

    /**
     * Select all stashes
     */
    function selectAll() {
        selectedIds = new Set(stashes.map((s) => s.id));
    }

    /**
     * Deselect all stashes
     */
    function deselectAll() {
        selectedIds = new Set();
    }

    /**
     * Export the selected stashes.
     *
     * The archive is built in Rust: it reads the attachments from disk itself, so their
     * bytes never cross IPC, and the deflate work stays off the UI thread. Doing it here
     * with JSZip froze the window for the duration.
     */
    async function handleExport() {
        if (selectedIds.size === 0) return;

        const asZip = includeAttachments && totalAttachments > 0;

        const safeName = context.name
            .replace(/[^a-zA-Z0-9_-]/g, "_")
            .toLowerCase();
        const now = new Date();
        const date = now.toISOString().slice(0, 10);
        const time = now.toTimeString().slice(0, 5).replace(":", "-");
        const defaultFileName = `${safeName}_${date}_${time}.${asZip ? "zip" : "md"}`;

        const filePath = await save({
            title: $_("contexts.exportTitle"),
            defaultPath: defaultFileName,
            filters: asZip
                ? [{ name: "ZIP Archive", extensions: ["zip"] }]
                : [{ name: "Markdown", extensions: ["md"] }],
        });

        if (!filePath) return;

        isExporting = true;
        try {
            await adapter.exportContextArchive(
                context.id,
                [...selectedIds],
                asZip,
                filePath,
            );
            handleClose();
        } catch (e) {
            console.error("Export failed:", e);
        } finally {
            isExporting = false;
        }
    }


    /**
     * Handle dialog close
     */
    function handleClose() {
        open = false;
    }

    /**
     * Get preview text for a stash (truncated)
     */
    function getPreviewText(stash: StashItem): string {
        const text = stash.content.trim();
        if (!text) return $_("stashCard.emptyStash");
        if (text.length > 60) return text.slice(0, 60) + "…";
        return text;
    }
</script>

<Dialog.Root bind:open onOpenChange={(v) => (open = v)}>
    <Dialog.Portal>
        <Dialog.Overlay
            class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm animate-in fade-in-0"
        />
        <Dialog.Content
            class="fixed left-[50%] top-[50%] z-[100] w-full max-w-2xl translate-x-[-50%] translate-y-[-50%] outline-none max-h-[85vh] flex flex-col animate-in zoom-in-95 fade-in-0 duration-200"
        >
            <div
                class="bg-popover text-popover-foreground border-border border shadow-lg rounded-lg flex flex-col overflow-hidden"
            >
                <!-- Header -->
                <div class="px-4 py-3 border-b border-border shrink-0">
                    <Dialog.Title
                        class="text-base font-semibold block tracking-tight"
                    >
                        {$_("contexts.exportDialog.title")}: {context.name}
                    </Dialog.Title>
                    <Dialog.Description
                        class="text-xs text-muted-foreground mt-0.5"
                    >
                        {$_("contexts.exportDialog.selectStashes")}
                    </Dialog.Description>
                </div>

                <!-- Stash List -->
                <div class="flex-1 overflow-y-auto px-4 py-2 max-h-[50vh]">
                    <!-- Active Stashes -->
                    {#if activeStashes.length > 0}
                        <div class="mb-3">
                            <div
                                class="text-[10px] uppercase tracking-wider text-muted-foreground font-medium mb-1.5 px-1"
                            >
                                {$_("queue.active")} ({activeStashes.length})
                            </div>
                            <div class="space-y-0.5">
                                {#each activeStashes as stash (stash.id)}
                                    <button
                                        type="button"
                                        class="w-full flex items-center gap-2 px-2 py-1.5 rounded transition-colors text-left
                                            {selectedIds.has(stash.id)
                                            ? 'bg-primary/10'
                                            : 'hover:bg-muted/50'}"
                                        onclick={() => toggleStash(stash.id)}
                                    >
                                        <div class="shrink-0">
                                            {#if selectedIds.has(stash.id)}
                                                <CheckSquare
                                                    size={14}
                                                    class="text-primary"
                                                />
                                            {:else}
                                                <Square
                                                    size={14}
                                                    class="text-muted-foreground"
                                                />
                                            {/if}
                                        </div>
                                        <span class="flex-1 text-sm truncate"
                                            >{getPreviewText(stash)}</span
                                        >
                                        <span
                                            class="text-[10px] text-muted-foreground shrink-0"
                                            >{getRelativeTime(
                                                stash.createdAt,
                                                $_,
                                            )}</span
                                        >
                                        {#if stash.files && stash.files.length > 0}
                                            <span
                                                class="text-[10px] text-muted-foreground shrink-0"
                                            >
                                                📎{stash.files.length}
                                            </span>
                                        {/if}
                                    </button>
                                {/each}
                            </div>
                        </div>
                    {/if}

                    <!-- Completed Stashes -->
                    {#if completedStashes.length > 0}
                        <div>
                            <div
                                class="text-[10px] uppercase tracking-wider text-muted-foreground font-medium mb-1.5 px-1"
                            >
                                {$_("queue.completed")} ({completedStashes.length})
                            </div>
                            <div class="space-y-0.5">
                                {#each completedStashes as stash (stash.id)}
                                    <button
                                        type="button"
                                        class="w-full flex items-center gap-2 px-2 py-1.5 rounded transition-colors text-left opacity-60
                                            {selectedIds.has(stash.id)
                                            ? 'bg-primary/10'
                                            : 'hover:bg-muted/50'}"
                                        onclick={() => toggleStash(stash.id)}
                                    >
                                        <div class="shrink-0">
                                            {#if selectedIds.has(stash.id)}
                                                <CheckSquare
                                                    size={14}
                                                    class="text-primary"
                                                />
                                            {:else}
                                                <Square
                                                    size={14}
                                                    class="text-muted-foreground"
                                                />
                                            {/if}
                                        </div>
                                        <span
                                            class="flex-1 text-sm truncate line-through"
                                            >{getPreviewText(stash)}</span
                                        >
                                        <span
                                            class="text-[10px] text-muted-foreground shrink-0"
                                            >{getRelativeTime(
                                                stash.createdAt,
                                                $_,
                                            )}</span
                                        >
                                        {#if stash.files && stash.files.length > 0}
                                            <span
                                                class="text-[10px] text-muted-foreground shrink-0"
                                            >
                                                📎{stash.files.length}
                                            </span>
                                        {/if}
                                    </button>
                                {/each}
                            </div>
                        </div>
                    {/if}

                    {#if stashes.length === 0}
                        <div
                            class="text-center py-8 text-muted-foreground text-sm"
                        >
                            {$_("contexts.exportDialog.noStashesSelected")}
                        </div>
                    {/if}
                </div>

                <!-- Footer -->
                <div class="p-4 border-t border-border space-y-4 shrink-0">
                    <!-- Selection controls -->
                    <div class="flex items-center justify-between text-xs">
                        <div class="flex gap-2">
                            <button
                                type="button"
                                class="text-primary hover:underline"
                                onclick={selectAll}
                            >
                                {$_("contexts.exportDialog.selectAll")}
                            </button>
                            <span class="text-muted-foreground">|</span>
                            <button
                                type="button"
                                class="text-primary hover:underline"
                                onclick={deselectAll}
                            >
                                {$_("contexts.exportDialog.deselectAll")}
                            </button>
                        </div>
                        <div class="text-muted-foreground">
                            {selectedIds.size} / {stashes.length}
                        </div>
                    </div>

                    <!-- Include attachments toggle -->
                    {#if totalAttachments > 0 || selectedStashes.some((s) => s.files?.length)}
                        <label
                            class="flex items-center gap-3 p-3 rounded-lg border border-border hover:bg-muted/50 cursor-pointer transition-colors"
                        >
                            <input
                                type="checkbox"
                                bind:checked={includeAttachments}
                                class="w-4 h-4 rounded border-border accent-primary"
                            />
                            <div class="flex items-center gap-2 text-sm">
                                <FileArchive
                                    size={16}
                                    class="text-muted-foreground"
                                />
                                <span
                                    >{$_(
                                        "contexts.exportDialog.includeAttachments",
                                    )}</span
                                >
                                {#if totalAttachments > 0}
                                    <span class="text-xs text-muted-foreground">
                                        ({totalAttachments}
                                        {totalAttachments === 1
                                            ? $_("contexts.exportDialog.file")
                                            : $_(
                                                  "contexts.exportDialog.files",
                                              )}{#if totalAttachmentSize > 0}
                                            • {formatBytesLocalized(
                                                totalAttachmentSize,
                                            )}
                                        {:else if isCalculatingSize}
                                            • <span class="animate-pulse"
                                                >...</span
                                            >
                                        {/if})
                                    </span>
                                {/if}
                            </div>
                        </label>
                    {/if}

                    <!-- Action buttons -->
                    <div class="flex items-center justify-end gap-4">
                        {#if isCalculatingSize}
                            <span
                                class="text-xs text-muted-foreground animate-pulse"
                                >{$_("common.calculating")}...</span
                            >
                        {:else if selectedIds.size > 0}
                            <span class="text-xs text-muted-foreground">
                                {$_("contexts.exportDialog.estimatedSize")}: {formatBytesLocalized(
                                    includeAttachments
                                        ? totalAttachmentSize + markdownEstimate
                                        : markdownEstimate,
                                )}
                            </span>
                        {/if}

                        <div class="flex gap-2">
                            <button
                                type="button"
                                class="px-3 py-2 text-sm font-medium hover:bg-muted rounded-md transition-colors"
                                onclick={handleClose}
                            >
                                {$_("common.cancel")}
                            </button>
                            <button
                                type="button"
                                class="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
                                onclick={handleExport}
                                disabled={selectedIds.size === 0 || isExporting}
                            >
                                <Download size={16} />
                                {isExporting
                                    ? $_("common.loading")
                                    : $_("contexts.exportDialog.export")}
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
