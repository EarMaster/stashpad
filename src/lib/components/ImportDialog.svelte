<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { open as openFile } from "@tauri-apps/plugin-dialog";
    import { readFile, readTextFile } from "@tauri-apps/plugin-fs";
    import JSZip from "jszip";
    import type { Context, StashItem } from "$lib/types";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import {
        Upload,
        FileText,
        FileArchive,
        Square,
        CheckSquare,
        AlertTriangle,
        FolderOpen,
    } from "lucide-svelte";

    let {
        open = $bindable(false),
        context,
        existingStashes,
        onClose,
        onImportComplete,
    } = $props<{
        open: boolean;
        context: Context;
        existingStashes: StashItem[];
        onClose: () => void;
        onImportComplete: () => void;
    }>();

    // State for import workflow
    let step = $state<"select" | "preview">("select");
    let parsedStashes = $state<StashItem[]>([]);
    let selectedIds = $state<Set<string>>(new Set());
    let duplicateIds = $state<Set<string>>(new Set());
    let attachmentFiles = $state<Map<string, Uint8Array>>(new Map());
    let isImporting = $state(false);
    let isParsing = $state(false);
    let importedFileName = $state("");

    const adapter = new DesktopStorageAdapter();

    // Reset state when dialog opens
    $effect(() => {
        if (open) {
            step = "select";
            parsedStashes = [];
            selectedIds = new Set();
            duplicateIds = new Set();
            attachmentFiles = new Map();
            isImporting = false;
            isParsing = false;
            importedFileName = "";
        }
    });

    // Derived stats
    let selectedStashes = $derived(
        parsedStashes.filter((s) => selectedIds.has(s.id)),
    );
    let duplicateCount = $derived(
        [...selectedIds].filter((id) => duplicateIds.has(id)).length,
    );

    /**
     * Open file picker and load file
     */
    async function selectFile() {
        const filePath = await openFile({
            title: $_("contexts.importDialog.selectFile"),
            filters: [
                {
                    name: "Stashpad Export",
                    extensions: ["md", "zip"],
                },
            ],
        });

        if (!filePath) return;

        isParsing = true;
        try {
            const fileName = (filePath as string).split(/[\\/]/).pop() || "";
            importedFileName = fileName;

            if (fileName.endsWith(".zip")) {
                await parseZipFile(filePath as string);
            } else {
                await parseMarkdownFile(filePath as string);
            }

            // Detect duplicates
            detectDuplicates();

            // Select non-completed, non-duplicate stashes by default
            selectedIds = new Set(
                parsedStashes
                    .filter((s) => !s.completed && !duplicateIds.has(s.id))
                    .map((s) => s.id),
            );

            step = "preview";
        } catch (e) {
            console.error("Failed to parse file:", e);
        } finally {
            isParsing = false;
        }
    }

    /**
     * Parse a markdown file exported from Stashpad
     */
    async function parseMarkdownFile(filePath: string) {
        const content = await readTextFile(filePath);
        parsedStashes = parseMarkdownContent(content);
    }

    /**
     * Parse a ZIP file exported from Stashpad
     */
    async function parseZipFile(filePath: string) {
        const zipData = await readFile(filePath);
        const zip = await JSZip.loadAsync(zipData);

        // Find and parse export.md
        const mdFile = zip.file("export.md");
        if (mdFile) {
            const content = await mdFile.async("text");
            parsedStashes = parseMarkdownContent(content);
        }

        // Load attachments
        const attachmentsFolder = zip.folder("attachments");
        if (attachmentsFolder) {
            const files = attachmentsFolder.file(/.*/);
            for (const file of files) {
                if (!file.dir) {
                    const data = await file.async("uint8array");
                    const name = file.name.split("/").pop() || file.name;
                    attachmentFiles.set(name, data);
                }
            }
            attachmentFiles = new Map(attachmentFiles); // Trigger reactivity
        }
    }

    /**
     * Parse markdown content into stash items
     * Format: ## [Status] Date
     */
    function parseMarkdownContent(content: string): StashItem[] {
        const stashes: StashItem[] = [];
        const lines = content.split("\n");

        let currentStash: Partial<StashItem> | null = null;
        let currentContent: string[] = [];
        let currentFiles: string[] = [];
        let inAttachments = false;

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];

            // Detect stash header: ## [Status] Date
            const headerMatch = line.match(
                /^## \[(✓ Completed|Active)\] (.+)$/,
            );
            if (headerMatch) {
                // Save previous stash
                if (currentStash) {
                    stashes.push(
                        finalizeStash(
                            currentStash,
                            currentContent,
                            currentFiles,
                        ),
                    );
                }

                // Start new stash
                const isCompleted = headerMatch[1] === "✓ Completed";
                const dateStr = headerMatch[2];
                currentStash = {
                    id: crypto.randomUUID(),
                    completed: isCompleted,
                    createdAt: parseDate(dateStr),
                    contextId: context.id,
                };
                currentContent = [];
                currentFiles = [];
                inAttachments = false;
                continue;
            }

            // Skip if no current stash
            if (!currentStash) continue;

            // Detect attachments section
            if (line.startsWith("**Attachments:**")) {
                inAttachments = true;
                continue;
            }

            // Detect separator
            if (line === "---") {
                inAttachments = false;
                continue;
            }

            // Parse attachment lines
            if (inAttachments && line.startsWith("- ")) {
                const attachMatch = line.match(
                    /^- \[(.+)\]\(attachments\/(.+)\)$/,
                );
                if (attachMatch) {
                    currentFiles.push(attachMatch[2]);
                } else {
                    // Plain attachment reference: - filename.ext
                    const plainMatch = line.match(/^- (.+)$/);
                    if (plainMatch) {
                        currentFiles.push(plainMatch[1]);
                    }
                }
                continue;
            }

            // Regular content line
            if (!inAttachments && line.trim() !== "") {
                currentContent.push(line);
            }
        }

        // Don't forget the last stash
        if (currentStash) {
            stashes.push(
                finalizeStash(currentStash, currentContent, currentFiles),
            );
        }

        return stashes;
    }

    /**
     * Finalize a stash object from parsed data
     */
    function finalizeStash(
        partial: Partial<StashItem>,
        contentLines: string[],
        files: string[],
    ): StashItem {
        return {
            id: partial.id || crypto.randomUUID(),
            content: contentLines.join("\n").trim(),
            files: files,
            createdAt: partial.createdAt || new Date().toISOString(),
            contextId: partial.contextId || context.id,
            completed: partial.completed || false,
            completedAt: partial.completed
                ? new Date().toISOString()
                : undefined,
        };
    }

    /**
     * Parse date string from export format
     */
    function parseDate(dateStr: string): string {
        try {
            const date = new Date(dateStr);
            if (!isNaN(date.getTime())) {
                return date.toISOString();
            }
        } catch {
            // Fall through to default
        }
        return new Date().toISOString();
    }

    /**
     * Detect duplicate stashes by comparing content
     */
    function detectDuplicates() {
        const dupes = new Set<string>();

        for (const parsed of parsedStashes) {
            const normalizedParsed = normalizeContent(parsed.content);
            if (!normalizedParsed) continue;

            for (const existing of existingStashes) {
                const normalizedExisting = normalizeContent(existing.content);
                if (!normalizedExisting) continue;

                // Check for exact match or high similarity
                if (
                    normalizedParsed === normalizedExisting ||
                    calculateSimilarity(normalizedParsed, normalizedExisting) >
                        0.8
                ) {
                    dupes.add(parsed.id);
                    break;
                }
            }
        }

        duplicateIds = dupes;
    }

    /**
     * Normalize content for comparison
     */
    function normalizeContent(content: string): string {
        return content.trim().toLowerCase().replace(/\s+/g, " ");
    }

    /**
     * Simple similarity calculation (Jaccard index on words)
     */
    function calculateSimilarity(a: string, b: string): number {
        const wordsA = new Set(a.split(/\s+/));
        const wordsB = new Set(b.split(/\s+/));

        const intersection = [...wordsA].filter((w) => wordsB.has(w)).length;
        const union = new Set([...wordsA, ...wordsB]).size;

        return union > 0 ? intersection / union : 0;
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
        selectedIds = new Set(selectedIds);
    }

    /**
     * Select all stashes
     */
    function selectAll() {
        selectedIds = new Set(parsedStashes.map((s) => s.id));
    }

    /**
     * Deselect all stashes
     */
    function deselectAll() {
        selectedIds = new Set();
    }

    /**
     * Import selected stashes
     */
    async function handleImport() {
        if (selectedIds.size === 0) return;

        isImporting = true;
        try {
            for (const stash of selectedStashes) {
                // Copy attachments if from ZIP
                const newFiles: string[] = [];
                for (const fileName of stash.files) {
                    const fileData = attachmentFiles.get(fileName);
                    if (fileData) {
                        // Create a File object from Uint8Array and save via adapter
                        // Use ArrayBuffer.slice to ensure correct type
                        const blob = new Blob([
                            fileData.buffer.slice(
                                fileData.byteOffset,
                                fileData.byteOffset + fileData.byteLength,
                            ) as ArrayBuffer,
                        ]);
                        const file = new File([blob], fileName);
                        const savedPath = await adapter.saveAsset(
                            file,
                            context.id,
                            stash.id,
                        );
                        newFiles.push(savedPath);
                    }
                }

                // Update stash with new file paths
                const stashToSave: StashItem = {
                    ...stash,
                    files: newFiles.length > 0 ? newFiles : [],
                    contextId: context.id,
                };

                await adapter.saveStash(stashToSave);
            }

            onImportComplete();
            handleClose();
        } catch (e) {
            console.error("Import failed:", e);
        } finally {
            isImporting = false;
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
                        {$_("contexts.importDialog.title")}
                    </Dialog.Title>
                    <Dialog.Description
                        class="text-sm text-muted-foreground mt-1"
                    >
                        {step === "select"
                            ? $_("contexts.importDialog.selectFileDesc")
                            : $_("contexts.importDialog.selectStashes")}
                    </Dialog.Description>
                </div>

                {#if step === "select"}
                    <!-- File Selection Step -->
                    <div class="p-8 flex flex-col items-center gap-4">
                        <div
                            class="w-16 h-16 rounded-full bg-muted flex items-center justify-center"
                        >
                            <FolderOpen
                                size={32}
                                class="text-muted-foreground"
                            />
                        </div>
                        <p class="text-sm text-muted-foreground text-center">
                            {$_("contexts.importDialog.supportedFormats")}
                        </p>
                        <button
                            type="button"
                            class="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2"
                            onclick={selectFile}
                            disabled={isParsing}
                        >
                            <Upload size={16} />
                            {isParsing
                                ? $_("contexts.importDialog.parsing")
                                : $_("contexts.importDialog.chooseFile")}
                        </button>
                    </div>
                {:else}
                    <!-- Preview Step -->
                    <div
                        class="flex-1 overflow-y-auto p-4 space-y-2 max-h-[40vh]"
                    >
                        {#if duplicateCount > 0}
                            <div
                                class="flex items-center gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-600 dark:text-amber-400 text-sm mb-3"
                            >
                                <AlertTriangle size={16} />
                                <span>
                                    {$_(
                                        "contexts.importDialog.duplicatesFound",
                                        {
                                            values: { count: duplicateCount },
                                        },
                                    )}
                                </span>
                            </div>
                        {/if}

                        {#each parsedStashes as stash (stash.id)}
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
                                        <span
                                            >{formatDate(stash.createdAt)}</span
                                        >
                                        {#if stash.completed}
                                            <span
                                                class="px-1.5 py-0.5 rounded bg-muted text-[10px]"
                                            >
                                                {$_("queue.completed")}
                                            </span>
                                        {/if}
                                        {#if duplicateIds.has(stash.id)}
                                            <span
                                                class="px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-600 dark:text-amber-400 text-[10px] flex items-center gap-1"
                                            >
                                                <AlertTriangle size={10} />
                                                {$_(
                                                    "contexts.importDialog.duplicate",
                                                )}
                                            </span>
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

                        {#if parsedStashes.length === 0}
                            <div
                                class="text-center py-8 text-muted-foreground text-sm"
                            >
                                {$_("contexts.importDialog.noStashesFound")}
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
                                {selectedIds.size} / {parsedStashes.length}
                            </div>
                        </div>

                        <!-- File info -->
                        <div
                            class="flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            {#if importedFileName.endsWith(".zip")}
                                <FileArchive size={14} />
                            {:else}
                                <FileText size={14} />
                            {/if}
                            <span class="truncate">{importedFileName}</span>
                            {#if attachmentFiles.size > 0}
                                <span class="text-muted-foreground">
                                    ({attachmentFiles.size} attachments)
                                </span>
                            {/if}
                        </div>

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
                                onclick={handleImport}
                                disabled={selectedIds.size === 0 || isImporting}
                            >
                                <Upload size={16} />
                                {isImporting
                                    ? $_("common.loading")
                                    : $_("contexts.importDialog.import")}
                            </button>
                        </div>
                    </div>
                {/if}
            </div>
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
