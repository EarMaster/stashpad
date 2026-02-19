<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann
-->

<script lang="ts">
    import { _, locale } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { save } from "@tauri-apps/plugin-dialog";
    import { writeTextFile, readFile, stat } from "@tauri-apps/plugin-fs";
    import JSZip from "jszip";
    import type { Context, StashItem } from "$lib/types";
    import { getRelativeTime } from "$lib/utils/date";
    import { formatBytes } from "$lib/utils/format";
    import { Download, FileArchive, Square, CheckSquare } from "lucide-svelte";
    import { dump } from "js-yaml";

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
    let totalAttachmentSize = $state(0);
    let isCalculatingSize = $state(false);

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
     * Build markdown content from selected stashes
     * @param filenameMap - Optional map of (stashId + fileName) -> uniqueFileName for renamed attachments in ZIPs
     */

    /**
     * Build markdown content from selected stashes
     * @param filenameMap - Optional map of (stashId + fileName) -> uniqueFileName for renamed attachments in ZIPs
     */
    function buildMarkdownContent(filenameMap?: Map<string, string>): string {
        const lines: string[] = [];

        // Create metadata object
        const metadata = {
            name: context.name,
            description: context.description || "",
            rules: context.rules || [],
        };

        // Add YAML frontmatter
        lines.push("---");
        lines.push(dump(metadata).trim());
        lines.push("---");
        lines.push("");

        lines.push(`# ${context.name}`);
        lines.push("");

        // Use ISO 8601 format (YYYY-MM-DD HH:MM:SS)
        const now = new Date();
        const isoDate = now.toISOString().slice(0, 19).replace("T", " ");
        lines.push(`Exported from Stashpad on ${isoDate}`);
        lines.push("");
        lines.push(`Total stashes: ${selectedStashes.length}`);
        lines.push("");
        lines.push("---");
        lines.push("");

        // Group by active and completed (like the queue)
        const activeStashes = selectedStashes
            .filter((s) => !s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            );

        const completedStashes = selectedStashes
            .filter((s) => s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            );

        // Export active stashes first
        if (activeStashes.length > 0) {
            lines.push(`## Active Stashes (${activeStashes.length})`);
            lines.push("");

            for (const stash of activeStashes) {
                const date = new Date(stash.createdAt).toLocaleString();
                lines.push(`### ${date}`);
                lines.push("");

                if (stash.content.trim()) {
                    lines.push(stash.content);
                    lines.push("");
                }

                const hasFiles =
                    (stash.files && stash.files.length > 0) ||
                    (stash.attachments && stash.attachments.length > 0);

                if (hasFiles) {
                    lines.push("**Attachments:**");

                    // Legacy files
                    if (stash.files) {
                        for (const file of stash.files) {
                            const fileName = file.split(/[\\\/]/).pop() || file;
                            const uniqueFileName =
                                filenameMap?.get(`${stash.id}:${fileName}`) ||
                                fileName;
                            if (includeAttachments) {
                                lines.push(
                                    `- [${fileName}](attachments/${uniqueFileName})`,
                                );
                            } else {
                                lines.push(`- ${fileName}`);
                            }
                        }
                    }

                    // New attachments
                    if (stash.attachments) {
                        for (const att of stash.attachments) {
                            const fileName = att.fileName;
                            const uniqueFileName =
                                filenameMap?.get(`${stash.id}:${fileName}`) ||
                                fileName;
                            if (includeAttachments) {
                                lines.push(
                                    `- [${fileName}](attachments/${uniqueFileName})`,
                                );
                            } else {
                                lines.push(`- ${fileName}`);
                            }
                        }
                    }
                    lines.push("");
                }

                lines.push("---");
                lines.push("");
            }
        }

        // Export completed stashes
        if (completedStashes.length > 0) {
            lines.push(`## Completed Stashes (${completedStashes.length})`);
            lines.push("");

            for (const stash of completedStashes) {
                const date = new Date(stash.createdAt).toLocaleString();
                lines.push(`### ${date}`);
                lines.push("");

                if (stash.content.trim()) {
                    lines.push(stash.content);
                    lines.push("");
                }

                const hasFiles =
                    (stash.files && stash.files.length > 0) ||
                    (stash.attachments && stash.attachments.length > 0);

                if (hasFiles) {
                    lines.push("**Attachments:**");

                    // Legacy files
                    if (stash.files) {
                        for (const file of stash.files) {
                            const fileName = file.split(/[\\\/]/).pop() || file;
                            const uniqueFileName =
                                filenameMap?.get(`${stash.id}:${fileName}`) ||
                                fileName;
                            if (includeAttachments) {
                                lines.push(
                                    `- [${fileName}](attachments/${uniqueFileName})`,
                                );
                            } else {
                                lines.push(`- ${fileName}`);
                            }
                        }
                    }

                    // New attachments
                    if (stash.attachments) {
                        for (const att of stash.attachments) {
                            const fileName = att.fileName;
                            const uniqueFileName =
                                filenameMap?.get(`${stash.id}:${fileName}`) ||
                                fileName;
                            if (includeAttachments) {
                                lines.push(
                                    `- [${fileName}](attachments/${uniqueFileName})`,
                                );
                            } else {
                                lines.push(`- ${fileName}`);
                            }
                        }
                    }
                    lines.push("");
                }

                lines.push("---");
                lines.push("");
            }
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
        // Format: YYYY-MM-DD_HH-MM
        const now = new Date();
        const date = now.toISOString().slice(0, 10);
        const time = now.toTimeString().slice(0, 5).replace(":", "-");
        const timestamp = `${date}_${time}`;
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

        // Add attachments folder
        const attachmentsFolder = zip.folder("attachments");

        // Map of "stashId:fileName" -> "prefixedFileName" for all files
        const filenameMap = new Map<string, string>();

        // Collect all files from selected stashes
        for (const stash of selectedStashes) {
            const stashIdShort = stash.id.slice(0, 8);

            // Legacy files
            if (stash.files && stash.files.length > 0) {
                for (const filePath of stash.files) {
                    try {
                        const fileName =
                            filePath.split(/[\\\/]/).pop() || filePath;
                        const fileData = await readFile(filePath);

                        // Always prefix filename with stash ID to prevent collisions
                        const prefixedFileName = `${stashIdShort}_${fileName}`;

                        // Store mapping for markdown generation
                        filenameMap.set(
                            `${stash.id}:${fileName}`,
                            prefixedFileName,
                        );

                        attachmentsFolder?.file(prefixedFileName, fileData);
                    } catch (e) {
                        console.error(`Failed to read file ${filePath}:`, e);
                    }
                }
            }

            // New attachments
            if (stash.attachments && stash.attachments.length > 0) {
                for (const att of stash.attachments) {
                    try {
                        const fileData = await readFile(att.filePath);

                        // Always prefix filename with stash ID to prevent collisions
                        const prefixedFileName = `${stashIdShort}_${att.fileName}`;

                        // Store mapping for markdown generation
                        filenameMap.set(
                            `${stash.id}:${att.fileName}`,
                            prefixedFileName,
                        );

                        attachmentsFolder?.file(prefixedFileName, fileData);
                    } catch (e) {
                        console.error(
                            `Failed to read attachment ${att.filePath}:`,
                            e,
                        );
                    }
                }
            }
        }

        // Generate markdown with filename mappings
        const markdownContent = buildMarkdownContent(filenameMap);

        // Add markdown file
        zip.file("export.md", markdownContent);

        // Generate zip blob
        const zipBlob = await zip.generateAsync({ type: "uint8array" });

        const safeName = context.name
            .replace(/[^a-zA-Z0-9_-]/g, "_")
            .toLowerCase();
        // Format: YYYY-MM-DD_HH-MM
        const now = new Date();
        const date = now.toISOString().slice(0, 10);
        const time = now.toTimeString().slice(0, 5).replace(":", "-");
        const timestamp = `${date}_${time}`;
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
                                        ? totalAttachmentSize +
                                              buildMarkdownContent().length
                                        : buildMarkdownContent().length,
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
