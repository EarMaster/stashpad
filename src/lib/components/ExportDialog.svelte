<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { save } from "@tauri-apps/plugin-dialog";
    import { writeTextFile, readFile } from "@tauri-apps/plugin-fs";
    import JSZip from "jszip";
    import type { Context, StashItem } from "$lib/types";
    import {
        Check,
        Download,
        FileArchive,
        Square,
        CheckSquare,
    } from "lucide-svelte";

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
    let selectedIds = $state<Set<string>>(new Set());
    let includeAttachments = $state(false);
    let isExporting = $state(false);

    // Initialize selection when dialog opens
    $effect(() => {
        if (open) {
            // Reset selection: check active stashes, uncheck completed
            selectedIds = new Set(
                stashes.filter((s) => !s.completed).map((s) => s.id),
            );
            includeAttachments = false;
            isExporting = false;
        }
    });

    // Derived stats
    let activeCount = $derived(stashes.filter((s) => !s.completed).length);
    let completedCount = $derived(stashes.filter((s) => s.completed).length);
    let selectedStashes = $derived(
        stashes.filter((s) => selectedIds.has(s.id)),
    );
    let totalAttachments = $derived(
        selectedStashes.reduce((sum, s) => sum + (s.files?.length || 0), 0),
    );

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
     * Build markdown content from selected stashes
     */
    function buildMarkdownContent(): string {
        const lines: string[] = [];
        lines.push(`# ${context.name}`);
        lines.push("");
        lines.push(
            `Exported from Stashpad on ${new Date().toLocaleDateString()}`,
        );
        lines.push("");
        lines.push(`Total stashes: ${selectedStashes.length}`);
        lines.push("");
        lines.push("---");
        lines.push("");

        // Sort by created date (newest first)
        const sorted = [...selectedStashes].sort(
            (a, b) =>
                new Date(b.createdAt).getTime() -
                new Date(a.createdAt).getTime(),
        );

        for (const stash of sorted) {
            const date = new Date(stash.createdAt).toLocaleString();
            const status = stash.completed ? "✓ Completed" : "Active";
            lines.push(`## [${status}] ${date}`);
            lines.push("");

            if (stash.content.trim()) {
                lines.push(stash.content);
                lines.push("");
            }

            if (stash.files && stash.files.length > 0) {
                lines.push("**Attachments:**");
                for (const file of stash.files) {
                    const fileName = file.split(/[\\/]/).pop() || file;
                    if (includeAttachments) {
                        // Reference to file in attachments folder
                        lines.push(`- [${fileName}](attachments/${fileName})`);
                    } else {
                        lines.push(`- ${fileName}`);
                    }
                }
                lines.push("");
            }

            lines.push("---");
            lines.push("");
        }

        return lines.join("\n");
    }

    /**
     * Export as markdown only
     */
    async function exportMarkdown() {
        const markdownContent = buildMarkdownContent();

        const safeName = context.name
            .replace(/[^a-zA-Z0-9_-]/g, "_")
            .toLowerCase();
        const timestamp = new Date().toISOString().slice(0, 10);
        const defaultFileName = `${safeName}_${timestamp}.md`;

        const filePath = await save({
            title: $_("contexts.exportTitle"),
            defaultPath: defaultFileName,
            filters: [{ name: "Markdown", extensions: ["md"] }],
        });

        if (filePath) {
            await writeTextFile(filePath, markdownContent);
            handleClose();
        }
    }

    /**
     * Export as ZIP with attachments
     */
    async function exportZip() {
        const zip = new JSZip();
        const markdownContent = buildMarkdownContent();

        // Add markdown file
        zip.file("export.md", markdownContent);

        // Add attachments folder
        const attachmentsFolder = zip.folder("attachments");

        // Collect all files from selected stashes
        for (const stash of selectedStashes) {
            if (stash.files && stash.files.length > 0) {
                for (const filePath of stash.files) {
                    try {
                        const fileName =
                            filePath.split(/[\\/]/).pop() || filePath;
                        // Read file as binary
                        const fileData = await readFile(filePath);
                        attachmentsFolder?.file(fileName, fileData);
                    } catch (e) {
                        console.error(`Failed to read file ${filePath}:`, e);
                    }
                }
            }
        }

        // Generate zip blob
        const zipBlob = await zip.generateAsync({ type: "uint8array" });

        const safeName = context.name
            .replace(/[^a-zA-Z0-9_-]/g, "_")
            .toLowerCase();
        const timestamp = new Date().toISOString().slice(0, 10);
        const defaultFileName = `${safeName}_${timestamp}.zip`;

        const filePath = await save({
            title: $_("contexts.exportTitle"),
            defaultPath: defaultFileName,
            filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
        });

        if (filePath) {
            // Write zip file using Tauri fs
            const { writeFile } = await import("@tauri-apps/plugin-fs");
            await writeFile(filePath, zipBlob);
            handleClose();
        }
    }

    /**
     * Handle export button click
     */
    async function handleExport() {
        if (selectedIds.size === 0) return;

        isExporting = true;
        try {
            if (includeAttachments && totalAttachments > 0) {
                await exportZip();
            } else {
                await exportMarkdown();
            }
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
        onClose();
    }

    /**
     * Get preview text for a stash (truncated)
     */
    function getPreviewText(stash: StashItem): string {
        const text = stash.content.trim();
        if (!text) return $_("stashCard.emptyStash");
        if (text.length > 80) return text.slice(0, 80) + "…";
        return text;
    }

    /**
     * Format date for display
     */
    function formatDate(dateStr: string): string {
        return new Date(dateStr).toLocaleDateString();
    }
</script>

<Dialog.Root bind:open onOpenChange={(v) => (open = v)}>
    <Dialog.Portal>
        <Dialog.Overlay
            class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm animate-in fade-in-0"
        />
        <Dialog.Content
            class="fixed left-[50%] top-[50%] z-[100] w-full max-w-lg translate-x-[-50%] translate-y-[-50%] outline-none max-h-[80vh] flex flex-col animate-in zoom-in-95 fade-in-0 duration-200"
        >
            <div
                class="bg-popover text-popover-foreground border-border border shadow-lg rounded-lg flex flex-col overflow-hidden"
            >
                <!-- Header -->
                <div class="p-4 border-b border-border shrink-0">
                    <Dialog.Title
                        class="text-lg font-semibold block tracking-tight"
                    >
                        {$_("contexts.exportDialog.title")}
                    </Dialog.Title>
                    <Dialog.Description
                        class="text-sm text-muted-foreground mt-1"
                    >
                        {$_("contexts.exportDialog.selectStashes")}
                    </Dialog.Description>
                </div>

                <!-- Stash List -->
                <div class="flex-1 overflow-y-auto p-4 space-y-2 max-h-[40vh]">
                    {#each stashes as stash (stash.id)}
                        <button
                            type="button"
                            class="w-full flex items-start gap-3 p-3 rounded-lg border transition-colors text-left
                                {selectedIds.has(stash.id)
                                ? 'border-primary bg-primary/5'
                                : 'border-border hover:bg-muted/50'}"
                            onclick={() => toggleStash(stash.id)}
                        >
                            <!-- Checkbox -->
                            <div class="shrink-0 mt-0.5">
                                {#if selectedIds.has(stash.id)}
                                    <CheckSquare
                                        size={18}
                                        class="text-primary"
                                    />
                                {:else}
                                    <Square
                                        size={18}
                                        class="text-muted-foreground"
                                    />
                                {/if}
                            </div>

                            <!-- Content -->
                            <div class="flex-1 min-w-0">
                                <div
                                    class="flex items-center gap-2 text-xs text-muted-foreground mb-1"
                                >
                                    <span>{formatDate(stash.createdAt)}</span>
                                    {#if stash.completed}
                                        <span
                                            class="px-1.5 py-0.5 rounded bg-muted text-[10px]"
                                            >{$_("queue.completed")}</span
                                        >
                                    {/if}
                                    {#if stash.files && stash.files.length > 0}
                                        <span
                                            class="px-1.5 py-0.5 rounded bg-muted text-[10px]"
                                        >
                                            {stash.files.length}
                                            {stash.files.length === 1
                                                ? "file"
                                                : "files"}
                                        </span>
                                    {/if}
                                </div>
                                <p class="text-sm truncate">
                                    {getPreviewText(stash)}
                                </p>
                            </div>
                        </button>
                    {/each}

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
                                            ? "file"
                                            : "files"})
                                    </span>
                                {/if}
                            </div>
                        </label>
                    {/if}

                    <!-- Action buttons -->
                    <div class="flex justify-end gap-2">
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
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
